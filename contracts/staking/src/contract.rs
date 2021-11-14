use crate::msg::{ExecuteMsg, GetValMetaResponse, InstantiateMsg, MerkleAirdropMsg, QueryBatchUndelegationResponse, QueryConfigResponse, QueryMsg, QueryStateResponse, Cw20HookMsg, GetFundsClaimRecord};
use crate::request_validation::{create_new_undelegation_batch, get_active_validators_sorted_by_stake, get_validator_for_deposit, validate, Verify, increase_tracked_stake, decrease_tracked_stake};
use crate::state::{AirdropRate, AirdropTransferRequest, Config, ConfigUpdateRequest, State, VMeta, BATCH_UNDELEGATION_REGISTRY, CONFIG, STATE, VALIDATOR_META, BatchUndelegationRecord, USERS, UndelegationInfo};
use crate::ContractError;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Addr, Binary, Coin, Decimal, Deps, DepsMut, Env, MessageInfo, QueryRequest, Response, StdResult, Uint128, WasmMsg, WasmQuery, Timestamp, StakingMsg, DistributionMsg, from_binary, SubMsg, BankMsg, Order, Storage};
// use cw20::{BalanceResponse as Cw20BalanceResponse, Cw20QueryMsg, Cw20ReceiveMsg, Cw20ExecuteMsg};
use cw_storage_plus::{U64Key, Bound};
use reward::msg::ExecuteMsg as RewardExecuteMsg;
use airdrops_registry::msg::{QueryMsg as AirdropsQueryMsg, GetAirdropContractsResponse};
use stader_utils::coin_utils::{decimal_division_in_256, decimal_multiplication_in_256, decimal_summation_in_256, get_decimal_from_uint128, merge_dec_coin_vector, multiply_u128_with_decimal, uint128_from_decimal, DecCoin, DecCoinVecOp, Operation, u128_from_decimal};
use std::borrow::{BorrowMut, Borrow};
use terra_cosmwasm::TerraMsgWrapper;
use std::ops::Deref;
use airdrops_registry::state::AirdropRegistryInfo;
use cw20_base::contract::{instantiate as cw20Instantiate};
use cw20_base::msg::{InstantiateMsg as Cw20InstantiateMsg, ExecuteMsg as Cw20ExecuteMsg, QueryMsg as Cw20QueryMsg};
use cw20_base::{ContractError as Cw20ContractError};
use cw20::{Cw20ReceiveMsg, MinterResponse};

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
        min_deposit: msg.min_deposit,
        max_deposit: msg.max_deposit,
        active: true,

        airdrop_registry_contract: deps.api.addr_validate(msg.airdrops_registry_contract.as_str())?,
        airdrop_withdrawal_contract: deps.api.addr_validate(msg.airdrop_withdrawal_contract.as_str())?,
        reward_contract: deps.api.addr_validate(msg.reward_contract.as_str())?,
        cw20_token_contract: Addr::unchecked("0"),

        protocol_fee_contract: deps.api.addr_validate(msg.protocol_fee_contract.as_str())?,
        protocol_reward_fee: msg.protocol_reward_fee,
        protocol_deposit_fee: msg.protocol_deposit_fee,
        protocol_withdraw_fee: msg.protocol_withdraw_fee,

        undelegation_cooldown: msg.undelegation_cooldown,
        unbonding_period: msg.unbonding_period,
    };

    CONFIG.save(deps.storage, &config)?;

    let initial_er = Decimal::one();
    let state = State {
        total_staked: Uint128::zero(),
        exchange_rate: initial_er,
        last_reconciled_batch_id: 0,
        current_undelegation_batch_id: 1,
        last_undelegation_time: env.block.time.minus_seconds(msg.undelegation_cooldown), // Gives flexibility for first undelegaion run.
        validators: vec![]
    };
    STATE.save(deps.storage, &state)?;

    // loads the saved state
    create_new_undelegation_batch(deps.storage, env.clone())?;

    // TODO - GM. Initialize a mint contract
    let msgs = vec![
        DistributionMsg::SetWithdrawAddress {
            address: config.reward_contract.to_string()
        }
    ];

    let contract_addr = env.contract.address.clone();
    let mint_response = cw20Instantiate(deps, env, info, Cw20InstantiateMsg {
        name: msg.name,
        symbol: msg.symbol,
        decimals: 6,
        initial_balances: vec![],
        mint: Some(cw20::MinterResponse {
            minter: contract_addr.to_string(),
            cap: None
        }),
        marketing: None
    });

    if mint_response.is_err() {
        return Err(ContractError::MintDeployFailed {});
    }

    // TODO - GM. Do I need to store the token contract
    Ok(Response::new().add_messages(msgs))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    match msg {
        ExecuteMsg::AddValidator { val_addr } => {
            add_validator(deps, info, env, val_addr)
        }
        ExecuteMsg::RemoveValidator {
            val_addr,
            redel_addr,
        } => remove_validator_from_pool(deps, info, env, val_addr, redel_addr),
        ExecuteMsg::RebalancePool {
            amount,
            val_addr,
            redel_addr,
        } => rebalance_pool(deps, info, env, amount, val_addr, redel_addr),
        ExecuteMsg::Deposit { } => deposit(deps, info, env),
        ExecuteMsg::RedeemRewards { } => redeem_rewards(deps, info, env),
        ExecuteMsg::Swap { } => swap_rewards(deps, info, env),
        ExecuteMsg::QueueUndelegate { cw20_msg } => receive_cw20(deps, env, info, cw20_msg),
        ExecuteMsg::Undelegate {} => undelegate_stake(deps, info, env),
        ExecuteMsg::ReconcileFunds {} => reconcile_funds(deps, info, env),
        ExecuteMsg::WithdrawFundsToWallet {
            batch_id,
        } => withdraw_funds_to_wallet(deps, info, env, batch_id),
        ExecuteMsg::ClaimAirdrops { rates } => claim_airdrops(deps, info, env, rates),
        ExecuteMsg::UpdateConfig { config_request } => {
            update_config(deps, info, env, config_request)
        }
    }
}

pub fn add_validator(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    if VALIDATOR_META.has(deps.storage, &val_addr) {
        return Err(ContractError::ValidatorAlreadyAdded {});
    }

    // check if the validator exists in the blockchain
    if deps.querier.query_validator(&val_addr).unwrap().is_none() {
        return Err(ContractError::ValidatorNotDiscoverable {});
    }

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.validators.push(val_addr.clone());
        Ok(state)
    })?;

    VALIDATOR_META.save(deps.storage, &val_addr, &VMeta::new())?;

    Ok(Response::new().add_attribute("new_validator", val_addr.to_string()))
}

pub fn remove_validator_from_pool(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
    redel_addr: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    check_slashing(&mut deps, &env)?;

    let mut state = STATE.load(deps.storage)?;

    // TODO - GM. Should we instead check state.validators and make it source of truth
    // as Validator_meta is intended just tobe trakcing data.
    let src_val = VALIDATOR_META.may_load(deps.storage, &val_addr)?;
    let redel_val = VALIDATOR_META.may_load(deps.storage, &redel_addr)?;
    if src_val.is_none() || redel_val.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }
    if val_addr.eq(&redel_addr) {
        return Err(ContractError::ValidatorsCannotBeSame {});
    }

    let new_validator_pool = state.validators
        .into_iter()
        .filter(|x| x.ne(&val_addr))
        .collect::<Vec<Addr>>();

    state.validators = new_validator_pool;

    // Update validator tracking amounts
    let val_delegation =
        deps.querier.query_delegation(env.contract.address, val_addr.clone())?;
    let mut msgs = vec![];
    if val_delegation.is_some() {
        let full_delegation = val_delegation.unwrap();

        if full_delegation.can_redelegate.ne(&full_delegation.amount) {
            return Err(ContractError::RedelegationInProgress {});
        }
        let mut redel_vmeta = redel_val.unwrap();
        redel_vmeta.staked = redel_vmeta.staked.checked_add(full_delegation.amount.amount).unwrap();
        VALIDATOR_META.save(deps.storage, &redel_addr, &redel_vmeta)?;

        if !full_delegation.amount.amount.is_zero() {
            msgs.push(StakingMsg::Redelegate {
                src_validator: val_addr.to_string(),
                dst_validator: redel_addr.to_string(),
                amount: full_delegation.amount,
            });
        }
    }
    STATE.save(deps.storage, &state)?;
    VALIDATOR_META.remove(deps.storage, &val_addr);

    Ok(Response::new().add_messages(msgs))
}

pub fn rebalance_pool(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    amount: Uint128,
    val_addr: Addr,
    redel_addr: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    let state = STATE.load(deps.storage)?;
    check_slashing(&mut deps, &env)?;
    if val_addr.eq(&redel_addr) {
        return Err(ContractError::ValidatorsCannotBeSame {});
    }

    if !state.validators.contains(&val_addr) || !state.validators.contains(&redel_addr) {
        return Err(ContractError::ValidatorNotAdded {});
    }

    let src_val_delegation = deps
        .querier
        .query_delegation(env.contract.address, val_addr.clone())?;
    if src_val_delegation.is_none() || src_val_delegation.unwrap().amount.amount.lt(&amount) {
        return Err(ContractError::InSufficientFunds {});
    }

    // Update validator tracking amounts
    decrease_tracked_stake(&mut deps, &val_addr, amount)?;
    increase_tracked_stake(&mut deps, &redel_addr, amount)?;

    Ok(Response::new().add_message(StakingMsg::Redelegate {
        src_validator: "val_addr".to_string(),
        dst_validator: "redel_addr".to_string(),
        amount: Coin::new(amount.u128(), config.vault_denom),
    }))
}

pub fn get_total_token_supply() -> Uint128 {
    // TODO - GM. Default to a zero here.
    return Uint128::new(12345);
}

// Modifies pool object. So re-fetch after this call is done.
pub fn check_slashing(
    deps: &mut DepsMut,
    env: &Env,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let mut total_staked_on_chain = Uint128::zero();
    let delegations = deps.querier.query_all_delegations(env.contract.address.clone())?;
    for delegation in delegations {
        total_staked_on_chain = total_staked_on_chain.checked_add(delegation.amount.amount).unwrap();
        VALIDATOR_META.update(
            deps.storage,
            &deps.api.addr_validate(delegation.validator.as_str())?,
            |x| -> StdResult<_> {
                let mut val_meta = x.unwrap();

                if val_meta.staked.gt(&delegation.amount.amount) {
                    val_meta.slashed = val_meta.slashed.checked_add(
                        val_meta
                            .staked
                            .checked_sub(delegation.amount.amount)
                            .unwrap_or(Uint128::zero()),
                    )?;
                    val_meta.staked = delegation.amount.amount;
                }

                Ok(val_meta)
            },
        )?;
    }

    let total_tokens = get_total_token_supply();

    // Slashing has occured. Update pointers.
    state.total_staked = total_staked_on_chain;
    state.exchange_rate = calculate_exchange_rate(state.total_staked, total_tokens);
    STATE.save(deps.storage, &state)?;

    Ok(Response::default())
}

pub fn calculate_exchange_rate(total_staked: Uint128, total_token_supply: Uint128) -> Decimal {
    if total_staked.is_zero() || total_token_supply.is_zero() {
        return Decimal::one();
    }
    Decimal::from_ratio(total_staked, total_token_supply)
}

pub fn create_mint_message(amount_to_mint: Uint128, recipient: Addr) -> SubMsg<TerraMsgWrapper> {
    return SubMsg::new(StakingMsg::Delegate {
        validator: "asdf".to_string(), amount: Default::default() }
    )
}

pub fn burn_minted_tokens(amount_to_burn: Uint128) -> SubMsg<TerraMsgWrapper> {
    return SubMsg::new(StakingMsg::Delegate {
        validator: "asdf".to_string(), amount: Default::default() }
    )
}

// Any address can call this.
pub fn deposit(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
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
    let sender = info.sender;
    let mut state = STATE.load(deps.storage)?;

    // Formula wise - we want to recompute user balance because slashing pointer has changed and then
    // add the money user wants to delegate. Money being added in this message should be considered post slashing.
    check_slashing(&mut deps, &env)?;

    // TODO - GM. Math.decimal_division
    let tokens_to_mint = uint128_from_decimal(
        decimal_division_in_256(get_decimal_from_uint128(amount), state.exchange_rate));

    let val_addr = get_validator_for_deposit(
        deps.querier, env.contract.address, state.validators.clone())?;

    state.total_staked = state.total_staked.checked_add(amount).unwrap();
    increase_tracked_stake(&mut deps, &val_addr, amount)?;

    // let msgs: Vec<SubMsg> = vec![
    //   create_mint_message(tokens_to_mint, sender),
    // ];

    Ok(Response::new().add_message(StakingMsg::Delegate {
        validator: val_addr.to_string(),
        amount: Coin::new(amount.u128(), config.vault_denom),
    }).add_submessage(create_mint_message(tokens_to_mint, sender)))
}

pub fn redeem_rewards(
    mut deps: DepsMut,
    _info: MessageInfo,
    env: Env,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    check_slashing(&mut deps, &env)?;
    let state = STATE.load(deps.storage)?;

    let mut messages = vec![];
    let mut failed_vals: Vec<String> = vec![];
    for val_addr in state.validators {
        // Skip validators that are currently jailed.
        if deps.querier.query_validator(val_addr.to_string())?.is_none()
            || deps.querier.query_delegation(env.contract.address.clone(), val_addr.to_string()).unwrap().is_none() {
            failed_vals.push(val_addr.to_string());
            continue;
        }

        messages.push(DistributionMsg::WithdrawDelegatorReward {
            validator: val_addr.to_string(),
        });
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("failed_validators", failed_vals.join(",")))
}

// TODO - GM. Does swap have a fixed cost or a linear cost?
// Useful to make this permissionless.
pub fn swap_rewards(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: config.reward_contract.to_string(),
        msg: to_binary(&RewardExecuteMsg::Swap {})?,
        funds: vec![],
    }))
}

// Don't need it to be permissioned. 0 transfers are treated as a NO-OP in rewards contract.
pub fn reinvest(
    mut deps: DepsMut,
    _info: MessageInfo,
    env: Env,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let mut state = STATE.load(deps.storage)?;
    let balance = deps
        .querier
        .query_balance(config.reward_contract.to_string(), config.vault_denom.clone())?;

    let protocol_fee_amount = Uint128::new(u128_from_decimal(
        decimal_multiplication_in_256(
            get_decimal_from_uint128(balance.amount),config.protocol_reward_fee)));
    let transfer_amount =
        balance.amount.checked_sub(protocol_fee_amount).unwrap_or(Uint128::zero());

    let val_addr = get_validator_for_deposit(
        deps.querier, env.contract.address.clone(), state.validators.clone())?;
    state.total_staked = state.total_staked.checked_add(transfer_amount).unwrap();
    increase_tracked_stake(&mut deps, &val_addr, transfer_amount)?;
    state.exchange_rate = Decimal::from_ratio(state.total_staked, get_total_token_supply());

    STATE.save(deps.storage, &state)?;

    // Reward contract throws an error if transfer_amount is not available to be sent over.
    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: config.reward_contract.to_string(),
        msg: to_binary(&RewardExecuteMsg::Transfer {
            reward_amount: transfer_amount,
            reward_withdraw_contract: env.contract.address,
            protocol_fee_amount,
            protocol_fee_contract: config.protocol_fee_contract,
        })?,
        funds: vec![],
    })
        .add_message(StakingMsg::Delegate {
            validator: val_addr.to_string(),
            amount: Coin::new(transfer_amount.u128(), config.vault_denom)
        })
    )
}

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let contract_addr = info.sender.clone();
    let config = CONFIG.load(deps.storage)?;

    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::QueueUndelegate {}) => {
            // only token contract can execute this message
            if contract_addr != config.cw20_token_contract {
                return Err(ContractError::Unauthorized {});
            }
            Ok(queue_undelegation(deps, env, info, cw20_msg.amount, cw20_msg.sender)?)
        }
        Err(err) => Err(ContractError::NoOp {}),
    }
}

// We don't actually burn tokens here. Burning happens only during undelegation
pub fn queue_undelegation(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    amount_to_burn: Uint128,
    user_addr_str: String,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    check_slashing(&mut deps, &env)?;

    let mut state = STATE.load(deps.storage)?;

    let total_tokens = get_total_token_supply();
    let new_token_supply = total_tokens.checked_sub(amount_to_burn).unwrap_or(Uint128::zero());
    state.exchange_rate = calculate_exchange_rate(state.total_staked, new_token_supply);

    let batch_key = U64Key::new(state.current_undelegation_batch_id);
    let user_addr = deps.api.addr_validate(user_addr_str.as_str())?;

    USERS.update(deps.storage, (&user_addr, batch_key.clone()), |x| -> StdResult<_> {
        let mut user_current_batch_undelegations = x.unwrap_or(UndelegationInfo {
            batch_id: state.current_undelegation_batch_id,
            token_amount: Uint128::zero(),
        });
        user_current_batch_undelegations.token_amount =
            user_current_batch_undelegations.token_amount.checked_add(amount_to_burn).unwrap();
        Ok(user_current_batch_undelegations)
    })?;
    BATCH_UNDELEGATION_REGISTRY.update(deps.storage, batch_key, |x| -> StdResult<_> {
        let mut batch_undelegation = x.unwrap();
        batch_undelegation.undelegated_tokens = batch_undelegation.undelegated_tokens.checked_add(amount_to_burn)?;
        Ok(batch_undelegation)
    })?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::default())
}

pub fn undelegate_stake(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    check_slashing(&mut deps, &env)?;

    let mut state = STATE.load(deps.storage)?;

    if info.sender.ne(&config.manager)
        && env.block.time.lt(&state.last_undelegation_time.plus_seconds(config.undelegation_cooldown)) {
        return Err(ContractError::UndelegationInCooldown {});
    }

    let mut messages = vec![];
    // This is because a new batch wuold be created before this message is called, so -1.
    let undelegate_batch_id = state.current_undelegation_batch_id;
    let batch_key = U64Key::new(undelegate_batch_id);
    let mut undel_amount = Uint128::zero(); // Amount to actually undelegate from blockchain
    BATCH_UNDELEGATION_REGISTRY.update(
        deps.storage,
        batch_key,
        |x| -> Result<_, ContractError> {
            let mut batch_undel = x.unwrap();

            if batch_undel.undelegated_tokens.is_zero() {
                return Err(ContractError::NoOp {});
            }
            messages.push(burn_minted_tokens(batch_undel.undelegated_tokens));

            batch_undel.est_release_time =
                Some(env.block.time.plus_seconds(config.unbonding_period));
            batch_undel.undelegated_stake = Uint128::new(
                multiply_u128_with_decimal(batch_undel.undelegated_tokens.u128(), state.exchange_rate));
            undel_amount = batch_undel.undelegated_stake;
            Ok(batch_undel)
        },
    )?;
    let validators = state.validators.clone();
    let mut to_undelegate = undel_amount;
    let stake_tuples =
        get_active_validators_sorted_by_stake(deps.querier, env.contract.address.clone(), validators)?;

    for index in (0..stake_tuples.len()).rev() {
        let tuple_val = stake_tuples.get(index).unwrap();
        if to_undelegate.is_zero() {
            break;
        }
        let val_addr = Addr::unchecked(tuple_val.clone().1);
        let amount = std::cmp::min(to_undelegate, tuple_val.clone().0);
        messages.push(SubMsg::new(StakingMsg::Undelegate {
            validator: val_addr.to_string(),
            amount: Coin::new(amount.u128(), config.vault_denom.clone()),
        }));

        decrease_tracked_stake(&mut deps, &val_addr, amount)?;
        to_undelegate = to_undelegate.checked_sub(amount).unwrap();
    }

    if !to_undelegate.is_zero() {
        return Err(ContractError::InSufficientFunds {});
    }

    state.last_undelegation_time = env.block.time;
    state.total_staked = state.total_staked.checked_sub(undel_amount).unwrap_or(Uint128::zero());
    STATE.save(deps.storage, &state)?;

    // Loads the saved state.
    create_new_undelegation_batch(deps.storage, env)?;

    Ok(Response::new()
        .add_submessages(messages)
        .add_attribute("Undelegation_amount", undel_amount.to_string()))
}

// No need for regular slashing check here because these funds have been undelegated 21 days ago and
// we are now checking if there was slashing in these 21 days for these funds.
pub fn reconcile_funds(
    deps: DepsMut,
    _info: MessageInfo,
    env: Env,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config= CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    let mut total_stake_expected = Uint128::zero();
    let mut last_reconciled_id = state.last_reconciled_batch_id;

    let upper_bound_exclusive = std::cmp::min(
        state.current_undelegation_batch_id + 1,
        state.last_reconciled_batch_id + 1 + 10, // 10 is default size of pagination
    );
    for batch_id in state.last_reconciled_batch_id + 1..upper_bound_exclusive {
        let key = U64Key::new(batch_id);
        let batch_meta = BATCH_UNDELEGATION_REGISTRY.load(deps.storage, key.clone())?;
        if batch_meta.est_release_time.is_none()
            || batch_meta.est_release_time.unwrap().gt(&env.block.time)
        {
            break;
        }
        total_stake_expected = total_stake_expected
            .checked_add(batch_meta.undelegated_stake)
            .unwrap();
        last_reconciled_id = batch_id;
    }

    if total_stake_expected.is_zero() {
        return Ok(Response::default())
    }

    // QUERY the base funds and check how much can be reconciled
    let balance = deps.querier.query_balance(env.contract.address.to_string(), config.vault_denom)?;
    if balance.amount.is_zero() {
        return Err(ContractError::ZeroAmount {});
    }

    // Slashing may have occured in the 21 day unbonding period. Capture that.
    let unbonding_slashing_ratio = std::cmp::min(
        Decimal::from_ratio(balance.amount, total_stake_expected),
        Decimal::one()
    );

    for batch_id in state.last_reconciled_batch_id + 1..upper_bound_exclusive {
        let key = U64Key::new(batch_id);
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

    state.last_reconciled_batch_id = last_reconciled_id;
    STATE.save(deps.storage, &state)?;

    Ok(Response::default())
}

// Slashing check not required
pub fn withdraw_funds_to_wallet(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    batch_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {

    let config = CONFIG.load(deps.storage)?;
    let user_addr = deps.api.addr_validate(info.sender.as_str())?;
    let funds_record = compute_withdrawable_funds(deps.storage.deref(), batch_id, &user_addr)?;
    let mut msgs = vec![];

    if !funds_record.user_withdrawal_amount.is_zero() {
        msgs.push(BankMsg::Send {
            to_address: user_addr.to_string(), amount: vec![Coin::new(funds_record.user_withdrawal_amount.u128(), config.vault_denom)]
        });
    }

    USERS.remove(deps.storage, (&user_addr, U64Key::new(batch_id)));
    Ok(Response::new().add_messages(msgs))
}

// Does not change any state. Used for both messages & queries
pub fn compute_withdrawable_funds(storage: & dyn Storage, batch_id: u64, user_addr: &Addr) -> Result<GetFundsClaimRecord, ContractError> {
    let config = CONFIG.load(storage)?;

    let und_opt =
        BATCH_UNDELEGATION_REGISTRY.may_load(storage, U64Key::new(batch_id))?;
    if und_opt.is_none() {
        return Err(ContractError::UndelegationBatchNotFound {});
    }

    let und_batch = und_opt.unwrap();
    if !und_batch.reconciled {
        return Err(ContractError::UndelegationBatchNotReconciled {});
    }

    let key = (user_addr, U64Key::from(batch_id));
    let user_undelegated_tokens_opt = USERS.may_load(storage, key)?;
    if user_undelegated_tokens_opt.is_none() {
        return Err(ContractError::UndelegationEntryNotFound {});
    }
    let user_undelegation = user_undelegated_tokens_opt.unwrap();
    if user_undelegation.token_amount.is_zero() {
        return Err(ContractError::ZeroAmount {});
    }

    let user_undelegated_amount = multiply_u128_with_decimal(
        user_undelegation.token_amount.u128(), und_batch.undelegation_er);

    let claimable_amount = multiply_u128_with_decimal(
        user_undelegated_amount, und_batch.unbonding_slashing_ratio);

    let protocol_fee = multiply_u128_with_decimal(
        claimable_amount, config.protocol_withdraw_fee);

    let user_withdrawal_amount = claimable_amount - protocol_fee;
    Ok(GetFundsClaimRecord {
        user_withdrawal_amount: Uint128::new(user_withdrawal_amount),
        protocol_fee: Uint128::new(protocol_fee),
        undelegated_amount: user_undelegation.token_amount,
    })
}

// Can be permissionless
pub fn claim_airdrops(
    deps: DepsMut,
    _info: MessageInfo,
    _env: Env,
    airdrop_rates: Vec<AirdropRate>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let mut msgs = vec![];
    let mut failed_airdrops = vec![];
    let airdrop_withdrawal_contract = config.airdrop_withdrawal_contract;
    let airdrops_registry_contract = config.airdrop_registry_contract;
    for rate in airdrop_rates {
        let contract_response: GetAirdropContractsResponse =
            deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: airdrops_registry_contract.clone().to_string(),
                msg: to_binary(&AirdropsQueryMsg::GetAirdropContracts {
                    token: rate.denom.to_string(),
                })?,
            }))?;

        if contract_response.contracts.is_none() {
            failed_airdrops.push((rate.denom, rate.stage));
            continue;
        }
        let contracts = contract_response.contracts.unwrap();
        let claim_msg = to_binary(&MerkleAirdropMsg::Claim {
            stage: rate.stage,
            amount: rate.amount,
            proof: rate.proof,
        })?;
        msgs.push(WasmMsg::Execute {
            contract_addr: contracts.airdrop_contract.to_string(),
            msg: claim_msg,
            funds: vec![],
        });
        msgs.push(WasmMsg::Execute {
            contract_addr: contracts.cw20_contract.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: airdrop_withdrawal_contract.to_string(),
                amount: rate.amount,
            }).unwrap(),
            funds: vec![],
        })
    }

    Ok(Response::new().add_messages(msgs))
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    update_config: ConfigUpdateRequest,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
//     let mut config = CONFIG.load(deps.storage)?;
//     validate(
//         &config,
//         &info,
//         &env,
//         vec![Verify::SenderManager, Verify::NoFunds],
//     )?;
//
//     if update_config.delegator_contract.is_some() {
//         if config.delegator_contract.eq(&Addr::unchecked("0")) {
//             config.delegator_contract = deps
//                 .api
//                 .addr_validate(update_config.delegator_contract.unwrap().as_str())?;
//         }
//     }
//     if update_config.scc_contract.is_some() {
//         config.scc_contract = deps
//             .api
//             .addr_validate(update_config.scc_contract.unwrap().as_str())?;
//     }
//
//     config.unbonding_period = update_config
//         .unbonding_period
//         .unwrap_or(config.unbonding_period);
//     config.undelegation_cooldown = update_config
//         .undelegation_cooldown
//         .unwrap_or(config.undelegation_cooldown);
//     config.min_deposit = update_config.min_deposit.unwrap_or(config.min_deposit);
//     config.max_deposit = update_config.max_deposit.unwrap_or(config.max_deposit);
//
//     CONFIG.save(deps.storage, &config)?;
//
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::BatchUndelegation { batch_id } => {
            to_binary(&query_batch_undelegate(deps, batch_id)?)
        }
        QueryMsg::GetUserComputedInfo { user_addr, start_after, limit } => {
            to_binary(&query_user_computed_info(deps, user_addr, start_after, limit)?)
        }
        QueryMsg::GetUserUndelegationRecord { user_addr, batch_id } => {
            to_binary(&query_user_undelegation_info(deps, user_addr, batch_id)?)
        }
        QueryMsg::GetValMeta { val_addr } => {
            to_binary(&query_val_meta(deps, val_addr)?)
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

pub fn query_batch_undelegate(
    deps: Deps,
    batch_id: u64,
) -> StdResult<QueryBatchUndelegationResponse> {
    let batch_meta = BATCH_UNDELEGATION_REGISTRY
        .may_load(deps.storage, U64Key::new(batch_id))?;
    Ok(QueryBatchUndelegationResponse { batch: batch_meta })
}

// TODO - GM. Test this
pub fn query_user_computed_info(
    deps: Deps,
    user_addr_str: String,
    start_after: Option<u64>,
    limit: Option<u64>,
) -> StdResult<Vec<UndelegationInfo>> {
    let user_addr = deps.api.addr_validate(user_addr_str.as_str())?;
    let limit = limit.unwrap_or(10).min(20) as usize;
    // TODO - GM. Will converting u64 to string for batch id start work?
    let start = start_after.map(|batch_id| Bound::exclusive(batch_id.to_string()));

    let user_undelegations = USERS.prefix(&user_addr)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| item.unwrap().1).collect::<Vec<UndelegationInfo>>();

    return Ok(user_undelegations);
}

pub fn query_user_undelegation_info(deps: Deps, user_addr: Addr, batch_id: u64) -> StdResult<GetFundsClaimRecord> {
    let funds_record = compute_withdrawable_funds(deps.storage, batch_id, &user_addr).unwrap();
    Ok(funds_record)
}

pub fn query_val_meta(deps: Deps, val_addr: Addr) -> StdResult<GetValMetaResponse> {
    let val_meta_opt = VALIDATOR_META.may_load(deps.storage, &val_addr)?;
    Ok(GetValMetaResponse {
        val_meta: val_meta_opt,
    })
}

