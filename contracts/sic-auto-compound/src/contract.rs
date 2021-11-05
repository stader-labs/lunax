#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Coin, Deps, DepsMut, DistributionMsg, Env, MessageInfo, Response,
    StakingMsg, StdResult, Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::helpers::{
    get_pool_stake_info, get_reward_tokens, get_unaccounted_funds, get_validator_for_deposit,
};
use crate::msg::{
    ExecuteMsg, GetFulfillableUndelegatedFundsResponse, GetStateResponse, GetTotalTokensResponse,
    InstantiateMsg, MigrateMsg, QueryMsg,
};
use crate::state::{State, STATE};
use cw2::set_contract_version;
use reward::msg::ExecuteMsg as reward_execute;
use stader_utils::helpers::send_funds_msg;
use std::cmp::min;
use terra_cosmwasm::TerraMsgWrapper;

const CONTRACT_NAME: &str = "sic-auto-compound";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        manager: info.sender.clone(),
        scc_address: deps.api.addr_validate(msg.scc_address.as_str())?,
        reward_contract_address: deps
            .api
            .addr_validate(msg.reward_contract_address.as_str())?,
        manager_seed_funds: msg.manager_seed_funds,
        min_validator_pool_size: msg.min_validator_pool_size.unwrap_or(3),
        strategy_denom: "uluna".to_string(),
        contract_genesis_block_height: _env.block.height,
        contract_genesis_timestamp: _env.block.time,
        validator_pool: msg.initial_validators,
    };

    STATE.save(deps.storage, &state)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_message(DistributionMsg::SetWithdrawAddress {
            address: msg.reward_contract_address,
        })
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender))
}

pub fn migrate(
    _deps: DepsMut,
    _env: Env,
    _msg: MigrateMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    match msg {
        ExecuteMsg::TransferRewards {} => transfer_rewards(deps, _env, info),
        ExecuteMsg::UndelegateRewards { amount } => undelegate_rewards(deps, _env, info, amount),
        ExecuteMsg::Reinvest {
            invest_transferred_rewards,
        } => reinvest(deps, _env, info, invest_transferred_rewards),
        ExecuteMsg::RedeemRewards {} => redeem_rewards(deps, _env, info),
        ExecuteMsg::Swap {} => swap(deps, _env, info),
        ExecuteMsg::ClaimAirdrops {
            airdrop_token_contract,
            cw20_token_contract,
            airdrop_token,
            amount,
            claim_msg,
        } => claim_airdrops(
            deps,
            _env,
            info,
            airdrop_token_contract,
            cw20_token_contract,
            airdrop_token,
            amount,
            claim_msg,
        ),
        ExecuteMsg::TransferUndelegatedRewards { amount } => {
            transfer_undelegated_rewards(deps, _env, info, amount)
        }
        ExecuteMsg::AddValidator { validator } => add_validator(deps, _env, info, validator),
        ExecuteMsg::ReplaceValidator {
            src_validator,
            dst_validator,
        } => replace_validator(deps, _env, info, src_validator, dst_validator),
        ExecuteMsg::RemoveValidator {
            removed_val,
            redelegate_val,
        } => remove_validator(deps, _env, info, removed_val, redelegate_val),
        ExecuteMsg::UpdateConfig {
            min_validator_pool_size,
            scc_address,
        } => update_config(deps, _env, info, min_validator_pool_size, scc_address),
        ExecuteMsg::SetRewardWithdrawAddress { reward_contract } => {
            set_reward_withdraw_address(deps, _env, info, reward_contract)
        }
    }
}

pub fn set_reward_withdraw_address(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    reward_contract: String,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    state.reward_contract_address = deps.api.addr_validate(reward_contract.as_str())?;
    STATE.save(deps.storage, &state)?;

    Ok(
        Response::new().add_message(DistributionMsg::SetWithdrawAddress {
            address: reward_contract.to_string(),
        }),
    )
}

pub fn update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    min_validator_pool_size: Option<u64>,
    scc_address: Option<String>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(mvps) = min_validator_pool_size {
        state.min_validator_pool_size = mvps;
    }

    if let Some(sa) = scc_address {
        state.scc_address = deps.api.addr_validate(sa.as_str())?;
    }

    STATE.save(deps.storage, &state)?;

    Ok(Response::default())
}

pub fn remove_validator(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    removed_val: String,
    redelegate_val: String,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    let removed_val = Addr::unchecked(removed_val);
    let redelegate_val = Addr::unchecked(redelegate_val);

    if !state.validator_pool.contains(&removed_val) {
        return Err(ContractError::ValidatorNotInPool {});
    }

    if state
        .validator_pool
        .len()
        .le(&(state.min_validator_pool_size as usize))
    {
        return Err(ContractError::CannotRemoveMoreValidators {});
    }

    if deps
        .querier
        .query_validator(redelegate_val.to_string())
        .unwrap_or(None)
        .is_none()
    {
        return Err(ContractError::ValidatorDoesNotExist {});
    }

    if !state.validator_pool.contains(&redelegate_val) {
        state.validator_pool.push(redelegate_val.clone());
    }

    let validator_delegation_opt = deps
        .querier
        .query_delegation(&_env.contract.address, removed_val.to_string())?;

    let new_validator_pool: Vec<Addr> = state
        .validator_pool
        .into_iter()
        .filter(|x| x.ne(&removed_val))
        .collect::<Vec<Addr>>();
    let mut redelegation_messages: Vec<StakingMsg> = vec![];
    if let Some(validator_delegation) = validator_delegation_opt {
        let validator_staked_coin = validator_delegation.amount;

        redelegation_messages.push(StakingMsg::Redelegate {
            src_validator: removed_val.to_string(),
            dst_validator: redelegate_val.to_string(),
            amount: validator_staked_coin,
        });
    }

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.validator_pool = new_validator_pool;
        Ok(state)
    })?;

    Ok(Response::new().add_messages(redelegation_messages))
}

pub fn replace_validator(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    src_validator: String,
    dst_validator: String,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    let src_validator = Addr::unchecked(src_validator);
    let dst_validator = Addr::unchecked(dst_validator);

    if src_validator.eq(&dst_validator) {
        return Ok(Response::default());
    }

    if !state.validator_pool.contains(&src_validator) {
        return Err(ContractError::ValidatorNotInPool {});
    }

    if state.validator_pool.contains(&dst_validator) {
        return Err(ContractError::ValidatorAlreadyExistsInPool {});
    }

    // check if validator is present in the blockchain
    if deps
        .querier
        .query_validator(dst_validator.to_string())
        .unwrap_or(None)
        .is_none()
    {
        return Err(ContractError::ValidatorDoesNotExist {});
    }

    let src_validator_delegation_opt = deps.querier.query_delegation(
        &_env.contract.address.to_string(),
        src_validator.to_string(),
    )?;

    let mut redelegation_msgs: Vec<StakingMsg> = vec![];
    if let Some(src_validator_delegation) = src_validator_delegation_opt {
        // send redelegation message only if src_validator has a redelegation
        redelegation_msgs.push(StakingMsg::Redelegate {
            src_validator: src_validator.to_string(),
            dst_validator: dst_validator.to_string(),
            amount: src_validator_delegation.can_redelegate,
        });
    }

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.validator_pool = state
            .validator_pool
            .into_iter()
            .filter(|x| x.ne(&src_validator))
            .collect();
        state.validator_pool.push(dst_validator);
        Ok(state)
    })?;

    Ok(Response::new().add_messages(redelegation_msgs))
}

pub fn add_validator(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    validator: String,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    let validator = Addr::unchecked(validator);
    // check if validator is already in the pool
    if state.validator_pool.contains(&validator) {
        return Err(ContractError::ValidatorAlreadyExistsInPool {});
    }

    // check if validator is present in the blockchain
    if deps
        .querier
        .query_validator(validator.to_string())
        .unwrap_or(None)
        .is_none()
    {
        return Err(ContractError::ValidatorDoesNotExist {});
    }

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.validator_pool.push(validator);
        Ok(state)
    })?;

    Ok(Response::default())
}

#[allow(clippy::too_many_arguments)]
pub fn claim_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    airdrop_token_contract: String,
    cw20_token_contract: String,
    _airdrop_token: String,
    amount: Uint128,
    claim_msg: Binary,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;
    if info.sender != state.scc_address {
        return Err(ContractError::Unauthorized {});
    }

    let airdrop_token_contract = deps.api.addr_validate(airdrop_token_contract.as_str())?;
    let cw20_token_contract = deps.api.addr_validate(cw20_token_contract.as_str())?;
    // this wasm-msg will transfer the airdrops from the airdrop cw20 token contract to the
    // SIC contract
    let mut messages: Vec<WasmMsg> = vec![WasmMsg::Execute {
        contract_addr: airdrop_token_contract.to_string(),
        msg: claim_msg,
        funds: vec![],
    }];

    // this wasm message will transfer the ownership from SIC to SCC
    messages.push(WasmMsg::Execute {
        contract_addr: cw20_token_contract.to_string(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
            recipient: state.scc_address.to_string(),
            amount,
        })
        .unwrap(),
        funds: vec![],
    });

    Ok(Response::new().add_messages(messages))
}

pub fn swap(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;

    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: state.reward_contract_address.to_string(),
        msg: to_binary(&reward_execute::Swap {}).unwrap(),
        funds: vec![],
    }))
}

pub fn transfer_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.scc_address {
        return Err(ContractError::Unauthorized {});
    }

    // check if any money is being sent
    if info.funds.is_empty() {
        return Ok(Response::new().add_attribute("no_funds_sent", "1"));
    }

    // accept only one coin
    if info.funds.len() > 1 {
        return Ok(Response::new().add_attribute("multiple_coins_passed", "1"));
    }

    let transferred_coin = info.funds[0].clone();
    if transferred_coin.denom.ne(&state.strategy_denom) {
        return Ok(Response::new().add_attribute("transferred_denom_is_wrong", "1"));
    }

    // reinvest the rewards immediately after a transfer. This is because when transfer rewards
    // is called, withdrawable shares are already allocated to the user.
    Ok(Response::new().add_messages(vec![WasmMsg::Execute {
        contract_addr: String::from(_env.contract.address),
        msg: to_binary(&ExecuteMsg::Reinvest {
            invest_transferred_rewards: Some(true),
        })
        .unwrap(),
        funds: vec![transferred_coin],
    }]))
}

// SCC needs to call this when it processes the undelegations.
// SCC is responsible for batching up the user undelegation requests. It sends the batched up
// undelegated amount to the SIC
pub fn undelegate_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage).unwrap();

    if info.sender != state.scc_address {
        return Err(ContractError::Unauthorized {});
    }

    if amount.is_zero() {
        return Err(ContractError::ZeroUndelegation {});
    }

    let pool_stake_info = get_pool_stake_info(
        deps.querier,
        _env.contract.address.to_string(),
        state.validator_pool,
    )?;
    let validator_stake_map = pool_stake_info.0;
    let total_staked_amount = pool_stake_info.1;
    if amount.gt(&total_staked_amount) {
        return Err(ContractError::NotEnoughFundsToUndelegate {});
    }

    // undelegate from each validator according to their staked fraction
    let mut messages: Vec<StakingMsg> = vec![];
    let strategy_denom = state.strategy_denom;
    let mut amount_to_undelegate = amount;
    let mut sorted_validator_stake_map: Vec<(&Addr, &Uint128)> = validator_stake_map
        .iter()
        .collect::<Vec<(&Addr, &Uint128)>>();
    sorted_validator_stake_map.sort();

    for v in sorted_validator_stake_map.iter() {
        if amount_to_undelegate.is_zero() {
            break;
        }

        let v = *v;
        let validator_addr = v.0.clone();
        let validator_stake_amount = *v.1;

        let undelegatable_amount: Uint128;
        if validator_stake_amount.gt(&amount_to_undelegate) {
            undelegatable_amount = amount_to_undelegate;
            amount_to_undelegate = Uint128::zero();
        } else {
            amount_to_undelegate = amount_to_undelegate
                .checked_sub(validator_stake_amount)
                .unwrap();
            undelegatable_amount = validator_stake_amount;
        }

        messages.push(StakingMsg::Undelegate {
            validator: validator_addr.to_string(),
            amount: Coin::new(undelegatable_amount.u128(), strategy_denom.clone()),
        });
    }

    Ok(Response::new().add_messages(messages))
}

pub fn transfer_undelegated_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.scc_address {
        return Err(ContractError::Unauthorized {});
    }

    let unaccounted_funds = get_unaccounted_funds(deps.querier, _env.contract.address, &state);

    // this way of handling slashing makes us more optimistic while handling undelegation slashing.
    // We have to give the user a warning when they remove their funds that it may potentially be slashed
    // during undelegation. here undelegation slashing is moved to the end. Let's take the following example
    // Undelegation 1: Expected 800, got back 780
    // Undelegation 2: Expected 600, got back 500
    // Undelegation 3: Expected 600, got back 600
    // When SCC requests the 800, we give back 800. Then when SCC requests 600, we give the 600.
    // When SCC finally requests 600, we give 480
    let total_funds_to_send = min(unaccounted_funds, amount);

    // no need to account for the undelegated funds separately as it will be deducted from the contract balance
    Ok(Response::new().add_message(send_funds_msg(
        &state.scc_address,
        &[Coin::new(total_funds_to_send.u128(), state.strategy_denom)],
    )))
}

pub fn reinvest(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    // if none, then we also invest rewards claimed from the validator otherwise its
    // only rewards transferred
    invest_transferred_rewards: Option<bool>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;

    // TODO - bchain99: add validation templates. discuss with gm about pushing it to stader-utils
    if info.sender != state.manager && info.sender != _env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    let mut rewards_to_invest: Uint128 = Uint128::zero();
    let mut reward_contract_messages: Vec<WasmMsg> = vec![];
    // query the rewards contracts
    if invest_transferred_rewards.is_none() {
        rewards_to_invest = get_reward_tokens(deps.querier, state.reward_contract_address.clone())?;
        if !rewards_to_invest.is_zero() {
            reward_contract_messages.push(WasmMsg::Execute {
                contract_addr: state.reward_contract_address.to_string(),
                msg: to_binary(&reward_execute::Transfer {
                    reward_amount: rewards_to_invest,
                    reward_withdraw_contract: _env.contract.address.clone(),
                    protocol_fee_amount: Uint128::zero(),
                    protocol_fee_contract: _env.contract.address.clone(),
                })
                .unwrap(),
                funds: vec![],
            });
        }
    }

    // validate the transferred coin
    let transferred_coin_amount = if !info.funds.is_empty() {
        if info.funds.len() > 1 {
            return Err(ContractError::MultipleCoins {});
        }
        let transferred_coin = info.funds[0].clone();
        if transferred_coin.denom.ne(&state.strategy_denom) {
            // throw an error here because this is an unacceptable situations
            return Err(ContractError::WrongDenom(transferred_coin.denom));
        }
        transferred_coin.amount
    } else {
        Uint128::zero()
    };

    rewards_to_invest = rewards_to_invest
        .checked_add(transferred_coin_amount)
        .unwrap();
    if rewards_to_invest.is_zero() {
        return Ok(Response::new().add_attribute("no_rewards_to_reinvest", "1"));
    }

    let strategy_denom = state.strategy_denom;

    // only split the stake amongst the non-jailed validators
    let mut staking_messages: Vec<StakingMsg> = vec![];
    let deposit_val = get_validator_for_deposit(
        deps.querier,
        _env.contract.address.to_string(),
        state.validator_pool,
    )?;
    staking_messages.push(StakingMsg::Delegate {
        validator: deposit_val.to_string(),
        amount: Coin::new(rewards_to_invest.u128(), strategy_denom),
    });

    Ok(Response::new()
        .add_messages(reward_contract_messages)
        .add_messages(staking_messages))
}

pub fn redeem_rewards(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;
    if info.sender != state.manager && info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    let mut messages: Vec<DistributionMsg> = vec![];

    let pool_stake_info = get_pool_stake_info(
        deps.querier,
        env.contract.address.to_string(),
        state.validator_pool,
    )?;
    let validator_pool = pool_stake_info.0;
    for validator in &validator_pool {
        let val_addr = validator.0.clone();

        messages.push(DistributionMsg::WithdrawDelegatorReward {
            validator: val_addr.to_string(),
        });
    }

    Ok(Response::new().add_messages(messages))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetTotalTokens {} => to_binary(&query_total_tokens(deps, _env)?),
        QueryMsg::GetState {} => to_binary(&query_state(deps)?),
        QueryMsg::GetFulfillableUndelegatedFunds { amount } => {
            to_binary(&query_fulfillable_undelegated_funds(deps, _env, amount)?)
        }
    }
}

fn query_state(deps: Deps) -> StdResult<GetStateResponse> {
    let state = STATE.may_load(deps.storage).unwrap();

    Ok(GetStateResponse { state })
}

fn query_total_tokens(deps: Deps, _env: Env) -> StdResult<GetTotalTokensResponse> {
    let state = STATE.load(deps.storage)?;
    let pool_stake_info = get_pool_stake_info(
        deps.querier,
        _env.contract.address.to_string(),
        state.validator_pool,
    )?;
    Ok(GetTotalTokensResponse {
        total_tokens: Some(pool_stake_info.1),
    })
}

fn query_fulfillable_undelegated_funds(
    deps: Deps,
    env: Env,
    amount: Uint128,
) -> StdResult<GetFulfillableUndelegatedFundsResponse> {
    let state = STATE.load(deps.storage)?;

    let unaccounted_funds = get_unaccounted_funds(deps.querier, env.contract.address, &state);

    Ok(GetFulfillableUndelegatedFundsResponse {
        undelegated_funds: Some(min(unaccounted_funds, amount)),
    })
}
