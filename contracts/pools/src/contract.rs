use crate::msg::{
    ExecuteMsg, GetAirdropMetaResponse, InstantiateMsg, QueryBatchUndelegationResponse,
    QueryConfigResponse, QueryMsg, QueryPoolResponse, QueryStateResponse,
};
use crate::request_validation::{create_new_undelegation_batch, get_validator_for_deposit, get_verified_pool, validate, Verify, get_active_validators_sorted_by_stake};
use crate::state::{AirdropRate, Config, ConfigUpdateRequest, PoolRegistryInfo, State, AIRDROP_REGISTRY, BATCH_UNDELEGATION_REGISTRY, CONFIG, POOL_REGISTRY, STATE, VALIDATOR_CONTRACTS, REWARD_CONTRACTS, AirdropRegistryInfo};
use crate::ContractError;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Addr, Binary, Coin, ContractResult, Decimal, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdResult, SubMsg, SubMsgExecutionResponse, Uint128, WasmMsg, ReplyOn, CosmosMsg};
use cw_storage_plus::U64Key;
use delegator::msg::ExecuteMsg as DelegatorMsg;
use stader_utils::coin_utils::{
    decimal_summation_in_256, merge_dec_coin_vector, DecCoin, DecCoinVecOp, Operation,
};
use stader_utils::event_constants::{EVENT_KEY_IDENTIFIER, EVENT_SWAP_KEY_AMOUNT, EVENT_SWAP_TYPE};
use std::borrow::BorrowMut;
use terra_cosmwasm::TerraMsgWrapper;
use validator::msg::ExecuteMsg as ValidatorExecuteMsg;
use reward::msg::ExecuteMsg as RewardExecuteMsg;

pub const MESSAGE_REPLY_SWAP_ID: u64 = 0;
pub const MESSAGE_REPLY_VALIDATOR_INST_ID: u64 = 1;
pub const MESSAGE_REPLY_REWARD_INST_ID: u64 = 2;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = Config {
        manager: info.sender.clone(),
        vault_denom: msg.vault_denom,
        delegator_contract: msg.delegator_contract,
        unbonding_period: msg.unbonding_period.unwrap_or(21 * 24 * 3600),
        unbonding_buffer: msg.unbonding_buffer.unwrap_or(3600),

        min_deposit: msg.min_deposit,
        max_deposit: msg.max_deposit,
    };
    let state = State {
        next_pool_id: 0_u64,
    };
    CONFIG.save(deps.storage, &config)?;
    validate(&config, &info, &env, vec![Verify::NoFunds])?;
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
        ExecuteMsg::AddPool { name, validator_contract, reward_contract} =>
            add_pool(deps, info, env, name, validator_contract, reward_contract),
        ExecuteMsg::AddValidator { val_addr, pool_id } => {
            add_validator_to_pool(deps, info, env, val_addr, pool_id)
        }
        ExecuteMsg::RemoveValidator { val_addr, redel_addr, pool_id } => {
            remove_validator_from_pool(deps, info, env, val_addr, redel_addr, pool_id)
        }
        ExecuteMsg::Deposit { pool_id } => deposit_to_pool(deps, info, env, pool_id),
        ExecuteMsg::RedeemRewards { pool_id } => redeem_rewards(deps, info, env, pool_id),
        ExecuteMsg::Swap { pool_id } => swap_rewards(deps, info, env, pool_id),
        ExecuteMsg::SendRewardsToScc { pool_id } => transfer_rewards_to_scc(deps, info, env, pool_id),
        ExecuteMsg::QueueUndelegate { pool_id, amount } => {
            queue_user_undelegation(deps, info, env, pool_id, amount)
        }
        ExecuteMsg::Undelegate { pool_id } => undelegate_from_pool(deps, info, env, pool_id),
        ExecuteMsg::ReconcileFunds { pool_id } => reconcile_funds(deps, info, env, pool_id),
        ExecuteMsg::WithdrawFundsToWallet {
            pool_id,
            batch_id,
            undelegate_id,
            amount,
        } => withdraw_funds_to_wallet(deps, info, env, pool_id, batch_id, undelegate_id, amount),
        ExecuteMsg::UpdateAirdropRegistry {
            airdrop_token,
            airdrop_contract,
            cw20_contract
        } => update_airdrop_registry(deps, info, env, airdrop_token, airdrop_contract, cw20_contract),
        ExecuteMsg::ClaimAirdrops {
            rates,
        } => claim_airdrops(deps, info, env, rates),
        ExecuteMsg::UpdateConfig { config_request } => {
            update_config(deps, info, env, config_request)
        }
    }
}

// Expects to receive instantiated validator and reward contract for each pool.
// Each pool is isolated by delegating from a separate contract.
pub fn add_pool(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    name: String,
    validator_contract: Addr,
    reward_contract: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManager],
    )?;

    if VALIDATOR_CONTRACTS.may_load(deps.storage, &validator_contract)?.is_some() {
        return Err(ContractError::ValidatorContractInUse {});
    }

    if REWARD_CONTRACTS.may_load(deps.storage, &reward_contract)?.is_some() {
        return Err(ContractError::RewardContractInUse {});
    }

    let mut state = STATE.load(deps.storage)?;
    let pool_id = state.next_pool_id;
    let pool_meta = PoolRegistryInfo {
        name,
        active: true,
        validator_contract: validator_contract.clone(),
        reward_contract: reward_contract.clone(),
        validators: vec![],
        staked: Uint128::zero(),
        rewards_pointer: Decimal::zero(),
        airdrops_pointer: vec![],
        current_undelegation_batch_id: 0_u64,
        last_reconciled_batch_id: 0_u64,
    };
    let pool_key = U64Key::new(pool_id);
    POOL_REGISTRY.save(deps.storage, pool_key.clone(), &pool_meta)?;
    VALIDATOR_CONTRACTS.save(deps.storage, &validator_contract.clone(), &pool_id)?;
    REWARD_CONTRACTS.save(deps.storage, &reward_contract.clone(), &pool_id)?;

    create_new_undelegation_batch(
        deps.storage,
        env,
        pool_id,
        pool_meta.to_owned().borrow_mut(),
    )?;

    state.next_pool_id += 1;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: validator_contract.to_string(),
        msg: to_binary(&ValidatorExecuteMsg::SetRewardWithdrawAddress { reward_contract })?,
        funds: vec![]
    }))
}

pub fn add_validator_to_pool(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManager],
    )?;

    // Can still add validators even if pool is inactive. Only deposits are restricted.
    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;
    if pool_meta.validators.contains(&val_addr) {
        return Err(ContractError::ValidatorAssociatedToPool {});
    }

    pool_meta.validators.push(val_addr.clone());
    POOL_REGISTRY.save(deps.storage, U64Key::new(pool_id), &pool_meta)?;

    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: pool_meta.validator_contract.to_string(),
            msg: to_binary(&ValidatorExecuteMsg::AddValidator {
                val_addr: val_addr.clone(),
            })
            .unwrap(),
            funds: vec![],
        })
        .add_attribute("new_validator", val_addr.to_string())
        .add_attribute("into_pool", pool_id.to_string()))
}

pub fn remove_validator_from_pool(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
    redel_addr: Addr,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {

    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;
    if val_addr.eq(&redel_addr) {
        return Err(ContractError::RemoveValidatorsCannotBeSame {});
    }

    if !pool_meta.validators.contains(&val_addr) || !pool_meta.validators.contains(&redel_addr) {
        return Err(ContractError::ValidatorNotAdded {});
    }

    let new_validator_pool = pool_meta
        .validators
        .into_iter()
        .filter(|x| x.ne(&val_addr))
        .collect::<Vec<Addr>>();

    pool_meta.validators = new_validator_pool;
    POOL_REGISTRY.save(deps.storage, U64Key::from(pool_id), &pool_meta)?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: pool_meta.validator_contract.to_string(),
        msg: to_binary(&ValidatorExecuteMsg::RemoveValidator {
            val_addr: val_addr,
            redelegate_addr: redel_addr,
        })
            .unwrap(),
        funds: vec![],
    }))
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
    if amount.gt(&config.max_deposit) {
        return Err(ContractError::MaxDeposit {});
    }
    if amount.lt(&config.min_deposit) {
        return Err(ContractError::MinDeposit {});
    }
    let mut pool_meta = get_verified_pool(deps.storage, pool_id, true)?;
    let user_addr = info.sender;
    let val_addr = get_validator_for_deposit(deps.storage, deps.querier, env, pool_meta.validators.clone())?;

    pool_meta.staked = pool_meta.staked.checked_add(amount).unwrap();
    POOL_REGISTRY.save(deps.storage, U64Key::new(pool_id), &pool_meta)?;

    let messages = vec![
        WasmMsg::Execute {
            contract_addr: config.delegator_contract.to_string(),
            msg: to_binary(&DelegatorMsg::Deposit {
                user_addr,
                amount,
                pool_id,
                pool_rewards_pointer: pool_meta.rewards_pointer,
                pool_airdrops_pointer: pool_meta.airdrops_pointer,
            })
            .unwrap(),
            funds: vec![],
        },
        WasmMsg::Execute {
            contract_addr: pool_meta.validator_contract.to_string(),
            msg: to_binary(&ValidatorExecuteMsg::Stake { val_addr }).unwrap(),
            funds: vec![Coin::new(amount.u128(), config.vault_denom)],
        },
    ];

    Ok(Response::new().add_messages(messages))
}

// Would this call fail when a validator is jailed?
pub fn redeem_rewards(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManager],
    )?;

    let pool_meta = get_verified_pool(deps.storage, pool_id, false)?;
    let messages = vec![WasmMsg::Execute {
        contract_addr: pool_meta.validator_contract.to_string(),
        msg: to_binary(&ValidatorExecuteMsg::RedeemRewards {
            validators: pool_meta.validators,
        })
        .unwrap(),
        funds: vec![],
    }];

    Ok(Response::new().add_messages(messages))
}

// Might need to paginate if pool size going to be greater than 10.
pub fn swap_rewards(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManager],
    )?;

    let pool_meta = get_verified_pool(deps.storage, pool_id, false)?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: pool_meta.reward_contract.to_string(),
        msg: to_binary(&RewardExecuteMsg::Swap {})?,
        funds: vec![]
    }))
}

pub fn transfer_rewards_to_scc(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManager],
    )?;

    let pool_meta = get_verified_pool(deps.storage, pool_id, false)?;
    let balance = deps.querier.query_balance(pool_meta.reward_contract.to_string(), config.vault_denom)?;

    if balance.amount.is_zero() {
        return Err(ContractError::ZeroRewards {});
    }

    POOL_REGISTRY.update(
        deps.storage,
        U64Key::new(pool_id),
        |pool_opt| -> StdResult<_> {
            let mut pool_meta = pool_opt.unwrap();
            pool_meta.rewards_pointer = decimal_summation_in_256(
                pool_meta.rewards_pointer,
                Decimal::from_ratio(balance.amount, pool_meta.staked),
            );
            Ok(pool_meta)
        },
    )?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: pool_meta.reward_contract.to_string(),
        msg: to_binary(&RewardExecuteMsg::Transfer { amount: balance.amount })?,
        funds: vec![]
    }))
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

    let user_addr = info.sender;
    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;

    let current_batch_id = pool_meta.current_undelegation_batch_id;
    let batch_undelegation_registry_id = (U64Key::new(pool_id), U64Key::new(current_batch_id));
    BATCH_UNDELEGATION_REGISTRY.update(
        deps.storage,
        batch_undelegation_registry_id,
        |x| -> StdResult<_> {
            let mut batch_undel = x.unwrap();
            batch_undel.amount = batch_undel.amount.checked_add(amount).unwrap();
            Ok(batch_undel)
        },
    )?;

    // We can subtract pool staked amount here so users won't get rewards for this epoch. But
    // we choose not to. Essentially every epoch users will get slightly less rewards because deposits
    // happening in that epoch will be treated as if they were deposited from the beginning of epoch.

    // Fire and forget will work here because if user transaction will fail then tx will fail this state change too.
    let message = WasmMsg::Execute {
        contract_addr: config.delegator_contract.to_string(),
        msg: to_binary(&DelegatorMsg::Undelegate {
            user_addr,
            batch_id: current_batch_id,
            from_pool: pool_id,
            amount,
            pool_rewards_pointer: pool_meta.rewards_pointer,
            pool_airdrops_pointer: pool_meta.airdrops_pointer,
        })
        .unwrap(),
        funds: vec![],
    };

    Ok(Response::new().add_message(message))
}

// TODO - SLASHING CHANGES
pub fn undelegate_from_pool(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;

    // This is because a new batch wuold be created before this message is called, so -1.
    let undelegate_batch_id = pool_meta.current_undelegation_batch_id;
    let batch_undelegation_registry_id = (U64Key::new(pool_id), U64Key::new(undelegate_batch_id));
    let mut undel_amount = Uint128::zero();
    BATCH_UNDELEGATION_REGISTRY.update(
        deps.storage,
        batch_undelegation_registry_id,
        |x| -> Result<_, ContractError> {
            let mut batch_undel = x.unwrap();

            if batch_undel.amount.is_zero() {
                return Err(ContractError::NoOp {});
            }

            batch_undel.est_release_time =
                Some(env.block.time.plus_seconds(config.unbonding_period));
            batch_undel.withdrawable_time = Some(
                env.block
                    .time
                    .plus_seconds(config.unbonding_period + config.unbonding_buffer),
            );
            undel_amount = batch_undel.amount;
            Ok(batch_undel)
        },
    )?;

    let mut messages = vec![];
    let validators = pool_meta.validators.clone();
    let mut to_undelegate = undel_amount;
    let stake_tuples = get_active_validators_sorted_by_stake(deps.storage, deps.querier, env.clone(), validators.clone())?;

    for index in (0..stake_tuples.len()).rev() {
        let tuple_val = stake_tuples.get(index).unwrap();
        if to_undelegate.is_zero() {
            break;
        }
        let val_addr = Addr::unchecked(tuple_val.clone().1);
        let amount = std::cmp::min(to_undelegate, tuple_val.clone().0);
        messages.push(WasmMsg::Execute {
            contract_addr: pool_meta.validator_contract.to_string(),
            msg: to_binary(&ValidatorExecuteMsg::Undelegate { val_addr, amount }).unwrap(),
            funds: vec![],
        });

        to_undelegate = to_undelegate.checked_sub(amount).unwrap();
    }

    if !to_undelegate.is_zero() {
        return Err(ContractError::InSufficientFunds {});
    }

    pool_meta.staked = pool_meta.staked.checked_sub(undel_amount).unwrap();
    create_new_undelegation_batch(deps.storage, env, pool_id, &mut pool_meta)?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("Undelegation_pool_id", pool_id.to_string())
        .add_attribute("Undelegation_amount", undel_amount.to_string()))
}

pub fn reconcile_funds(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;
    let mut total_amount = Uint128::zero();
    let mut last_reconciled_id = pool_meta.last_reconciled_batch_id;
    for batch_id in
        pool_meta.last_reconciled_batch_id + 1..pool_meta.current_undelegation_batch_id + 1
    {
        let key = (U64Key::new(pool_id), U64Key::new(batch_id));
        let batch_meta = BATCH_UNDELEGATION_REGISTRY.load(deps.storage, key.clone())?;
        if batch_meta.est_release_time.is_none()
            || batch_meta.est_release_time.unwrap().gt(&env.block.time)
        {
            break;
        }
        total_amount = total_amount.checked_add(batch_meta.amount).unwrap();
        last_reconciled_id = batch_id;
    }

    pool_meta.last_reconciled_batch_id = last_reconciled_id;
    POOL_REGISTRY.save(deps.storage, U64Key::new(pool_id), &pool_meta)?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: pool_meta.validator_contract.to_string(),
        msg: to_binary(&ValidatorExecuteMsg::TransferReconciledFunds {
            amount: total_amount,
        })
        .unwrap(),
        funds: vec![],
    }))
}

// Anyone can call this
// TODO - GM. Slashing changes?
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
    if und_batch.withdrawable_time.is_none()
        || und_batch.withdrawable_time.unwrap().gt(&env.block.time)
    {
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
        })
        .unwrap(),
        funds: vec![],
    };
    Ok(Response::new().add_message(msg))
}


pub fn update_airdrop_registry(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    airdrop_token: String,
    airdrop_contract: Addr,
    cw20_contract: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;
    AIRDROP_REGISTRY.save(
        deps.storage,
        airdrop_token,
        &AirdropRegistryInfo {
            airdrop_contract,
            cw20_contract,
        },
    )?;

    Ok(Response::default())
}

pub fn claim_airdrops(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    airdrop_rates: Vec<AirdropRate>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    let mut msgs = vec![];
    let mut failed_pools = vec![];
    for rate in airdrop_rates {

        let airdrop_info_opt = AIRDROP_REGISTRY.may_load(deps.storage, rate.denom.clone())?;
        if airdrop_info_opt.is_none() {
            return Err(ContractError::AirdropNotRegistered {});
        }

        let AirdropRegistryInfo { airdrop_contract, cw20_contract } = airdrop_info_opt.unwrap();
        let pool_meta_opt = POOL_REGISTRY.may_load(deps.storage, U64Key::new(rate.pool_id))?;
        if pool_meta_opt.is_none() {
            failed_pools.push(rate.pool_id.to_string());
            continue;
        }
        let mut pool_meta = pool_meta_opt.unwrap();
        pool_meta.airdrops_pointer = merge_dec_coin_vector(
            &pool_meta.airdrops_pointer,
            DecCoinVecOp {
                fund: vec![DecCoin::new(
                    Decimal::from_ratio(rate.amount, pool_meta.staked),
                    rate.denom,
                )],
                operation: Operation::Add,
            },
        );

        msgs.push(WasmMsg::Execute {
            contract_addr: pool_meta.validator_contract.to_string(),
            msg: to_binary(&ValidatorExecuteMsg::RedeemAirdropAndTransfer {
                amount: rate.amount,
                claim_msg: rate.claim_msg,
                airdrop_contract,
                cw20_contract,
            })
                .unwrap(),
            funds: vec![],
        });
        POOL_REGISTRY.save(deps.storage, U64Key::new(rate.pool_id), &pool_meta)?;
    }

    Ok(Response::new().add_messages(msgs))
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    update_config: ConfigUpdateRequest,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManager, Verify::NoFunds],
    )?;

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.delegator_contract = update_config
            .delegator_contract
            .unwrap_or(config.delegator_contract);
        config.unbonding_period = update_config
            .unbonding_period
            .unwrap_or(config.unbonding_period);
        config.unbonding_buffer = update_config
            .unbonding_buffer
            .unwrap_or(config.unbonding_buffer);
        config.min_deposit = update_config.min_deposit.unwrap_or(config.min_deposit);
        config.max_deposit = update_config.max_deposit.unwrap_or(config.max_deposit);
        Ok(config)
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::Pool { pool_id } => to_binary(&query_pool(deps, pool_id)?),
        // QueryMsg::ValidatorInPool { val_addr, pool_id } => to_binary(&query_validator(deps, val_addr, pool_id)?),
        QueryMsg::BatchUndelegation { pool_id, batch_id } => {
            to_binary(&query_batch_undelegate(deps, pool_id, batch_id)?)
        }
    }
}

pub fn query_config(deps: Deps) -> StdResult<QueryConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(QueryConfigResponse { config })
}

pub fn query_state(deps: Deps) -> StdResult<QueryStateResponse> {
    let state: State = STATE.load(deps.storage)?;
    Ok(QueryStateResponse { state })
}

pub fn query_pool(deps: Deps, pool_id: u64) -> StdResult<QueryPoolResponse> {
    let pool_meta = POOL_REGISTRY.may_load(deps.storage, U64Key::new(pool_id))?;
    Ok(QueryPoolResponse { pool: pool_meta })
}

// pub fn query_validator_in_pool(deps: Deps, val_addr: Addr, pool_id: u64) -> StdResult<QueryValidatorResponse> {
//     let val_meta = VALIDATOR_REGISTRY.may_load(deps.storage, (&val_addr, U64Key::new(pool_id)))?;
//     Ok(QueryValidatorResponse { val: val_meta })
// }

pub fn query_batch_undelegate(
    deps: Deps,
    pool_id: u64,
    batch_id: u64,
) -> StdResult<QueryBatchUndelegationResponse> {
    let batch_meta = BATCH_UNDELEGATION_REGISTRY
        .may_load(deps.storage, (U64Key::new(pool_id), U64Key::new(batch_id)))?;
    Ok(QueryBatchUndelegationResponse { batch: batch_meta })
}

pub fn query_airdrop_meta(deps: Deps, token: String) -> StdResult<GetAirdropMetaResponse> {
    let airdrop_meta_opt = AIRDROP_REGISTRY.may_load(deps.storage, token)?;
    Ok(GetAirdropMetaResponse {
        airdrop_meta: airdrop_meta_opt,
    })
}

// #[cfg_attr(not(feature = "library"), entry_point)]
// pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
//     match msg.id {
//         // Called for remove_validator clean up.
//         MESSAGE_REPLY_SWAP_ID => reply_swap(deps, env, msg.id, msg.result),
//         // MESSAGE_REPLY_REWARD_INST_ID => reply_instantiate_reward(deps, env, msg.id, msg.result),
//         _ => panic!("Cannot find operation id {:?}", msg.id),
//     }
// }
//
// // Update pool pointer with swapped rewards available in SCC.
// pub fn reply_swap(
//     deps: DepsMut,
//     _env: Env,
//     _msg_id: u64,
//     result: ContractResult<SubMsgExecutionResponse>,
// ) -> Result<Response, ContractError> {
//     if result.is_err() {
//         return Err(ContractError::SwapFailed {});
//     }
//
//     // TODO - GM. Handle error case as well.
//     let res = result.unwrap();
//     let mut keys: Vec<String> = vec![];
//
//     for event in res.events.clone() {
//         keys.push(event.ty);
//     }
//
//     let event_name = format!("wasm-{}", EVENT_SWAP_TYPE);
//     let event_opt = res
//         .events
//         .clone()
//         .into_iter()
//         .find(|x| x.ty.eq(&event_name));
//     if event_opt.is_none() {
//         return Err(ContractError::EventNotFound {});
//     }
//
//     let attrs = event_opt.unwrap().attributes;
//     let swap_amount_attr = attrs
//         .clone()
//         .into_iter()
//         .find(|x| x.key.eq(&EVENT_SWAP_KEY_AMOUNT))
//         .unwrap();
//     let identifier = attrs
//         .clone()
//         .into_iter()
//         .find(|x| x.key.eq(&EVENT_KEY_IDENTIFIER))
//         .unwrap();
//     let swap_amount = swap_amount_attr.value.parse::<u128>().unwrap();
//     let pool_id = identifier.value.parse::<u64>().unwrap();
//     POOL_REGISTRY.update(
//         deps.storage,
//         U64Key::new(pool_id),
//         |pool_opt| -> StdResult<_> {
//             let mut pool_meta = pool_opt.unwrap();
//             pool_meta.rewards_pointer = decimal_summation_in_256(
//                 pool_meta.rewards_pointer,
//                 Decimal::from_ratio(swap_amount, pool_meta.staked),
//             );
//             Ok(pool_meta)
//         },
//     )?;
//     Ok(Response::new()
//         .add_attribute("Swapped_amount", swap_amount.to_string())
//         .add_attribute("Pool_id", pool_id.to_string()))
// }
