#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use stader_utils::helpers::{query_exchange_rates, send_funds_msg};
use terra_cosmwasm::{create_swap_msg, TerraMsgWrapper};
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, Deps, StdResult, Binary, Addr, Reply, ContractResult, SubMsgExecutionResponse, Uint128, SubMsg, WasmMsg, to_binary, Coin, Decimal, attr};
use crate::msg::{InstantiateMsg, ExecuteMsg, QueryMsg, QueryConfigResponse, QueryStateResponse, GetAirdropMetaResponse};
use crate::ContractError;
use crate::state::{Config, State, CONFIG, STATE, AIRDROP_REGISTRY, POOL_REGISTRY, PoolRegistryInfo, VALIDATOR_REGISTRY, ValInfo, BATCH_UNDELEGATION_REGISTRY, BatchUndelegationRecord, AirdropRate};
use crate::request_validation::{validate, Verify, get_verified_pool, get_validator_for_deposit, get_validator_for_undelegate, create_new_undelegation_batch};
use delegator::msg::ExecuteMsg as DelegatorMsg;
use validator::msg::ExecuteMsg as ValidatorMsg;
use cw_storage_plus::U64Key;
use stader_utils::event_constants::{POOLS_VALIDATOR_EVENT_SWAP_ID, EVENT_SWAP_KEY_AMOUNT, EVENT_KEY_IDENTIFIER};
use std::convert::TryFrom;
use stader_utils::coin_utils::{decimal_summation_in_256, merge_coin_vector, CoinVecOp, Operation, merge_dec_coin_vector, DecCoinVecOp, DecCoin};

pub const MESSAGE_REPLY_SWAP_ID: u64 = 0;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = Config {
        manager: info.sender,
        vault_denom: msg.vault_denom,
        validator_contract: msg.validator_contract,
        delegator_contract: msg.delegator_contract,
        unbonding_period: msg.unbonding_period.unwrap_or(21 * 24 * 3600),
        unbonding_buffer: msg.unbonding_buffer.unwrap_or(3600),
    };
    let state = State { next_pool_id: 0_u64 };
    CONFIG.save(deps.storage, &config)?;
    STATE.save(deps.storage, &state)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    match msg {
        ExecuteMsg::AddPool { name} => add_pool(deps, info, env, name),
        ExecuteMsg::AddValidator { val_addr, pool_id } => add_validator_to_pool(deps, info, env, val_addr, pool_id),
        ExecuteMsg::RemoveValidator { val_addr } => remove_validator_from_pool(deps, info, env, val_addr),
        ExecuteMsg::Deposit { pool_id } => deposit_to_pool(deps, info, env, pool_id),
        ExecuteMsg::RedeemRewards { pool_id } => redeem_rewards(deps, info, env, pool_id),
        ExecuteMsg::Swap { pool_id } => swap(deps, info, env, pool_id),
        ExecuteMsg::QueueUndelegate { pool_id, amount } => queue_user_undelegation(deps, info, env, pool_id, amount),
        ExecuteMsg::Undelegate { pool_id } => undelegate_from_pool(deps, info, env, pool_id),
        ExecuteMsg::ReconcileFunds { pool_id } => reconcile_funds(deps, info, env, pool_id),
        ExecuteMsg::WithdrawFundsToWallet { pool_id, batch_id, undelegate_id, amount } =>
            withdraw_funds_to_wallet(deps, info, env, pool_id, batch_id, undelegate_id, amount),
        ExecuteMsg::UpdateAirdropPointers { airdrop_amount, rates } => update_airdrop_pointers(deps, info, env, airdrop_amount, rates),
    }
}

pub fn add_pool(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    name: String,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager, Verify::NoFunds])?;

    let mut state = STATE.load(deps.storage)?;
    let pool_id = state.next_pool_id;
    let mut pool_meta = PoolRegistryInfo {
        name,
        active: true,
        validators: vec![],
        staked: Uint128::zero(),
        rewards_pointer: Decimal::zero(),
        airdrops_pointer: vec![],
        current_undelegation_batch_id: 0_u64,
        last_reconciled_batch_id: 0_u64,
    };
    POOL_REGISTRY.save(deps.storage, U64Key::new(pool_id), &pool_meta)?;

    create_new_undelegation_batch(deps.storage, env, pool_id, &mut pool_meta)?;

    state.next_pool_id = state.next_pool_id + 1;
    STATE.save(deps.storage, &state);
    Ok(Response::new())
}

// TODO: Add a msg for moving valdator between pools.
pub fn add_validator_to_pool(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager, Verify::NoFunds])?;

    // check if the validator exists in the blockchain
    if deps.querier.query_validator(&val_addr).unwrap().is_none() {
        return Err(ContractError::ValidatorNotDiscoverable {});
    }

    if VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr).unwrap().is_some() {
        return Err(ContractError::ValidatorAssociatedToPool {});
    }


    let pool_meta_opt = POOL_REGISTRY.may_load(deps.storage, U64Key::new(pool_id))?;
    if pool_meta_opt.is_none() {
        return Err(ContractError::PoolNotFound {});
    }
    let mut pool_meta = pool_meta_opt.unwrap();
    if !pool_meta.active {
        return Err(ContractError::PoolInactive {});
    }

    VALIDATOR_REGISTRY.save(deps.storage, &val_addr, &ValInfo {
        pool_id,
        staked: Uint128::zero()
    })?;

    pool_meta.validators.push(val_addr.clone());
    POOL_REGISTRY.save(deps.storage, U64Key::new(pool_id), &pool_meta)?;

    // We will not run into a case where pool already has validator obj but val is not present in VALIDATOR_REGISTRY.
    Ok(Response::new()
        .add_attribute("new_validator", val_addr.to_string())
        .add_attribute("into_pool", pool_id.to_string()))
}

// TODO - GM. Add tests
pub fn remove_validator_from_pool(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    Ok(Response::default())
}

// Any address can call this.
pub fn deposit_to_pool(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::NonZeroSingleInfoFund])?;

    let amount = info.funds.first().unwrap().amount;
    let mut pool_meta = get_verified_pool(deps.storage, pool_id, true)?;
    let user_addr = info.sender;
    let val_addr = get_validator_for_deposit(deps.storage, pool_meta.validators.clone())?;

    VALIDATOR_REGISTRY.update(deps.storage, &val_addr, |val_meta_opt| -> StdResult<_> {
        let mut val_meta = val_meta_opt.unwrap();
        val_meta.staked = val_meta.staked.checked_add(amount).unwrap();
        Ok(val_meta)
    })?;

    pool_meta.staked = pool_meta.staked.checked_add(amount).unwrap();
    POOL_REGISTRY.save(deps.storage, U64Key::new(pool_id), &pool_meta)?;

    let messages = vec![WasmMsg::Execute {
        contract_addr: config.delegator_contract.to_string(),
        msg: to_binary(&DelegatorMsg::Deposit {
            user_addr: user_addr.clone(),
            amount,
            pool_id,
            pool_rewards_pointer: pool_meta.rewards_pointer.clone(),
            pool_airdrops_pointer: pool_meta.airdrops_pointer.clone()
        }).unwrap(),
        funds: vec![]
    }, WasmMsg::Execute {
        contract_addr: config.validator_contract.to_string(),
        msg: to_binary(&ValidatorMsg::Stake {
            val_addr,
        }).unwrap(),
        funds: vec![Coin::new(amount.u128(), config.vault_denom)]
    }];

    Ok(Response::new().add_messages(messages))
}

// Might need to paginate if pool size going to be greater than 10.
pub fn redeem_rewards(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager, Verify::NoFunds])?;

    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;
    let messages = vec![WasmMsg::Execute {
        contract_addr: config.validator_contract.to_string(),
        msg: to_binary(&ValidatorMsg::RedeemRewards {
            validators: pool_meta.validators
        }).unwrap(),
        funds: vec![]
    }];

    Ok(Response::new().add_messages(messages))
}

// Might need to paginate if pool size going to be greater than 10.
pub fn swap(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager, Verify::NoFunds])?;

    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;

    Ok(Response::new().add_submessage(
        SubMsg::reply_always(WasmMsg::Execute {
            contract_addr: config.validator_contract.to_string(),
            msg: to_binary(&ValidatorMsg::SwapAndTransfer {
                validators: pool_meta.validators,
                identifier: pool_id.to_string(),
            }).unwrap(),
            funds: vec![]
        }, MESSAGE_REPLY_SWAP_ID))
    )
}

// Any address can call this fn.
pub fn queue_user_undelegation(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::NoFunds])?;
    let user_addr = info.sender;
    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;

    let current_batch_id = pool_meta.current_undelegation_batch_id;
    let batch_undelegation_registry_id = (U64Key::new(pool_id), U64Key::new(current_batch_id));
    BATCH_UNDELEGATION_REGISTRY.update(
        deps.storage, batch_undelegation_registry_id, |x| -> StdResult<_> {
            let mut batch_undel = x.unwrap();
            batch_undel.amount = batch_undel.amount.checked_add(amount).unwrap();
            Ok(batch_undel)
        })?;

    pool_meta.staked = pool_meta.staked.checked_sub(amount).unwrap();
    POOL_REGISTRY.save(deps.storage, U64Key::from(pool_id), &pool_meta);

    // Fire and forget will work here because if user transaction will fail then tx will fail this state change too.
    let message = WasmMsg::Execute {
        contract_addr: config.delegator_contract.to_string(),
        msg: to_binary(&DelegatorMsg::Undelegate {
            user_addr,
            batch_id: current_batch_id,
            from_pool: pool_id,
            amount,
            pool_rewards_pointer: pool_meta.rewards_pointer.clone(),
            pool_airdrops_pointer: pool_meta.airdrops_pointer.clone()
        }).unwrap(),
        funds: vec![]
    };

    Ok(Response::new().add_message(message))
}

pub fn undelegate_from_pool(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager, Verify::NoFunds])?;

    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;

    // This is because a new batch wuold be created before this message is called, so -1.
    let undelegate_batch_id = pool_meta.current_undelegation_batch_id;
    let batch_undelegation_registry_id = (U64Key::new(pool_id), U64Key::new(undelegate_batch_id));
    let mut undel_amount = Uint128::zero();
    BATCH_UNDELEGATION_REGISTRY.update(
        deps.storage, batch_undelegation_registry_id, |x| -> Result<_, ContractError> {
            let mut batch_undel = x.unwrap();

            if (batch_undel.amount.is_zero()) {
                return Err(ContractError::NoOp {});
            }

            batch_undel.est_release_time = Some(env.block.time.plus_seconds(config.unbonding_period));
            batch_undel.withdrawable_time = Some(env.block.time.plus_seconds(config.unbonding_period + config.unbonding_buffer));
            undel_amount = batch_undel.amount;
            Ok(batch_undel)
        })?;

    let mut messages = vec![];
    let validators = pool_meta.validators.clone();
    let mut to_undelegate = undel_amount;
    while to_undelegate.ne(&Uint128::zero()) {
        let val_addr = get_validator_for_undelegate(deps.storage, validators.clone()).unwrap();
        VALIDATOR_REGISTRY.update(deps.storage, &val_addr.clone(), |x| -> StdResult<_> {
            let mut val_meta = x.unwrap();
            let amount = std::cmp::min(to_undelegate, val_meta.staked);
            println!("amount|{:?}|{:?}", val_addr.clone(), amount);
            messages.push(WasmMsg::Execute {
                contract_addr: config.validator_contract.to_string(),
                msg: to_binary(&ValidatorMsg::Undelegate {
                    val_addr,
                    amount,
                }).unwrap(),
                funds: vec![]
            });

            to_undelegate = to_undelegate.checked_sub(amount).unwrap();
            val_meta.staked = val_meta.staked.checked_sub(amount).unwrap();
            Ok(val_meta)
        }).unwrap();
    }
    pool_meta.staked = pool_meta.staked.checked_sub(undel_amount).unwrap();
    create_new_undelegation_batch(deps.storage, env, pool_id, &mut pool_meta)?;

    Ok(Response::new().add_messages(messages)
        .add_attribute("Undelegation_pool_id", pool_id.to_string())
        .add_attribute("Undelegation_amount", undel_amount.to_string())
    )
}

// TODO - GM. Make this loop through several histories.
pub fn reconcile_funds(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager, Verify::NoFunds])?;

    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;
    let mut total_amount = Uint128::zero();
    let mut last_reconciled_id = pool_meta.last_reconciled_batch_id;
    for batch_id in pool_meta.last_reconciled_batch_id+1..pool_meta.current_undelegation_batch_id+1 {
        let key = (U64Key::new(pool_id), U64Key::new(batch_id));
        let batch_meta = BATCH_UNDELEGATION_REGISTRY.load(deps.storage, key.clone())?;
        if batch_meta.est_release_time.is_none() || batch_meta.est_release_time.unwrap().gt(&env.block.time) {
            break;
        }
        total_amount = total_amount.checked_add(batch_meta.amount).unwrap();
        last_reconciled_id = batch_id;
    }

    pool_meta.last_reconciled_batch_id = last_reconciled_id;
    POOL_REGISTRY.save(deps.storage, U64Key::new(pool_id), &pool_meta)?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: config.validator_contract.to_string(),
        msg: to_binary(&ValidatorMsg::TransferReconciledFunds {
            amount: total_amount,
        }).unwrap(),
        funds: vec![]
    }))
}

// Anyone can call this
pub fn withdraw_funds_to_wallet(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
    batch_id: u64,
    undelegate_id: u64,
    amount: Uint128, // Needed only for bookkeeping.
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::NoFunds])?;

    let user_addr = info.sender;
    let key = (U64Key::from(pool_id), U64Key::from(batch_id));
    let und_opt = BATCH_UNDELEGATION_REGISTRY.may_load(deps.storage, key.clone())?;
    if und_opt.is_none() {
        return Err(ContractError::UndelegationBatchNotFound {});
    }
    let mut und_batch = und_opt.unwrap();
    if und_batch.withdrawable_time.is_none() || und_batch.withdrawable_time.unwrap().gt(&env.block.time) {
        return Err(ContractError::UndelegationNotWithdrawable {});
    }
    und_batch.amount = und_batch.amount.checked_sub(amount).unwrap();
    BATCH_UNDELEGATION_REGISTRY.save(deps.storage, key, &und_batch)?;

    let msg = WasmMsg::Execute {
        contract_addr: config.delegator_contract.to_string(),
        msg: to_binary(&DelegatorMsg::WithdrawFunds {
            user_addr,
            pool_id,
            undelegate_id,
            amount,
        }).unwrap(),
        funds: vec![]
    };
    Ok(Response::new().add_message(msg))
}

// 1. Call Validator.redeem_rewards_before
// 2. Call this msg
pub fn update_airdrop_pointers(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    airdrop_amount: Uint128,
    airdrop_rates: Vec<AirdropRate>
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager, Verify::NoFunds])?;

    if airdrop_amount.is_zero() {
        return Err(ContractError::ZeroAmount {})
    }

    let mut total_amount = Uint128::zero();
    for rate in airdrop_rates {
        POOL_REGISTRY.update(deps.storage, U64Key::new(rate.pool_id), |x| -> StdResult<_> {
            let mut pool_meta = x.unwrap();
            total_amount = total_amount.checked_add(rate.amount).unwrap();
            pool_meta.airdrops_pointer = merge_dec_coin_vector(&pool_meta.airdrops_pointer, DecCoinVecOp {
                fund: vec![DecCoin::new(Decimal::from_ratio(rate.amount, pool_meta.staked), rate.denom)],
                operation: Operation::Add
            });
            Ok(pool_meta)
        })?;
    }
    if total_amount.ne(&airdrop_amount) {
        return Err(ContractError::MismatchingAmounts {});
    }

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<QueryConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(QueryConfigResponse { config: config })
}

pub fn query_state(deps: Deps) -> StdResult<QueryStateResponse> {
    let state: State = STATE.load(deps.storage)?;
    Ok(QueryStateResponse { state: state })
}

pub fn query_airdrop_meta(deps: Deps, token: String) -> StdResult<GetAirdropMetaResponse> {
    let airdrop_meta_opt = AIRDROP_REGISTRY.may_load(deps.storage, token)?;
    Ok(GetAirdropMetaResponse {
        airdrop_meta: airdrop_meta_opt,
    })
}

/**
    SubMessage Signals
*/


#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        // Called for remove_validator clean up.
        MESSAGE_REPLY_SWAP_ID => reply_swap(deps, env, msg.id.into(), msg.result),
        _ => panic!("Cannot find operation id {:?}", msg.id),
    }
}

// Update pool pointer with swapped rewards available in SCC.
pub fn reply_swap(
    deps: DepsMut,
    _env: Env,
    msg_id: u64,
    result: ContractResult<SubMsgExecutionResponse>,
) -> Result<Response, ContractError> {
    if result.is_err() {
        return Err(ContractError::SwapFailed {});
    }

    // TODO - GM. Handle error case as well.
    let res = result.unwrap();
    let mut keys: Vec<String> = vec![];

    for event in res.events.clone() {
        keys.push(event.ty);
    }

    let event_name = format!("wasm-{}", POOLS_VALIDATOR_EVENT_SWAP_ID);
    let event_opt = res
        .events
        .clone()
        .into_iter()
        .find(|x| x.ty.eq(&event_name));
    if event_opt.is_none() {
        return Err(ContractError::EventNotFound {});
    }

    let attrs = event_opt.unwrap().attributes;
    let swap_amount_attr = attrs
        .clone()
        .into_iter()
        .find(|x| x.key.eq(&EVENT_SWAP_KEY_AMOUNT))
        .unwrap();
    let identifier = attrs
        .clone()
        .into_iter()
        .find(|x| x.key.eq(&EVENT_KEY_IDENTIFIER))
        .unwrap();
    let swap_amount = swap_amount_attr.value.parse::<u128>().unwrap();
    let pool_id = identifier.value.parse::<u64>().unwrap();
    POOL_REGISTRY.update(deps.storage, U64Key::new(pool_id), |pool_opt| -> StdResult<_> {
        let mut pool_meta = pool_opt.unwrap();
        pool_meta.rewards_pointer = decimal_summation_in_256(
            pool_meta.rewards_pointer, Decimal::from_ratio(swap_amount, pool_meta.staked));
        Ok(pool_meta)
    })?;
    Ok(Response::new()
        .add_attribute("Swapped_amount", swap_amount.to_string())
        .add_attribute("Pool_id", pool_id.to_string())
    )
}
