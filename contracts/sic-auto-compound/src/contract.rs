#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Attribute, Binary, Coin, Decimal, Deps, DepsMut, DistributionMsg, Env,
    Fraction, MessageInfo, Response, StakingMsg, StdResult, Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::helpers::get_unaccounted_funds;
use crate::msg::{
    ExecuteMsg, GetFulfillableUndelegatedFundsResponse, GetStateResponse, GetTotalTokensResponse,
    InstantiateMsg, MigrateMsg, QueryMsg,
};
use crate::state::{StakeQuota, State, STATE, VALIDATORS_TO_STAKED_QUOTA};
use stader_utils::coin_utils::{merge_coin, merge_coin_vector, CoinOp, CoinVecOp, Operation};
use stader_utils::helpers::send_funds_msg;
use std::cmp::min;
use std::collections::HashMap;
use terra_cosmwasm::{create_swap_msg, SwapResponse, TerraMsgWrapper, TerraQuerier};

pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        manager: info.sender.clone(),
        scc_address: msg.scc_address,
        manager_seed_funds: msg.manager_seed_funds,
        strategy_denom: msg.strategy_denom.clone(),
        contract_genesis_block_height: _env.block.height,
        contract_genesis_timestamp: _env.block.time,
        validator_pool: msg.initial_validators,
        unswapped_rewards: vec![],
        uninvested_rewards: Coin::new(0_u128, msg.strategy_denom),

        total_staked_tokens: Uint128::zero(),
        total_slashed_amount: Uint128::zero(),
    };

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender))
}

pub fn migrate(
    deps: DepsMut,
    _env: Env,
    msg: MigrateMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    Ok(Response::default())
}

pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    match msg {
        ExecuteMsg::TransferRewards {} => try_transfer_rewards(deps, _env, info),
        ExecuteMsg::UndelegateRewards { amount } => {
            try_undelegate_rewards(deps, _env, info, amount)
        }
        ExecuteMsg::Reinvest {} => try_reinvest(deps, _env, info),
        ExecuteMsg::RedeemRewards {} => try_redeem_rewards(deps, _env, info),
        ExecuteMsg::Swap {} => try_swap(deps, _env, info),
        ExecuteMsg::ClaimAirdrops {
            airdrop_token_contract,
            cw20_token_contract,
            airdrop_token,
            amount,
            claim_msg,
        } => try_claim_airdrops(
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
            try_transfer_undelegated_rewards(deps, _env, info, amount)
        }
    }
}

pub fn try_claim_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    airdrop_token_contract: Addr,
    cw20_token_contract: Addr,
    _airdrop_token: String,
    amount: Uint128,
    claim_msg: Binary,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.scc_address {
        return Err(ContractError::Unauthorized {});
    }

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

pub fn try_swap(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;

    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    if state.unswapped_rewards.is_empty() {
        return Ok(Response::new().add_attribute("no_unswapped_rewards", "1"));
    }

    // fetch the swapped money
    let strategy_denom = state.strategy_denom;
    let mut logs: Vec<Attribute> = vec![];
    let mut swapped_coin: Coin = Coin::new(0_u128, strategy_denom.clone());
    let terra_querier = TerraQuerier::new(&deps.querier);
    let mut failed_coins: Vec<Coin> = vec![];
    let mut messages = vec![];
    for reward_coin in state.unswapped_rewards {
        let mut swapped_out_coin = reward_coin.clone();

        if swapped_out_coin.denom.ne(&strategy_denom) {
            let coin_swap_wrapped =
                terra_querier.query_swap(reward_coin.clone(), strategy_denom.clone());
            // TODO: bchain99 - I think this could mean that there is no swap possible for the pair.
            if coin_swap_wrapped.is_err() {
                // TODO: bchain99 - Check if this is needed. Check the cases when the query_swap can fail.
                logs.push(attr("failed_to_swap", reward_coin.to_string()));
                failed_coins.push(reward_coin);
                continue;
            }

            messages.push(create_swap_msg(reward_coin, strategy_denom.clone()));

            let coin_swap: SwapResponse = coin_swap_wrapped.unwrap();
            swapped_out_coin = coin_swap.receive;
        }

        swapped_coin = merge_coin(
            swapped_coin,
            CoinOp {
                fund: swapped_out_coin,
                operation: Operation::Add,
            },
        );
    }

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        // empty out the unstaked rewards after
        state.unswapped_rewards = state
            .unswapped_rewards
            .into_iter()
            .filter(|coin| failed_coins.contains(coin))
            .collect();
        state.uninvested_rewards = merge_coin(
            state.uninvested_rewards,
            CoinOp {
                fund: swapped_coin.clone(),
                operation: Operation::Add,
            },
        );
        Ok(state)
    })?;

    logs.push(attr("total_swapped_rewards", swapped_coin.to_string()));

    Ok(Response::new().add_messages(messages).add_attributes(logs))
}

pub fn try_transfer_rewards(
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

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.uninvested_rewards = merge_coin(
            state.uninvested_rewards,
            CoinOp {
                fund: transferred_coin,
                operation: Operation::Add,
            },
        );
        Ok(state)
    })?;

    // reinvest the rewards immediately after a transfer. This is because when transfer rewards
    // is called, withdrawable shares are already allocated to the user.
    Ok(Response::new().add_messages(vec![
        WasmMsg::Execute {
            contract_addr: String::from(_env.contract.address.clone()),
            msg: to_binary(&ExecuteMsg::RedeemRewards {}).unwrap(),
            funds: vec![],
        },
        WasmMsg::Execute {
            contract_addr: String::from(_env.contract.address),
            msg: to_binary(&ExecuteMsg::Reinvest {}).unwrap(),
            funds: vec![],
        },
    ]))
}

// SCC needs to call this when it processes the undelegations.
// SCC is responsible for batching up the user undelegation requests. It sends the batched up
// undelegated amount to the SIC
pub fn try_undelegate_rewards(
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
        return Ok(Response::new().add_attribute("undelegated_zero_funds", "1"));
    }

    if amount.gt(&state.total_staked_tokens) {
        return Ok(Response::new().add_attribute("amount_greater_than_total_tokens", "1"));
    }

    let new_total_staked_tokens = state.total_staked_tokens.checked_sub(amount).unwrap();
    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.total_staked_tokens = new_total_staked_tokens;
        Ok(state)
    })?;

    // undelegate from each validator according to their staked fraction
    let mut messages: Vec<StakingMsg> = vec![];
    let strategy_denom = state.strategy_denom;
    for validator in &state.validator_pool {
        let validator_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.storage, validator)
            .unwrap();
        if validator_staked_quota_option.is_none() {
            // validator has no stake. so don't undelegate from him.
            continue;
        }

        let validator_staked_quota = validator_staked_quota_option.unwrap();
        let total_delegated_amount = validator_staked_quota.amount.amount;
        let stake_fraction = validator_staked_quota.stake_fraction;

        let mut unstake_amount = Uint128::zero();
        if !stake_fraction.is_zero() {
            unstake_amount = Uint128::new(
                amount.u128() * stake_fraction.numerator() / stake_fraction.denominator(),
            );

            messages.push(StakingMsg::Undelegate {
                validator: String::from(validator),
                amount: Coin {
                    denom: strategy_denom.clone(),
                    amount: unstake_amount,
                },
            });
        }

        let new_validator_staked_amount = total_delegated_amount
            .checked_sub(unstake_amount)
            .unwrap_or(Uint128::zero()) // to avoid any overflows
            .u128();
        VALIDATORS_TO_STAKED_QUOTA.save(
            deps.storage,
            validator,
            &StakeQuota {
                amount: Coin::new(new_validator_staked_amount, strategy_denom.clone()),
                stake_fraction: Decimal::from_ratio(
                    new_validator_staked_amount,
                    new_total_staked_tokens,
                ),
            },
        )?;
    }

    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: _env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::RedeemRewards {}).unwrap(),
            funds: vec![],
        })
        .add_messages(messages))
}

pub fn try_transfer_undelegated_rewards(
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
        return Ok(Response::new().add_attribute("undelegated_zero_funds", "1"));
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
        &vec![Coin::new(total_funds_to_send.u128(), state.strategy_denom)],
    )))
}

pub fn try_reinvest(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;

    // TODO - bchain99: add validation templates. discuss with gm about pushing it to stader-utils
    if info.sender != state.manager && info.sender != _env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    if state.uninvested_rewards.amount.is_zero() {
        return Ok(Response::new().add_attribute("no_uninvested_rewards", "1"));
    }

    let strategy_denom = state.strategy_denom;
    let mut current_total_staked_tokens = Uint128::zero();
    let mut validator_to_delegation_map: HashMap<&Addr, Uint128> = HashMap::new();
    for validator in &state.validator_pool {
        let result = deps
            .querier
            .query_delegation(&_env.contract.address, validator)?;
        // this will happen if there is no delegation to the validator
        if result.is_none() {
            continue;
        }

        let full_delegation = result.unwrap();

        validator_to_delegation_map.insert(validator, full_delegation.amount.amount);

        current_total_staked_tokens = current_total_staked_tokens
            .checked_add(full_delegation.amount.amount)
            .unwrap();
    }

    let total_slashed_amount = state
        .total_staked_tokens
        .checked_sub(current_total_staked_tokens)
        .unwrap_or_else(|_| Uint128::zero());

    let rewards_to_invest = state.uninvested_rewards.amount;

    let new_current_staked_tokens = current_total_staked_tokens
        .checked_add(rewards_to_invest)
        .unwrap();

    let validator_pool_length = state.validator_pool.len();
    let even_split = rewards_to_invest.u128() / validator_pool_length as u128;
    let mut extra_split = rewards_to_invest.u128() % validator_pool_length as u128;
    let mut messages: Vec<StakingMsg> = vec![];
    state.validator_pool.iter().for_each(|v| {
        let delegation_amount = Uint128::new(even_split + extra_split);
        if !delegation_amount.is_zero() {
            messages.push(StakingMsg::Delegate {
                validator: v.to_string(),
                amount: Coin {
                    denom: strategy_denom.clone(),
                    amount: delegation_amount,
                },
            });
        }

        let current_validator_staked_amount = *(validator_to_delegation_map
            .get(v)
            .unwrap_or(&Uint128::zero()));
        let new_validator_staked_amount = current_validator_staked_amount
            .checked_add(delegation_amount)
            .unwrap();
        // validator stake quota will get updated as we are reconciling the validator stake
        let new_validator_stake_quota: StakeQuota = StakeQuota {
            amount: Coin {
                denom: strategy_denom.clone(),
                amount: new_validator_staked_amount,
            },
            stake_fraction: Decimal::from_ratio(
                new_validator_staked_amount,
                new_current_staked_tokens,
            ),
        };

        VALIDATORS_TO_STAKED_QUOTA
            .save(deps.storage, v, &new_validator_stake_quota)
            .unwrap();

        extra_split = 0_u128;
    });

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.total_staked_tokens = new_current_staked_tokens;
        state.total_slashed_amount = state
            .total_slashed_amount
            .checked_add(total_slashed_amount)
            .unwrap();
        state.uninvested_rewards = Coin::new(0_u128, strategy_denom);
        Ok(state)
    })?;

    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: _env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::RedeemRewards {}).unwrap(),
            funds: vec![],
        })
        .add_message(WasmMsg::Execute {
            contract_addr: _env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::Swap {}).unwrap(),
            funds: vec![],
        })
        .add_messages(messages))
}

pub fn try_redeem_rewards(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;

    let mut total_rewards: Vec<Coin> = vec![];
    let mut messages: Vec<DistributionMsg> = vec![];

    for validator in &state.validator_pool {
        let result = deps
            .querier
            .query_delegation(&_env.contract.address, validator)?;
        if let Some(full_delegation) = result {
            total_rewards = merge_coin_vector(
                full_delegation.accumulated_rewards,
                CoinVecOp {
                    fund: total_rewards,
                    operation: Operation::Add,
                },
            );
        } else {
            continue;
        }

        messages.push(DistributionMsg::WithdrawDelegatorReward {
            validator: validator.to_string(),
        });
    }

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.unswapped_rewards = merge_coin_vector(
            state.unswapped_rewards,
            CoinVecOp {
                fund: total_rewards,
                operation: Operation::Add,
            },
        );

        Ok(state)
    })?;

    Ok(Response::new().add_messages(messages))
}

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
    let state = STATE.load(deps.storage).unwrap();
    Ok(GetTotalTokensResponse {
        total_tokens: Option::from(state.total_staked_tokens),
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
