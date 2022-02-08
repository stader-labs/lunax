use crate::helpers::{
    burn_minted_tokens, calculate_exchange_rate, create_mint_message,
    create_new_undelegation_batch, decrease_tracked_stake, get_active_validators_sorted_by_stake,
    get_airdrop_contracts, get_total_token_supply, get_user_balance, get_validator_for_deposit,
    increase_tracked_stake, validate, Verify,
};
use crate::msg::{
    Cw20HookMsg, ExecuteMsg, GetFundsClaimRecord, GetFundsDepositRecord, GetValMetaResponse,
    InstantiateMsg, MerkleAirdropMsg, MigrateMsg, QueryBatchUndelegationResponse,
    QueryConfigResponse, QueryMsg, QueryStateResponse, TmpManagerStoreResponse, UserInfoResponse,
    UserQueryInfo,
};
use crate::state::{
    AirdropRate, Config, ConfigUpdateRequest, OperationControls, OperationControlsUpdateRequest,
    State, TmpManagerStore, UndelegationInfo, VMeta, BATCH_UNDELEGATION_REGISTRY, CONFIG,
    OPERATION_CONTROLS, STATE, TMP_MANAGER_STORE, USERS, VALIDATOR_META,
};
use crate::ContractError;
use airdrops_registry::msg::GetAirdropContractsResponse;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, BankMsg, Binary, Coin, Decimal, Deps, DepsMut, DistributionMsg,
    Env, MessageInfo, Order, Response, StakingMsg, StdError, StdResult, Storage, SubMsg, Uint128,
    WasmMsg,
};
use cw20::Cw20ReceiveMsg;
use cw20_base::msg::ExecuteMsg as Cw20ExecuteMsg;
use cw_storage_plus::{Bound, U64Key};
use reward::msg::ExecuteMsg as RewardExecuteMsg;
use stader_utils::coin_utils::{
    decimal_division_in_256, decimal_multiplication_in_256, get_decimal_from_uint128,
    multiply_u128_with_decimal, uint128_from_decimal,
};
use std::ops::{Deref, Mul};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    if msg.protocol_reward_fee.gt(&Decimal::one())
        || msg.protocol_deposit_fee.gt(&Decimal::one())
        || msg.protocol_withdraw_fee.gt(&Decimal::one())
    {
        return Err(ContractError::ProtocolFeeAboveLimit {});
    }

    let config = Config {
        manager: info.sender.clone(),
        vault_denom: "uluna".to_string(),
        min_deposit: msg.min_deposit,
        max_deposit: msg.max_deposit,
        active: true,

        airdrop_registry_contract: deps
            .api
            .addr_validate(msg.airdrops_registry_contract.as_str())?,
        airdrop_withdrawal_contract: deps
            .api
            .addr_validate(msg.airdrop_withdrawal_contract.as_str())?,
        reward_contract: deps.api.addr_validate(msg.reward_contract.as_str())?,
        cw20_token_contract: Addr::unchecked("0"),

        protocol_fee_contract: deps.api.addr_validate(msg.protocol_fee_contract.as_str())?,
        protocol_reward_fee: msg.protocol_reward_fee,
        protocol_deposit_fee: msg.protocol_deposit_fee,
        protocol_withdraw_fee: msg.protocol_withdraw_fee,

        undelegation_cooldown: msg.undelegation_cooldown,
        swap_cooldown: msg.swap_cooldown,
        unbonding_period: msg.unbonding_period,
        reinvest_cooldown: msg.reinvest_cooldown,
    };

    CONFIG.save(deps.storage, &config)?;

    let initial_er = Decimal::one();

    let state = State {
        total_staked: Uint128::zero(),
        exchange_rate: initial_er,
        last_reconciled_batch_id: 0,
        current_undelegation_batch_id: 0,
        last_undelegation_time: env.block.time.minus_seconds(msg.undelegation_cooldown), // Gives flexibility for first undelegaion run.
        last_swap_time: env.block.time.minus_seconds(msg.swap_cooldown),
        last_reinvest_time: env.block.time.minus_seconds(msg.reinvest_cooldown),
        validators: vec![],
        reconciled_funds_to_withdraw: Uint128::zero(),
    };
    STATE.save(deps.storage, &state)?;

    let operation_controls = OperationControls {
        deposit_paused: false,
        queue_undelegate_paused: false,
        undelegate_paused: false,
        withdraw_paused: false,
        reinvest_paused: false,
        reconcile_paused: false,
        claim_airdrops_paused: false,
        redeem_rewards_paused: false,
        swap_paused: false,
        reimburse_slashing_paused: false,
    };
    OPERATION_CONTROLS.save(deps.storage, &operation_controls)?;

    // loads the saved state
    create_new_undelegation_batch(deps.storage, env.clone())?;

    let msgs = vec![DistributionMsg::SetWithdrawAddress {
        address: config.reward_contract.to_string(),
    }];

    Ok(Response::new().add_messages(msgs))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::AddValidator { val_addr } => add_validator(deps, info, env, val_addr),
        ExecuteMsg::RemoveValidator {
            val_addr,
            redel_addr,
        } => remove_validator_from_pool(deps, info, env, val_addr, redel_addr),
        ExecuteMsg::RebalancePool {
            amount,
            val_addr,
            redel_addr,
        } => rebalance_pool(deps, info, env, amount, val_addr, redel_addr),
        ExecuteMsg::Deposit {} => deposit(deps, info, env),
        ExecuteMsg::RedeemRewards {} => redeem_rewards(deps, info, env),
        ExecuteMsg::Swap {} => swap_rewards(deps, info, env),
        ExecuteMsg::Reinvest {} => reinvest(deps, info, env),
        ExecuteMsg::ReimburseSlashing { val_addr } => reimburse_slashing(deps, info, env, val_addr),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Undelegate {} => undelegate_stake(deps, info, env),
        ExecuteMsg::ReconcileFunds {} => reconcile_funds(deps, info, env),
        ExecuteMsg::WithdrawFundsToWallet { batch_id } => {
            withdraw_funds_to_wallet(deps, info, env, batch_id)
        }
        ExecuteMsg::ClaimAirdrops { rates } => claim_airdrops(deps, info, env, rates),
        ExecuteMsg::UpdateConfig { config_request } => {
            update_config(deps, info, env, config_request)
        }
        ExecuteMsg::UpdateOperationFlags {
            operation_controls_update_request,
        } => update_operation_flags(deps, info, env, operation_controls_update_request),
        ExecuteMsg::SetManager { manager } => set_manager(deps, info, env, manager),
        ExecuteMsg::AcceptManager {} => accept_manager(deps, info, env),
    }
}

pub fn set_manager(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    manager: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    TMP_MANAGER_STORE.save(deps.storage, &TmpManagerStore { manager })?;

    Ok(Response::default())
}

pub fn accept_manager(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    let tmp_manager_store =
        if let Some(tmp_manager_store) = TMP_MANAGER_STORE.may_load(deps.storage)? {
            tmp_manager_store
        } else {
            return Err(ContractError::TmpManagerStoreEmpty {});
        };

    config.manager = deps.api.addr_validate(tmp_manager_store.manager.as_str())?;
    TMP_MANAGER_STORE.remove(deps.storage);

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

pub fn update_operation_flags(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    operation_controls_update_request: OperationControlsUpdateRequest,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;
    let mut operation_controls = OPERATION_CONTROLS.load(deps.storage)?;

    operation_controls.deposit_paused = operation_controls_update_request
        .deposit_paused
        .unwrap_or(operation_controls.deposit_paused);
    operation_controls.withdraw_paused = operation_controls_update_request
        .withdraw_paused
        .unwrap_or(operation_controls.withdraw_paused);
    operation_controls.reconcile_paused = operation_controls_update_request
        .reconcile_paused
        .unwrap_or(operation_controls.reconcile_paused);
    operation_controls.undelegate_paused = operation_controls_update_request
        .undelegate_paused
        .unwrap_or(operation_controls.undelegate_paused);
    operation_controls.queue_undelegate_paused = operation_controls_update_request
        .queue_undelegate_paused
        .unwrap_or(operation_controls.queue_undelegate_paused);
    operation_controls.reinvest_paused = operation_controls_update_request
        .reinvest_paused
        .unwrap_or(operation_controls.reinvest_paused);
    operation_controls.redeem_rewards_paused = operation_controls_update_request
        .redeem_rewards_paused
        .unwrap_or(operation_controls.redeem_rewards_paused);
    operation_controls.swap_paused = operation_controls_update_request
        .swap_paused
        .unwrap_or(operation_controls.swap_paused);
    operation_controls.claim_airdrops_paused = operation_controls_update_request
        .claim_airdrops_paused
        .unwrap_or(operation_controls.claim_airdrops_paused);
    operation_controls.reimburse_slashing_paused = operation_controls_update_request
        .reimburse_slashing_paused
        .unwrap_or(operation_controls.reimburse_slashing_paused);

    OPERATION_CONTROLS.save(deps.storage, &operation_controls)?;

    Ok(Response::default())
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    update_config: ConfigUpdateRequest,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    if let Some(cw20_contract) = update_config.cw20_token_contract {
        if config.cw20_token_contract == Addr::unchecked("0") {
            config.cw20_token_contract = deps.api.addr_validate(cw20_contract.as_str())?;
        }
    }

    if let Some(arc) = update_config.airdrop_registry_contract {
        config.airdrop_registry_contract = deps.api.addr_validate(arc.as_str())?;
    }

    config.min_deposit = update_config.min_deposit.unwrap_or(config.min_deposit);
    config.max_deposit = update_config.max_deposit.unwrap_or(config.max_deposit);
    config.active = update_config.active.unwrap_or(config.active);

    if let Some(pdf) = update_config.protocol_deposit_fee {
        if pdf.gt(&Decimal::one()) {
            return Err(ContractError::ProtocolFeeAboveLimit {});
        }
        config.protocol_deposit_fee = pdf;
    }

    if let Some(pwf) = update_config.protocol_withdraw_fee {
        if pwf.gt(&Decimal::one()) {
            return Err(ContractError::ProtocolFeeAboveLimit {});
        }
        config.protocol_withdraw_fee = pwf;
    }

    if let Some(prf) = update_config.protocol_reward_fee {
        if prf.gt(&Decimal::one()) {
            return Err(ContractError::ProtocolFeeAboveLimit {});
        }
        config.protocol_reward_fee = prf;
    }

    config.undelegation_cooldown = update_config
        .undelegation_cooldown
        .unwrap_or(config.undelegation_cooldown);
    config.unbonding_period = update_config
        .unbonding_period
        .unwrap_or(config.unbonding_period);
    config.swap_cooldown = update_config.swap_cooldown.unwrap_or(config.swap_cooldown);
    config.reinvest_cooldown = update_config
        .reinvest_cooldown
        .unwrap_or(config.reinvest_cooldown);

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

pub fn add_validator(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    if state.validators.contains(&val_addr) {
        return Err(ContractError::ValidatorAlreadyAdded {});
    }

    // check if the validator exists in the blockchain
    if deps.querier.query_validator(&val_addr)?.is_none() {
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
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    check_slashing(&mut deps, &env)?;

    let mut state = STATE.load(deps.storage)?;

    if val_addr.eq(&redel_addr) {
        return Err(ContractError::ValidatorsCannotBeSame {});
    }

    if !state.validators.contains(&val_addr) || !state.validators.contains(&redel_addr) {
        return Err(ContractError::ValidatorNotAdded {});
    }

    state.validators = state
        .validators
        .into_iter()
        .filter(|x| x.ne(&val_addr))
        .collect::<Vec<Addr>>();

    // Update validator tracking amounts
    let val_delegation = deps
        .querier
        .query_delegation(env.contract.address, val_addr.clone())?;
    let mut msgs = vec![];
    if val_delegation.is_some() {
        let full_delegation = val_delegation.unwrap();

        if full_delegation.can_redelegate.ne(&full_delegation.amount) {
            return Err(ContractError::RedelegationInProgress {});
        }

        increase_tracked_stake(&mut deps, &redel_addr, full_delegation.amount.amount)?;

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
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    if amount.is_zero() {
        return Err(ContractError::ZeroAmount {});
    }

    check_slashing(&mut deps, &env)?;

    let state = STATE.load(deps.storage)?;
    if val_addr.eq(&redel_addr) {
        return Err(ContractError::ValidatorsCannotBeSame {});
    }

    if !state.validators.contains(&val_addr) || !state.validators.contains(&redel_addr) {
        return Err(ContractError::ValidatorNotAdded {});
    }

    let src_val_delegation_opt = deps
        .querier
        .query_delegation(env.contract.address, val_addr.clone())?;
    if let Some(src_val_delegation) = src_val_delegation_opt {
        if src_val_delegation.amount.amount.lt(&amount) {
            return Err(ContractError::InSufficientFunds {});
        }

        if src_val_delegation
            .can_redelegate
            .amount
            .ne(&src_val_delegation.amount.amount)
        {
            return Err(ContractError::RedelegationInProgress {});
        }
    } else {
        return Err(ContractError::InSufficientFunds {});
    };

    // Update validator tracking amounts
    decrease_tracked_stake(&mut deps, &val_addr, amount)?;
    increase_tracked_stake(&mut deps, &redel_addr, amount)?;

    Ok(Response::new().add_message(StakingMsg::Redelegate {
        src_validator: val_addr.to_string(),
        dst_validator: redel_addr.to_string(),
        amount: Coin::new(amount.u128(), config.vault_denom),
    }))
}

pub fn check_slashing(deps: &mut DepsMut, env: &Env) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;
    let mut total_staked_on_chain = Uint128::zero();

    for val_addr in state.validators.iter() {
        let delegation_amount = if let Some(delegation) = deps
            .querier
            .query_delegation(env.contract.address.clone(), val_addr)?
        {
            delegation.amount.amount
        } else {
            Uint128::zero()
        };

        total_staked_on_chain = total_staked_on_chain
            .checked_add(delegation_amount)
            .unwrap();

        VALIDATOR_META.update(deps.storage, val_addr, |x| -> Result<_, ContractError> {
            let mut val_meta = x.unwrap_or(VMeta::new());

            if val_meta.staked.gt(&delegation_amount) {
                let slashed_amount = val_meta
                    .staked
                    .checked_sub(delegation_amount)
                    .unwrap_or(Uint128::zero());
                val_meta.slashed = val_meta.slashed.checked_add(slashed_amount).unwrap();
            }
            val_meta.staked = delegation_amount;

            Ok(val_meta)
        })?;
    }

    let total_tokens = get_total_token_supply(deps.querier, config.cw20_token_contract)?;

    state.total_staked = total_staked_on_chain;
    state.exchange_rate = calculate_exchange_rate(state.total_staked, total_tokens);
    STATE.save(deps.storage, &state)?;

    Ok(Response::default())
}

// Any address can call this.
pub fn deposit(mut deps: DepsMut, info: MessageInfo, env: Env) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let operation_controls = OPERATION_CONTROLS.load(deps.storage)?;

    if operation_controls.deposit_paused {
        return Err(ContractError::OperationPaused("deposit".to_string()));
    }

    validate(&config, &info, &env, vec![Verify::NonZeroSingleInfoFund])?;

    check_slashing(&mut deps, &env)?;

    let amount = info.funds.first().unwrap().amount;
    if amount.gt(&config.max_deposit) {
        return Err(ContractError::MaxDeposit {});
    }
    if amount.lt(&config.min_deposit) {
        return Err(ContractError::MinDeposit {});
    }
    let sender = info.sender;
    let mut state = STATE.load(deps.storage)?;

    let mut msgs = vec![];
    let deposit_breakdown = compute_deposit_breakdown(deps.storage.deref(), amount)?;

    if !deposit_breakdown.protocol_fee.is_zero() {
        msgs.push(SubMsg::new(BankMsg::Send {
            to_address: config.protocol_fee_contract.to_string(),
            amount: vec![Coin::new(
                deposit_breakdown.protocol_fee.u128(),
                config.vault_denom.clone(),
            )],
        }));
    }

    if !deposit_breakdown.staked_amount.is_zero() {
        let val_addr = get_validator_for_deposit(
            deps.querier,
            env.contract.address,
            state.validators.clone(),
        )?;

        state.total_staked = state
            .total_staked
            .checked_add(deposit_breakdown.staked_amount)
            .unwrap();
        increase_tracked_stake(&mut deps, &val_addr, deposit_breakdown.staked_amount)?;

        msgs.push(SubMsg::new(StakingMsg::Delegate {
            validator: val_addr.to_string(),
            amount: Coin::new(deposit_breakdown.staked_amount.u128(), config.vault_denom),
        }));
    }

    let mut mint_messages = vec![];
    if !deposit_breakdown.tokens_to_mint.is_zero() {
        mint_messages.push(create_mint_message(
            config.cw20_token_contract,
            deposit_breakdown.tokens_to_mint,
            sender,
        )?);
    }

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_submessages(msgs)
        .add_messages(mint_messages))
}

pub fn compute_deposit_breakdown(
    storage: &dyn Storage,
    user_amount: Uint128, // funds sent by user.
) -> Result<GetFundsDepositRecord, ContractError> {
    let config = CONFIG.load(storage)?;
    let state = STATE.load(storage)?;

    let mut amount_to_stake = user_amount;
    let mut protocol_deposit_fee = Uint128::zero();

    if !config.protocol_deposit_fee.is_zero() {
        protocol_deposit_fee = config.protocol_deposit_fee.mul(user_amount);
        amount_to_stake = user_amount
            .checked_sub(protocol_deposit_fee)
            .unwrap_or(Uint128::zero());
    }
    let mint_tokens = uint128_from_decimal(decimal_division_in_256(
        get_decimal_from_uint128(amount_to_stake),
        state.exchange_rate, // exchange rate will never be 0
    ));
    Ok(GetFundsDepositRecord {
        user_deposit_amount: user_amount,
        protocol_fee: protocol_deposit_fee,
        staked_amount: amount_to_stake,
        tokens_to_mint: mint_tokens,
    })
}

pub fn redeem_rewards(
    mut deps: DepsMut,
    _info: MessageInfo,
    env: Env,
) -> Result<Response, ContractError> {
    check_slashing(&mut deps, &env)?;
    let state = STATE.load(deps.storage)?;
    let operation_controls = OPERATION_CONTROLS.load(deps.storage)?;
    if operation_controls.redeem_rewards_paused {
        return Err(ContractError::OperationPaused("redeem_rewards".to_string()));
    }

    let mut messages = vec![];
    let mut failed_vals: Vec<String> = vec![];
    for val_addr in state.validators {
        // Skip validators that are currently jailed.
        if deps
            .querier
            .query_validator(val_addr.to_string())?
            .is_none()
            || deps
                .querier
                .query_delegation(env.contract.address.clone(), val_addr.to_string())?
                .is_none()
        {
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

// Useful to make this permissionless.
pub fn swap_rewards(deps: DepsMut, info: MessageInfo, env: Env) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;
    let operation_controls = OPERATION_CONTROLS.load(deps.storage)?;
    if operation_controls.swap_paused {
        return Err(ContractError::OperationPaused("swap".to_string()));
    }

    if info.sender.ne(&config.manager)
        && env
            .block
            .time
            .lt(&state.last_swap_time.plus_seconds(config.swap_cooldown))
    {
        return Err(ContractError::SwapInCooldown {});
    }

    state.last_swap_time = env.block.time;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: config.reward_contract.to_string(),
        msg: to_binary(&RewardExecuteMsg::Swap {})?,
        funds: vec![],
    }))
}

pub fn reinvest(mut deps: DepsMut, info: MessageInfo, env: Env) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let operation_controls = OPERATION_CONTROLS.load(deps.storage)?;
    if operation_controls.reinvest_paused {
        return Err(ContractError::OperationPaused("reinvest".to_string()));
    }

    check_slashing(&mut deps, &env)?;

    let mut state = STATE.load(deps.storage)?;

    if info.sender.ne(&config.manager)
        && env.block.time.lt(&state
            .last_reinvest_time
            .plus_seconds(config.reinvest_cooldown))
    {
        return Err(ContractError::ReinvestInCooldown {});
    }

    let balance = deps.querier.query_balance(
        config.reward_contract.to_string(),
        config.vault_denom.clone(),
    )?;

    let protocol_fee_amount = uint128_from_decimal(decimal_multiplication_in_256(
        get_decimal_from_uint128(balance.amount),
        config.protocol_reward_fee,
    ));
    let transfer_amount = balance
        .amount
        .checked_sub(protocol_fee_amount)
        .unwrap_or(Uint128::zero());

    let val_addr = get_validator_for_deposit(
        deps.querier,
        env.contract.address.clone(),
        state.validators.clone(),
    )?;
    state.total_staked = state.total_staked.checked_add(transfer_amount).unwrap();
    increase_tracked_stake(&mut deps, &val_addr, transfer_amount)?;
    state.exchange_rate = calculate_exchange_rate(
        state.total_staked,
        get_total_token_supply(deps.querier, config.cw20_token_contract)?,
    );

    state.last_reinvest_time = env.block.time;
    STATE.save(deps.storage, &state)?;

    let mut msgs = vec![];
    msgs.push(SubMsg::new(WasmMsg::Execute {
        contract_addr: config.reward_contract.to_string(),
        msg: to_binary(&RewardExecuteMsg::Transfer {
            reward_amount: transfer_amount,
            reward_withdraw_contract: env.contract.address,
            protocol_fee_amount,
            protocol_fee_contract: config.protocol_fee_contract,
        })?,
        funds: vec![],
    }));

    if !transfer_amount.is_zero() {
        msgs.push(SubMsg::new(StakingMsg::Delegate {
            validator: val_addr.to_string(),
            amount: Coin::new(transfer_amount.u128(), config.vault_denom),
        }));
    }

    // Reward contract throws an error if transfer_amount is not available to be sent over.
    Ok(Response::new().add_submessages(msgs))
}

// Useful for staking to a validator as a mechanism for filling lost slashing funds.
// Anyone call this function.
pub fn reimburse_slashing(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::NonZeroSingleInfoFund])?;
    let operation_controls = OPERATION_CONTROLS.load(deps.storage)?;
    if operation_controls.reimburse_slashing_paused {
        return Err(ContractError::OperationPaused(
            "reimburse_slashing".to_string(),
        ));
    }

    let reimburse_amount = info.funds[0].amount;
    let state = STATE.load(deps.storage)?;
    if !state.validators.contains(&val_addr) {
        return Err(ContractError::ValidatorNotAdded {});
    }

    VALIDATOR_META.update(deps.storage, &val_addr, |x| -> StdResult<_> {
        let mut vmeta = x.unwrap_or(VMeta::new());
        vmeta.filled = vmeta.filled.checked_add(reimburse_amount).unwrap();
        Ok(vmeta)
    })?;
    Ok(Response::new()
        .add_message(StakingMsg::Delegate {
            validator: val_addr.to_string(),
            amount: Coin::new(reimburse_amount.u128(), config.vault_denom),
        })
        .add_message(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::RedeemRewards {})?,
            funds: vec![],
        }))
}

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let contract_addr = info.sender.clone();
    let config = CONFIG.load(deps.storage)?;

    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::QueueUndelegate {}) => {
            // only token contract can execute this message
            if contract_addr != config.cw20_token_contract {
                return Err(ContractError::Unauthorized {});
            }
            // bchain: Note: Undelegating 0 tokens is not possible because the cw20_send call will fail
            Ok(queue_undelegation(
                deps,
                env,
                info,
                cw20_msg.amount,
                cw20_msg.sender,
            )?)
        }
        Err(_err) => Err(ContractError::NoOp {}),
    }
}

// We don't actually burn tokens here. Burning happens only during undelegation
pub fn queue_undelegation(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    amount_to_burn: Uint128,
    user_addr_str: String,
) -> Result<Response, ContractError> {
    let operation_controls = OPERATION_CONTROLS.load(deps.storage)?;
    if operation_controls.queue_undelegate_paused {
        return Err(ContractError::OperationPaused(
            "queue_undelegate".to_string(),
        ));
    }

    check_slashing(&mut deps, &env)?;

    let state = STATE.load(deps.storage)?;

    let batch_key = U64Key::new(state.current_undelegation_batch_id);
    let user_addr = deps.api.addr_validate(user_addr_str.as_str())?;

    USERS.update(
        deps.storage,
        (&user_addr, batch_key.clone()),
        |x| -> StdResult<_> {
            let mut user_current_batch_undelegations = x.unwrap_or(UndelegationInfo {
                batch_id: state.current_undelegation_batch_id,
                token_amount: Uint128::zero(),
            });
            user_current_batch_undelegations.token_amount = user_current_batch_undelegations
                .token_amount
                .checked_add(amount_to_burn)
                .unwrap();
            Ok(user_current_batch_undelegations)
        },
    )?;
    BATCH_UNDELEGATION_REGISTRY.update(deps.storage, batch_key, |x| -> StdResult<_> {
        let mut batch_undelegation = x.unwrap();
        batch_undelegation.undelegated_tokens = batch_undelegation
            .undelegated_tokens
            .checked_add(amount_to_burn)?;
        // Updated every time a new entry is added. Final update to this value happens when the actual undelegation occurs.
        // Helps the caller understand what the latest er for this batch_undelegation is.
        batch_undelegation.undelegation_er = state.exchange_rate;
        Ok(batch_undelegation)
    })?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::default())
}

pub fn undelegate_stake(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let operation_controls = OPERATION_CONTROLS.load(deps.storage)?;
    if operation_controls.undelegate_paused {
        return Err(ContractError::OperationPaused(
            "undelegate_stake".to_string(),
        ));
    }

    check_slashing(&mut deps, &env)?;

    let mut state = STATE.load(deps.storage)?;

    if info.sender.ne(&config.manager)
        && env.block.time.lt(&state
            .last_undelegation_time
            .plus_seconds(config.undelegation_cooldown))
    {
        return Err(ContractError::UndelegationInCooldown {});
    }

    let mut burn_message: Vec<WasmMsg> = vec![];
    let mut undelegate_message: Vec<StakingMsg> = vec![];
    // This is because a new batch would be created before this message is called.
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
            burn_message.push(burn_minted_tokens(
                config.cw20_token_contract.clone(),
                batch_undel.undelegated_tokens,
            )?);

            batch_undel.est_release_time =
                Some(env.block.time.plus_seconds(config.unbonding_period));
            batch_undel.undelegated_stake = Uint128::new(multiply_u128_with_decimal(
                batch_undel.undelegated_tokens.u128(),
                state.exchange_rate,
            ));
            batch_undel.undelegation_er = state.exchange_rate;
            undel_amount = batch_undel.undelegated_stake;
            Ok(batch_undel)
        },
    )?;
    let validators = state.validators.clone();
    let mut to_undelegate = undel_amount;
    let stake_tuples = get_active_validators_sorted_by_stake(
        deps.querier,
        env.contract.address.clone(),
        validators,
    )?;

    for index in (0..stake_tuples.len()).rev() {
        let tuple_val = stake_tuples.get(index).unwrap().clone();
        if to_undelegate.is_zero() {
            break;
        }
        let val_addr = Addr::unchecked(tuple_val.1);
        let amount = std::cmp::min(to_undelegate, tuple_val.0);
        undelegate_message.push(StakingMsg::Undelegate {
            validator: val_addr.to_string(),
            amount: Coin::new(amount.u128(), config.vault_denom.clone()),
        });

        decrease_tracked_stake(&mut deps, &val_addr, amount)?;
        to_undelegate = to_undelegate
            .checked_sub(amount)
            .unwrap_or_else(|_| Uint128::zero());
    }

    if !to_undelegate.is_zero() {
        return Err(ContractError::InSufficientFunds {});
    }

    state.last_undelegation_time = env.block.time;
    state.total_staked = state
        .total_staked
        .checked_sub(undel_amount)
        .unwrap_or(Uint128::zero());
    STATE.save(deps.storage, &state)?;

    // Loads the saved state.
    create_new_undelegation_batch(deps.storage, env)?;

    Ok(Response::new()
        .add_messages(undelegate_message)
        .add_messages(burn_message)
        .add_attribute("Undelegation_amount", undel_amount.to_string()))
}

// No need for regular slashing check here because these funds have been undelegated 21 days ago and
// we are now checking if there was slashing in these 21 days for these funds.
pub fn reconcile_funds(
    deps: DepsMut,
    _info: MessageInfo,
    env: Env,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let operation_controls = OPERATION_CONTROLS.load(deps.storage)?;
    if operation_controls.reconcile_paused {
        return Err(ContractError::OperationPaused(
            "reconcile_funds".to_string(),
        ));
    }

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
        return Ok(Response::default());
    }

    // QUERY the base funds and check how much can be reconciled
    let contract_balance = deps
        .querier
        .query_balance(env.contract.address.to_string(), config.vault_denom)?;

    let unaccounted_funds = contract_balance
        .amount
        .checked_sub(state.reconciled_funds_to_withdraw)
        .unwrap_or(Uint128::zero());
    if unaccounted_funds.is_zero() {
        return Err(ContractError::ZeroAmount {});
    }

    // Slashing may have occured in the 21 day unbonding period. Capture that.
    let unbonding_slashing_ratio = std::cmp::min(
        Decimal::from_ratio(unaccounted_funds, total_stake_expected),
        Decimal::one(),
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
        BATCH_UNDELEGATION_REGISTRY.save(deps.storage, key, &batch_meta)?;
    }

    state.reconciled_funds_to_withdraw = state
        .reconciled_funds_to_withdraw
        .checked_add(std::cmp::min(unaccounted_funds, total_stake_expected))
        .unwrap();
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
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let operation_controls = OPERATION_CONTROLS.load(deps.storage)?;
    if operation_controls.withdraw_paused {
        return Err(ContractError::OperationPaused("withdraw".to_string()));
    }

    let mut state = STATE.load(deps.storage)?;
    let user_addr = deps.api.addr_validate(info.sender.as_str())?;
    let funds_record = compute_withdrawable_funds(deps.storage.deref(), batch_id, &user_addr)?;
    let mut msgs = vec![];

    if !funds_record.user_withdrawal_amount.is_zero() {
        state.reconciled_funds_to_withdraw = state
            .reconciled_funds_to_withdraw
            .checked_sub(funds_record.user_withdrawal_amount)
            .unwrap_or(Uint128::zero());
        msgs.push(BankMsg::Send {
            to_address: user_addr.to_string(),
            amount: vec![Coin::new(
                funds_record.user_withdrawal_amount.u128(),
                config.vault_denom.clone(),
            )],
        });
    }
    if !funds_record.protocol_fee.is_zero() {
        state.reconciled_funds_to_withdraw = state
            .reconciled_funds_to_withdraw
            .checked_sub(funds_record.protocol_fee)
            .unwrap_or(Uint128::zero());
        msgs.push(BankMsg::Send {
            to_address: config.protocol_fee_contract.to_string(),
            amount: vec![Coin::new(
                funds_record.protocol_fee.u128(),
                config.vault_denom,
            )],
        });
    }

    STATE.save(deps.storage, &state)?;
    USERS.remove(deps.storage, (&user_addr, U64Key::new(batch_id)));
    Ok(Response::new().add_messages(msgs))
}

// Does not change any state. Used for both messages & queries
pub fn compute_withdrawable_funds(
    storage: &dyn Storage,
    batch_id: u64,
    user_addr: &Addr,
) -> Result<GetFundsClaimRecord, ContractError> {
    let config = CONFIG.load(storage)?;

    let und_opt = BATCH_UNDELEGATION_REGISTRY.may_load(storage, U64Key::new(batch_id))?;
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
    let user_undelegated_amount = multiply_u128_with_decimal(
        user_undelegation.token_amount.u128(),
        und_batch.undelegation_er,
    );

    let claimable_amount =
        multiply_u128_with_decimal(user_undelegated_amount, und_batch.unbonding_slashing_ratio);

    let protocol_fee = multiply_u128_with_decimal(claimable_amount, config.protocol_withdraw_fee);

    let user_withdrawal_amount = claimable_amount.checked_sub(protocol_fee).unwrap_or(0_u128);
    Ok(GetFundsClaimRecord {
        user_withdrawal_amount: Uint128::new(user_withdrawal_amount),
        protocol_fee: Uint128::new(protocol_fee),
        undelegated_tokens: user_undelegation.token_amount,
    })
}

// Can be permissionless and no check_slashing reqd because all airdrops are drained.
pub fn claim_airdrops(
    deps: DepsMut,
    _info: MessageInfo,
    _env: Env,
    airdrop_rates: Vec<AirdropRate>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let operation_controls = OPERATION_CONTROLS.load(deps.storage)?;
    if operation_controls.claim_airdrops_paused {
        return Err(ContractError::OperationPaused("claim_airdrops".to_string()));
    }

    let mut msgs = vec![];
    let airdrop_withdrawal_contract = config.airdrop_withdrawal_contract;
    let airdrops_registry_contract = config.airdrop_registry_contract;
    for rate in airdrop_rates {
        if rate.amount.is_zero() {
            continue;
        }

        let contract_response: GetAirdropContractsResponse = get_airdrop_contracts(
            deps.querier,
            airdrops_registry_contract.clone(),
            rate.denom.clone(),
        )?;

        let contracts = if let Some(contracts) = contract_response.contracts {
            contracts
        } else {
            return Err(ContractError::AirdropNotRegistered(rate.denom));
        };

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
            })?,
            funds: vec![],
        })
    }

    Ok(Response::new().add_messages(msgs))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::OperationControls {} => to_binary(&query_operation_controls(deps)?),
        QueryMsg::BatchUndelegation { batch_id } => {
            to_binary(&query_batch_undelegate(deps, batch_id)?)
        }
        QueryMsg::GetUserUndelegationRecords {
            user_addr,
            start_after,
            limit,
        } => to_binary(&query_user_undelegation_records(
            deps,
            user_addr,
            start_after,
            limit,
        )?),
        QueryMsg::GetValMeta { val_addr } => to_binary(&query_val_meta(deps, val_addr)?),
        QueryMsg::GetUserInfo { user_addr } => to_binary(&query_user_info(deps, user_addr)?),
        QueryMsg::ComputeDepositBreakdown { amount } => {
            to_binary(&query_compute_deposit_breakdown(deps, amount)?)
        }
        QueryMsg::GetUserUndelegationInfo {
            user_addr,
            batch_id,
        } => to_binary(&query_user_undelegation_info(deps, user_addr, batch_id)?),
        QueryMsg::TmpManagerStore {} => to_binary(&query_manager_tmp_store(deps)?),
    }
}

pub fn query_manager_tmp_store(deps: Deps) -> StdResult<TmpManagerStoreResponse> {
    let tmp_manager_store = TMP_MANAGER_STORE.may_load(deps.storage)?;
    Ok(TmpManagerStoreResponse { tmp_manager_store })
}

pub fn query_operation_controls(deps: Deps) -> StdResult<OperationControls> {
    let operation_controls = OPERATION_CONTROLS.load(deps.storage)?;
    Ok(operation_controls)
}

pub fn query_user_info(deps: Deps, user_addr: String) -> StdResult<UserInfoResponse> {
    let user_addr = deps.api.addr_validate(user_addr.as_str())?;
    let config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;

    let user_token_balance = get_user_balance(deps.querier, config.cw20_token_contract, user_addr)?;
    let user_amount = state.exchange_rate.mul(user_token_balance);

    Ok(UserInfoResponse {
        user_info: UserQueryInfo {
            total_tokens: user_token_balance,
            total_amount: Coin::new(user_amount.u128(), "uluna".to_string()),
        },
    })
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
    let batch_meta = BATCH_UNDELEGATION_REGISTRY.may_load(deps.storage, U64Key::new(batch_id))?;
    Ok(QueryBatchUndelegationResponse { batch: batch_meta })
}

pub fn query_user_undelegation_records(
    deps: Deps,
    user_addr_str: String,
    start_after: Option<u64>,
    limit: Option<u64>,
) -> StdResult<Vec<UndelegationInfo>> {
    let user_addr = deps.api.addr_validate(user_addr_str.as_str())?;
    let limit = limit.unwrap_or(10).min(20) as usize;
    let start = start_after.map(|batch_id| Bound::exclusive(U64Key::new(batch_id)));

    let user_undelegations = USERS
        .prefix(&user_addr)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| item.unwrap().1)
        .collect::<Vec<UndelegationInfo>>();

    return Ok(user_undelegations);
}

pub fn query_val_meta(deps: Deps, val_addr: Addr) -> StdResult<GetValMetaResponse> {
    let val_meta_opt = VALIDATOR_META.may_load(deps.storage, &val_addr)?;
    Ok(GetValMetaResponse {
        val_meta: val_meta_opt,
    })
}

pub fn query_user_undelegation_info(
    deps: Deps,
    user_addr: String,
    batch_id: u64,
) -> StdResult<GetFundsClaimRecord> {
    let user_addr = deps.api.addr_validate(user_addr.as_str())?;
    let res = compute_withdrawable_funds(deps.storage, batch_id, &user_addr);
    if res.is_err() {
        return Err(StdError::GenericErr {
            msg: "Error in computing the withdrawable funds".to_string(),
        });
    }

    let funds_record = res.unwrap();

    Ok(funds_record)
}

pub fn query_compute_deposit_breakdown(
    deps: Deps,
    amount: Uint128,
) -> StdResult<GetFundsDepositRecord> {
    let res = compute_deposit_breakdown(deps.storage, amount);
    if res.is_err() {
        return Err(StdError::GenericErr {
            msg: "Error in computing the deposit breakdown".to_string(),
        });
    }

    let deposit_breakdown = res.unwrap();
    Ok(deposit_breakdown)
}
