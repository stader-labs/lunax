use crate::msg::{
    ExecuteMsg, GetAirdropMetaResponse, GetValMetaResponse, InstantiateMsg, MerkleAirdropMsg,
    QueryBatchUndelegationResponse, QueryConfigResponse, QueryMsg, QueryPoolResponse,
    QueryStateResponse,
};
use crate::request_validation::{
    create_new_undelegation_batch, get_active_validators_sorted_by_stake,
    get_validator_for_deposit, get_verified_pool, validate, Verify,
};
use crate::state::{
    AirdropRate, AirdropRegistryInfo, AirdropTransferRequest, Config, ConfigUpdateRequest,
    PoolConfigUpdateRequest, PoolRegistryInfo, State, VMeta, AIRDROP_REGISTRY,
    BATCH_UNDELEGATION_REGISTRY, CONFIG, POOL_REGISTRY, REWARD_CONTRACTS, STATE,
    VALIDATOR_CONTRACTS, VALIDATOR_META,
};
use crate::ContractError;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Coin, Decimal, Deps, DepsMut, Env, MessageInfo, QueryRequest,
    Response, StdResult, Uint128, WasmMsg, WasmQuery,
};
use cw20::{BalanceResponse as Cw20BalanceResponse, Cw20QueryMsg};
use cw_storage_plus::U64Key;
use delegator::msg::QueryMsg as DelegatorQueryMsg;
use delegator::msg::{ExecuteMsg as DelegatorExecuteMsg, UserPoolResponse};
use delegator::state::PoolPointerInfo;
use reward::msg::ExecuteMsg as RewardExecuteMsg;
use stader_utils::coin_utils::{
    decimal_division_in_256, decimal_multiplication_in_256, decimal_summation_in_256,
    get_decimal_from_uint128, merge_dec_coin_vector, multiply_u128_with_decimal,
    uint128_from_decimal, DecCoin, DecCoinVecOp, Operation,
};
use std::borrow::BorrowMut;
use terra_cosmwasm::TerraMsgWrapper;
use validator::msg::ExecuteMsg as ValidatorExecuteMsg;
use validator::msg::QueryMsg as ValidatorQueryMsg;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = Config {
        manager: info.sender.clone(),
        vault_denom: "uluna".to_string(),
        delegator_contract: deps.api.addr_validate(msg.delegator_contract.as_str())?,
        scc_contract: deps.api.addr_validate(msg.scc_contract.as_str())?,
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
        ExecuteMsg::AddPool {
            name,
            validator_contract,
            reward_contract,
            protocol_fee_contract,
            protocol_fee_percent,
        } => add_pool(
            deps,
            info,
            env,
            name,
            validator_contract,
            reward_contract,
            protocol_fee_contract,
            protocol_fee_percent,
        ),
        ExecuteMsg::AddValidator { val_addr, pool_id } => {
            add_validator_to_pool(deps, info, env, val_addr, pool_id)
        }
        ExecuteMsg::RemoveValidator {
            val_addr,
            redel_addr,
            pool_id,
        } => remove_validator_from_pool(deps, info, env, val_addr, redel_addr, pool_id),
        ExecuteMsg::RebalancePool {
            pool_id,
            amount,
            val_addr,
            redel_addr,
        } => rebalance_pool(deps, info, env, pool_id, amount, val_addr, redel_addr),
        ExecuteMsg::Deposit { pool_id } => deposit_to_pool(deps, info, env, pool_id),
        ExecuteMsg::RedeemRewards { pool_id } => redeem_rewards(deps, info, env, pool_id),
        ExecuteMsg::Swap { pool_id } => swap_rewards(deps, info, env, pool_id),
        ExecuteMsg::SendRewardsToScc { pool_id } => {
            transfer_rewards_to_scc(deps, info, env, pool_id)
        }
        ExecuteMsg::QueueUndelegate { pool_id, amount } => {
            queue_user_undelegation(deps, info, env, pool_id, amount)
        }
        ExecuteMsg::Undelegate { pool_id } => undelegate_from_pool(deps, info, env, pool_id),
        ExecuteMsg::ReconcileFunds { pool_id } => reconcile_funds(deps, info, env, pool_id),
        ExecuteMsg::WithdrawFundsToWallet {
            pool_id,
            batch_id,
            undelegate_id,
        } => withdraw_funds_to_wallet(deps, info, env, pool_id, batch_id, undelegate_id),
        ExecuteMsg::UpdateAirdropRegistry {
            airdrop_token,
            airdrop_contract,
            cw20_contract,
        } => update_airdrop_registry(
            deps,
            info,
            env,
            airdrop_token,
            airdrop_contract,
            cw20_contract,
        ),
        ExecuteMsg::ClaimAirdrops { rates } => claim_airdrops(deps, info, env, rates),
        ExecuteMsg::UpdateAirdropPointers { transfers } => {
            update_airdrop_pointers(deps, info, env, transfers)
        }
        ExecuteMsg::UpdateConfig { config_request } => {
            update_config(deps, info, env, config_request)
        }
        ExecuteMsg::UpdatePoolMetadata {
            pool_id,
            pool_config_update_request,
        } => update_pool_metadata(deps, info, env, pool_id, pool_config_update_request),
    }
}

// Expects to receive instantiated validator and reward contract for each pool.
// Each pool is isolated by delegating from a separate contract.
#[allow(clippy::too_many_arguments)]
pub fn add_pool(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    name: String,
    validator_contract_str: String,
    reward_contract_str: String,
    protocol_fee_contract_str: String,
    protocol_fee_percent: Decimal,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    let validator_contract = deps.api.addr_validate(validator_contract_str.as_str())?;
    let reward_contract = deps.api.addr_validate(reward_contract_str.as_str())?;
    let protocol_fee_contract = deps.api.addr_validate(protocol_fee_contract_str.as_str())?;

    if VALIDATOR_CONTRACTS
        .may_load(deps.storage, &validator_contract)?
        .is_some()
    {
        return Err(ContractError::ValidatorContractInUse {});
    }

    if REWARD_CONTRACTS
        .may_load(deps.storage, &reward_contract)?
        .is_some()
    {
        return Err(ContractError::RewardContractInUse {});
    }

    let mut state = STATE.load(deps.storage)?;
    let pool_id = state.next_pool_id;
    let mut pool_meta = PoolRegistryInfo {
        name,
        active: true,
        validator_contract: validator_contract.clone(),
        reward_contract: reward_contract.clone(),
        protocol_fee_contract,
        protocol_fee_percent,
        validators: vec![],
        staked: Uint128::zero(),
        rewards_pointer: Decimal::zero(),
        airdrops_pointer: vec![],
        slashing_pointer: Decimal::one(),
        current_undelegation_batch_id: 0_u64,
        last_reconciled_batch_id: 0_u64,
    };
    let pool_key = U64Key::new(pool_id);
    POOL_REGISTRY.save(deps.storage, pool_key, &pool_meta)?;
    VALIDATOR_CONTRACTS.save(deps.storage, &validator_contract, &pool_id)?;
    REWARD_CONTRACTS.save(deps.storage, &reward_contract, &pool_id)?;

    create_new_undelegation_batch(deps.storage, env, pool_id, pool_meta.borrow_mut())?;

    state.next_pool_id += 1;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: validator_contract.to_string(),
        msg: to_binary(&ValidatorExecuteMsg::SetRewardWithdrawAddress { reward_contract })?,
        funds: vec![],
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
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    // Can still add validators even if pool is inactive. Only deposits are restricted.
    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;

    let vmeta_key = (val_addr.clone(), U64Key::new(pool_id));
    if VALIDATOR_META.has(deps.storage, vmeta_key.clone()) {
        return Err(ContractError::ValidatorAssociatedToPool {});
    }

    pool_meta.validators.push(val_addr.clone());
    VALIDATOR_META.save(
        deps.storage,
        vmeta_key,
        &VMeta {
            staked: Uint128::zero(),
            slashed: Uint128::zero(),
            filled: Uint128::zero(),
        },
    )?;
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
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
    redel_addr: Addr,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    check_slashing(&mut deps, env, pool_id)?;

    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;
    if val_addr.eq(&redel_addr) {
        return Err(ContractError::ValidatorsCannotBeSame {});
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

    // Update validator tracking amounts
    let val_delegation = deps
        .querier
        .query_delegation(pool_meta.validator_contract.clone(), val_addr.clone())?;
    if val_delegation.is_some() {
        let full_delegation = val_delegation.unwrap();
        VALIDATOR_META.update(
            deps.storage,
            (redel_addr.clone(), U64Key::new(pool_id)),
            |x| -> StdResult<_> {
                let mut redelegate_val_meta = x.unwrap();
                redelegate_val_meta.staked = redelegate_val_meta
                    .staked
                    .checked_add(full_delegation.amount.amount)?;
                Ok(redelegate_val_meta)
            },
        )?;
    }
    VALIDATOR_META.remove(deps.storage, (val_addr.clone(), U64Key::new(pool_id)));

    POOL_REGISTRY.save(deps.storage, U64Key::from(pool_id), &pool_meta)?;
    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: pool_meta.validator_contract.to_string(),
        msg: to_binary(&ValidatorExecuteMsg::RemoveValidator {
            val_addr,
            redelegate_addr: redel_addr,
        })
        .unwrap(),
        funds: vec![],
    }))
}

pub fn rebalance_pool(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
    amount: Uint128,
    val_addr: Addr,
    redel_addr: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    check_slashing(&mut deps, env, pool_id)?;
    let pool_meta = get_verified_pool(deps.storage, pool_id, false)?;
    if val_addr.eq(&redel_addr) {
        return Err(ContractError::ValidatorsCannotBeSame {});
    }

    if !pool_meta.validators.contains(&val_addr) || !pool_meta.validators.contains(&redel_addr) {
        return Err(ContractError::ValidatorNotAdded {});
    }

    // Fund sufficieny for redelegation is checked by validator contract.
    // We only update the tracking info as no other pool level info changes here.

    // Update validator tracking amounts
    VALIDATOR_META.update(
        deps.storage,
        (val_addr.clone(), U64Key::new(pool_id)),
        |x| -> StdResult<_> {
            let mut src_val_meta = x.unwrap();
            src_val_meta.staked = src_val_meta.staked.checked_sub(amount)?;
            Ok(src_val_meta)
        },
    )?;
    VALIDATOR_META.update(
        deps.storage,
        (redel_addr.clone(), U64Key::new(pool_id)),
        |x| -> StdResult<_> {
            let mut redelegate_val_meta = x.unwrap();
            redelegate_val_meta.staked = redelegate_val_meta.staked.checked_add(amount)?;
            Ok(redelegate_val_meta)
        },
    )?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: pool_meta.validator_contract.to_string(),
        msg: to_binary(&ValidatorExecuteMsg::Redelegate {
            src: val_addr,
            dst: redel_addr,
            amount,
        })
        .unwrap(),
        funds: vec![],
    }))
}

// Modifies pool object. So re-fetch after this call is done.
pub fn check_slashing(
    deps: &mut DepsMut,
    _env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;
    let mut staked_amount_from_blockchain = Uint128::zero();
    for val_addr in pool_meta.validators.clone() {
        let delegation_opt = deps
            .querier
            .query_delegation(pool_meta.validator_contract.clone(), val_addr.clone())?;
        if delegation_opt.is_none() {
            continue;
        } else {
            let full_delegation = delegation_opt.unwrap();
            staked_amount_from_blockchain = staked_amount_from_blockchain
                .checked_add(full_delegation.amount.amount)
                .unwrap();
            // Update validator tracking information.
            VALIDATOR_META.update(
                deps.storage,
                (val_addr.clone(), U64Key::new(pool_id)),
                |x| -> StdResult<_> {
                    let mut val_meta = x.unwrap();

                    if val_meta.staked.gt(&full_delegation.amount.amount) {
                        val_meta.slashed = val_meta.slashed.checked_add(
                            val_meta
                                .staked
                                .checked_sub(full_delegation.amount.amount)
                                .unwrap(),
                        )?;
                        val_meta.staked = full_delegation.amount.amount;
                    }

                    Ok(val_meta)
                },
            )?;
        }
    }
    // greater than condition eliminates stake being zero edge-case handling.
    if pool_meta.staked.gt(&staked_amount_from_blockchain) {
        // Slashing occured. So adjust pool slashing pointers.
        let ratio_funds = decimal_division_in_256(
            get_decimal_from_uint128(staked_amount_from_blockchain),
            get_decimal_from_uint128(pool_meta.staked),
        );

        pool_meta.slashing_pointer =
            decimal_multiplication_in_256(ratio_funds, pool_meta.slashing_pointer);
        pool_meta.staked = staked_amount_from_blockchain;
    }

    POOL_REGISTRY.save(deps.storage, U64Key::new(pool_id), &pool_meta)?;

    let current_undelegation_batch = pool_meta.current_undelegation_batch_id;
    BATCH_UNDELEGATION_REGISTRY.update(
        deps.storage,
        (
            U64Key::new(pool_id),
            U64Key::new(current_undelegation_batch),
        ),
        |batch_opt| -> StdResult<_> {
            let mut batch = batch_opt.unwrap();

            // Slashing pointers have been updated => Recalculate undelegation amount because technically
            // all users who have queued their undelegation requests for the current undelegation batch
            // are affected as well (as funds have actually not been undelegated).
            if pool_meta
                .slashing_pointer
                .lt(&batch.last_updated_slashing_pointer)
            {
                let ratio = decimal_division_in_256(
                    pool_meta.slashing_pointer,
                    batch.last_updated_slashing_pointer,
                );
                batch.prorated_amount = decimal_multiplication_in_256(batch.prorated_amount, ratio);
                batch.last_updated_slashing_pointer = pool_meta.slashing_pointer;
            }

            Ok(batch)
        },
    )?;

    Ok(Response::default())
}

// Any address can call this.
pub fn deposit_to_pool(
    mut deps: DepsMut,
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

    // Formula wise - we want to recompute user balance because slashing pointer has changed and then
    // add the money user wants to delegate. Money being added in this message should be considered post slashing.
    check_slashing(&mut deps, env, pool_id)?;

    let mut pool_meta = get_verified_pool(deps.storage, pool_id, true)?;
    let user_addr = info.sender;
    let val_addr = get_validator_for_deposit(
        deps.querier,
        pool_meta.validator_contract.clone(),
        pool_meta.validators.clone(),
    )?;

    pool_meta.staked = pool_meta.staked.checked_add(amount).unwrap();
    POOL_REGISTRY.save(deps.storage, U64Key::new(pool_id), &pool_meta)?;
    VALIDATOR_META.update(
        deps.storage,
        (val_addr.clone(), U64Key::new(pool_id)),
        |meta| -> StdResult<_> {
            let mut vmeta = meta.unwrap();
            vmeta.staked = vmeta.staked.checked_add(amount)?;
            Ok(vmeta)
        },
    )?;

    let messages = vec![
        WasmMsg::Execute {
            contract_addr: config.delegator_contract.to_string(),
            msg: to_binary(&DelegatorExecuteMsg::Deposit {
                user_addr,
                amount,
                pool_id,
                pool_rewards_pointer: pool_meta.rewards_pointer,
                pool_airdrops_pointer: pool_meta.airdrops_pointer,
                pool_slashing_pointer: pool_meta.slashing_pointer,
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
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    check_slashing(&mut deps, env, pool_id)?;

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
// No need for slashing checking.
pub fn swap_rewards(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    let pool_meta = get_verified_pool(deps.storage, pool_id, false)?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: pool_meta.reward_contract.to_string(),
        msg: to_binary(&RewardExecuteMsg::Swap {})?,
        funds: vec![],
    }))
}

// no need for slashing check
pub fn transfer_rewards_to_scc(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    let pool_meta = get_verified_pool(deps.storage, pool_id, false)?;
    let balance = deps
        .querier
        .query_balance(pool_meta.reward_contract.to_string(), config.vault_denom)?;

    let protocol_fee_amount = Uint128::new(multiply_u128_with_decimal(
        balance.amount.u128(),
        pool_meta.protocol_fee_percent,
    ));
    let rewards_transfer_amount = balance.amount.checked_sub(protocol_fee_amount).unwrap();

    if rewards_transfer_amount.is_zero() {
        return Err(ContractError::ZeroRewards {});
    }

    POOL_REGISTRY.update(
        deps.storage,
        U64Key::new(pool_id),
        |pool_opt| -> StdResult<_> {
            let mut pool_meta = pool_opt.unwrap();
            pool_meta.rewards_pointer = decimal_summation_in_256(
                pool_meta.rewards_pointer,
                Decimal::from_ratio(rewards_transfer_amount, pool_meta.staked),
            );
            Ok(pool_meta)
        },
    )?;
    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: pool_meta.reward_contract.to_string(),
        msg: to_binary(&RewardExecuteMsg::Transfer {
            reward_amount: rewards_transfer_amount,
            reward_withdraw_contract: config.scc_contract,
            protocol_fee_amount,
            protocol_fee_contract: pool_meta.protocol_fee_contract,
        })?,
        funds: vec![],
    }))
}

// Any address can call this fn.
pub fn queue_user_undelegation(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let user_addr = info.sender;
    let pool_meta = get_verified_pool(deps.storage, pool_id, false)?;

    let current_batch_id = pool_meta.current_undelegation_batch_id;
    let batch_undelegation_registry_id = (U64Key::new(pool_id), U64Key::new(current_batch_id));
    BATCH_UNDELEGATION_REGISTRY.update(
        deps.storage,
        batch_undelegation_registry_id,
        |x| -> StdResult<_> {
            let mut batch_undel = x.unwrap();

            batch_undel.prorated_amount = decimal_summation_in_256(
                batch_undel.prorated_amount,
                Decimal::from_ratio(amount, 1_u128),
            );
            Ok(batch_undel)
        },
    )?;

    // We can subtract pool staked amount here so users won't get rewards for this epoch. But
    // we choose not to. Essentially every epoch users will get slightly less rewards because deposits
    // happening in that epoch will be treated as if they were deposited from the beginning of epoch.

    // Fire and forget will work here because if user transaction will fail then tx will fail this state change too.
    let message = WasmMsg::Execute {
        contract_addr: config.delegator_contract.to_string(),
        msg: to_binary(&DelegatorExecuteMsg::Undelegate {
            user_addr,
            batch_id: current_batch_id,
            from_pool: pool_id,
            amount,
            pool_rewards_pointer: pool_meta.rewards_pointer,
            pool_airdrops_pointer: pool_meta.airdrops_pointer,
            pool_slashing_pointer: pool_meta.slashing_pointer,
        })
        .unwrap(),
        funds: vec![],
    };

    // Check slashing after creating user message because up until this message, slashing pointer
    // could be x and we could be detecting slashing here thereby changing the pointer to y.
    // User when submitting the request would have undelegated from his balance based on slashing pointer x.

    // If slashing were to be detected here, user undelegation amount immediately computed is slashed as well
    // because pointer difference will be there.
    // This real time value of user undelegation amount is computed using pool_batch_slashing pointers
    // and delegator contract undelegation entry pointer.
    check_slashing(&mut deps, env, pool_id)?;

    Ok(Response::new().add_message(message))
}

pub fn undelegate_from_pool(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    check_slashing(&mut deps, env.clone(), pool_id)?;
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

            if batch_undel.prorated_amount.is_zero() {
                return Err(ContractError::NoOp {});
            }

            batch_undel.est_release_time =
                Some(env.block.time.plus_seconds(config.unbonding_period));

            // TODO - Do we need to undelegate 1 uluna extra for any potential round off errors.
            // If yes, this extra uluna money will be ready to be withdrawn, so this action does not misdirect users funds.
            // However it is true that this 1 uluna will not be earning rewards for the user.
            batch_undel.undelegated_amount = uint128_from_decimal(batch_undel.prorated_amount);
            undel_amount = batch_undel.undelegated_amount;
            Ok(batch_undel)
        },
    )?;

    let mut messages = vec![];
    let validators = pool_meta.validators.clone();
    let mut to_undelegate = undel_amount;
    let stake_tuples = get_active_validators_sorted_by_stake(
        deps.querier,
        pool_meta.validator_contract.clone(),
        validators,
    )?;

    for index in (0..stake_tuples.len()).rev() {
        let tuple_val = stake_tuples.get(index).unwrap();
        if to_undelegate.is_zero() {
            break;
        }
        let val_addr = Addr::unchecked(tuple_val.clone().1);
        let amount = std::cmp::min(to_undelegate, tuple_val.clone().0);
        messages.push(WasmMsg::Execute {
            contract_addr: pool_meta.validator_contract.to_string(),
            msg: to_binary(&ValidatorExecuteMsg::Undelegate {
                val_addr: val_addr.clone(),
                amount,
            })
            .unwrap(),
            funds: vec![],
        });

        VALIDATOR_META.update(
            deps.storage,
            (val_addr.clone(), U64Key::new(pool_id)),
            |x| -> StdResult<_> {
                let mut meta = x.unwrap();
                meta.staked = meta.staked.checked_sub(amount)?;
                Ok(meta)
            },
        )?;

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

// No need for regular slashing check here because these funds have been undelegated 21 days ago and
// we are now checking if there was slashing in these 21 days for these funds.
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
    let upper_bound = std::cmp::min(
        pool_meta.current_undelegation_batch_id + 1,
        pool_meta.last_reconciled_batch_id + 1 + 10,
    );
    for batch_id in pool_meta.last_reconciled_batch_id + 1..upper_bound {
        let key = (U64Key::new(pool_id), U64Key::new(batch_id));
        let batch_meta = BATCH_UNDELEGATION_REGISTRY.load(deps.storage, key.clone())?;
        if batch_meta.est_release_time.is_none()
            || batch_meta.est_release_time.unwrap().gt(&env.block.time)
        {
            break;
        }
        total_amount = total_amount
            .checked_add(batch_meta.undelegated_amount)
            .unwrap();
        last_reconciled_id = batch_id;
    }

    // TODO - GM. This can be just a bank balance query.
    // QUERY the base funds in validator contract and check how much can be reconciled
    let unaccounted_base_funds: Coin =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: pool_meta.validator_contract.to_string(),
            msg: to_binary(&ValidatorQueryMsg::GetUnaccountedBaseFunds {})?,
        }))?;

    // Slashing has occured in the 21 day unbonding period. Capture that.
    let unbonding_slashing_ratio = if unaccounted_base_funds.amount.lt(&total_amount) {
        decimal_division_in_256(
            get_decimal_from_uint128(unaccounted_base_funds.amount),
            get_decimal_from_uint128(total_amount),
        )
    } else {
        Decimal::one()
    };

    for batch_id in pool_meta.last_reconciled_batch_id + 1..upper_bound {
        let key = (U64Key::new(pool_id), U64Key::new(batch_id));
        let mut batch_meta = BATCH_UNDELEGATION_REGISTRY.load(deps.storage, key.clone())?;
        if batch_meta.est_release_time.is_none()
            || batch_meta.est_release_time.unwrap().gt(&env.block.time)
        {
            break;
        }
        batch_meta.unbonding_slashing_ratio = unbonding_slashing_ratio;
        batch_meta.reconciled = true;
        BATCH_UNDELEGATION_REGISTRY.save(deps.storage, key.clone(), &batch_meta)?;
    }

    pool_meta.last_reconciled_batch_id = last_reconciled_id;
    POOL_REGISTRY.save(deps.storage, U64Key::new(pool_id), &pool_meta)?;

    let amount_to_transfer = std::cmp::min(unaccounted_base_funds.amount, total_amount);
    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: pool_meta.validator_contract.to_string(),
        msg: to_binary(&ValidatorExecuteMsg::TransferReconciledFunds {
            amount: amount_to_transfer,
        })
        .unwrap(),
        funds: vec![],
    }))
}

// Slashing check not required
pub fn withdraw_funds_to_wallet(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
    batch_id: u64,
    undelegate_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::NoFunds])?;

    let user_addr = info.sender;
    let key = (U64Key::from(pool_id), U64Key::from(batch_id));
    let und_opt = BATCH_UNDELEGATION_REGISTRY.may_load(deps.storage, key)?;
    if und_opt.is_none() {
        return Err(ContractError::UndelegationBatchNotFound {});
    }

    let und_batch = und_opt.unwrap();
    if !und_batch.reconciled {
        return Err(ContractError::UndelegationBatchNotReconciled {});
    }

    let msg = WasmMsg::Execute {
        contract_addr: config.delegator_contract.to_string(),
        msg: to_binary(&DelegatorExecuteMsg::WithdrawFunds {
            user_addr,
            pool_id,
            undelegate_id,
            undelegation_batch_slashing_pointer: und_batch.last_updated_slashing_pointer,
            undelegation_batch_unbonding_slashing_ratio: und_batch.unbonding_slashing_ratio,
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
    airdrop_token_str: String,
    airdrop_contract_str: String,
    cw20_contract_str: String,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    if airdrop_token_str.is_empty() {
        return Err(ContractError::TokenEmpty {});
    }

    let airdrop_token = airdrop_token_str.to_lowercase();
    let airdrop_contract = deps.api.addr_validate(airdrop_contract_str.as_str())?;
    let cw20_contract = deps.api.addr_validate(cw20_contract_str.as_str())?;
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

        let AirdropRegistryInfo {
            airdrop_contract,
            cw20_contract: _,
        } = airdrop_info_opt.unwrap();
        let pool_meta_opt = POOL_REGISTRY.may_load(deps.storage, U64Key::new(rate.pool_id))?;
        if pool_meta_opt.is_none() {
            failed_pools.push(rate.pool_id.to_string());
            continue;
        }
        let pool_meta = pool_meta_opt.unwrap();
        msgs.push(WasmMsg::Execute {
            contract_addr: pool_meta.validator_contract.to_string(),
            msg: to_binary(&ValidatorExecuteMsg::ClaimAirdrop {
                amount: rate.amount,
                claim_msg: to_binary(&MerkleAirdropMsg::Claim {
                    stage: rate.stage,
                    amount: rate.amount,
                    proof: rate.proof,
                })?,
                airdrop_contract,
            })
            .unwrap(),
            funds: vec![],
        });
    }

    Ok(Response::new().add_messages(msgs))
}

// Don't need authentication.
pub fn update_airdrop_pointers(
    deps: DepsMut,
    _info: MessageInfo,
    _env: Env,
    transfers: Vec<AirdropTransferRequest>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let mut msgs = vec![];
    for transfer in transfers {
        let airdrop_info_opt = AIRDROP_REGISTRY.may_load(deps.storage, transfer.denom.clone())?;
        if airdrop_info_opt.is_none() {
            return Err(ContractError::AirdropNotRegistered {});
        }

        let airdrop_meta = airdrop_info_opt.unwrap();
        let mut pool_meta = get_verified_pool(deps.storage, transfer.pool_id, false)?;

        let res: Cw20BalanceResponse =
            deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: airdrop_meta.cw20_contract.to_string(),
                msg: to_binary(&Cw20QueryMsg::Balance {
                    address: pool_meta.validator_contract.to_string(),
                })?,
            }))?;
        let balance = res.balance;

        pool_meta.airdrops_pointer = merge_dec_coin_vector(
            &pool_meta.airdrops_pointer,
            DecCoinVecOp {
                fund: vec![DecCoin::new(
                    Decimal::from_ratio(balance, pool_meta.staked),
                    transfer.denom,
                )],
                operation: Operation::Add,
            },
        );

        msgs.push(WasmMsg::Execute {
            contract_addr: pool_meta.validator_contract.to_string(),
            msg: to_binary(&ValidatorExecuteMsg::TransferAirdrop {
                amount: balance,
                cw20_contract: airdrop_meta.cw20_contract,
            })
            .unwrap(),
            funds: vec![],
        });
        POOL_REGISTRY.save(deps.storage, U64Key::new(transfer.pool_id), &pool_meta)?;
    }

    Ok(Response::new().add_messages(msgs))
}

pub fn update_pool_metadata(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pool_id: u64,
    update_pool_config: PoolConfigUpdateRequest,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManager, Verify::NoFunds],
    )?;

    let mut pool_meta = get_verified_pool(deps.storage, pool_id, false)?;
    pool_meta.active = update_pool_config.active.unwrap_or(pool_meta.active);
    if update_pool_config.reward_contract.is_some() {
        pool_meta.reward_contract = deps
            .api
            .addr_validate(update_pool_config.reward_contract.unwrap().as_str())?;
    }
    if update_pool_config.protocol_fee_contract.is_some() {
        pool_meta.protocol_fee_contract = deps
            .api
            .addr_validate(update_pool_config.protocol_fee_contract.unwrap().as_str())?;
    }
    pool_meta.protocol_fee_percent = update_pool_config
        .protocol_fee_percent
        .unwrap_or(pool_meta.protocol_fee_percent);

    POOL_REGISTRY.save(deps.storage, U64Key::new(pool_id), &pool_meta)?;
    Ok(Response::default())
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
        config.scc_contract = update_config.scc_contract.unwrap_or(config.scc_contract);
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
        QueryMsg::BatchUndelegation { pool_id, batch_id } => {
            to_binary(&query_batch_undelegate(deps, pool_id, batch_id)?)
        }
        QueryMsg::GetUserComputedInfo { pool_id, user_addr } => {
            to_binary(&query_user_computed_info(deps, user_addr, pool_id)?)
        }
        QueryMsg::GetValMeta { pool_id, val_addr } => {
            to_binary(&query_val_meta(deps, pool_id, val_addr)?)
        }
        QueryMsg::GetAirdropRegistry { denom } => {
            to_binary(&query_airdrop_registry(deps, denom)?)
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

pub fn query_batch_undelegate(
    deps: Deps,
    pool_id: u64,
    batch_id: u64,
) -> StdResult<QueryBatchUndelegationResponse> {
    let batch_meta = BATCH_UNDELEGATION_REGISTRY
        .may_load(deps.storage, (U64Key::new(pool_id), U64Key::new(batch_id)))?;
    Ok(QueryBatchUndelegationResponse { batch: batch_meta })
}

pub fn query_airdrop_registry(deps: Deps, token: String) -> StdResult<GetAirdropMetaResponse> {
    let airdrop_meta_opt = AIRDROP_REGISTRY.may_load(deps.storage, token.to_lowercase())?;
    Ok(GetAirdropMetaResponse {
        airdrop_meta: airdrop_meta_opt,
    })
}

pub fn query_user_computed_info(
    deps: Deps,
    user_addr: Addr,
    pool_id: u64,
) -> StdResult<UserPoolResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let pool_meta_opt = POOL_REGISTRY.may_load(deps.storage, U64Key::new(pool_id))?;
    if pool_meta_opt.is_none() {
        return Ok(UserPoolResponse { info: None });
    }
    let pool_meta = pool_meta_opt.unwrap();
    let user_pool_response: UserPoolResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.delegator_contract.to_string(),
            msg: to_binary(&DelegatorQueryMsg::ComputeUserInfo {
                user_addr,
                pool_pointer_info: PoolPointerInfo {
                    pool_id,
                    airdrops_pointer: pool_meta.airdrops_pointer,
                    rewards_pointer: pool_meta.rewards_pointer,
                    slashing_pointer: pool_meta.slashing_pointer,
                },
            })?,
        }))?;

    return Ok(user_pool_response);
}

pub fn query_val_meta(deps: Deps, pool_id: u64, val_addr: Addr) -> StdResult<GetValMetaResponse> {
    let val_meta_opt = VALIDATOR_META.may_load(deps.storage, (val_addr, U64Key::new(pool_id)))?;
    Ok(GetValMetaResponse {
        val_meta: val_meta_opt,
    })
}
