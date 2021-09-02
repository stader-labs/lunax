#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Attribute, BankMsg, Binary, Coin, Decimal, Deps, DepsMut,
    DistributionMsg, Env, Fraction, MessageInfo, Order, Response, StakingMsg, StdResult, Uint128,
    WasmMsg,
};

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetCurrentUndelegationBatchIdResponse, GetStateResponse, GetTotalTokensResponse,
    GetUndelegationBatchInfoResponse, InstantiateMsg, QueryMsg,
};
use crate::state::{
    BatchUndelegationRecord, StakeQuota, State, STATE, UNDELEGATION_INFO_LEDGER,
    VALIDATORS_TO_STAKED_QUOTA,
};
use cw20::Cw20ExecuteMsg;
use cw_storage_plus::U64Key;
use stader_utils::coin_utils::{
    merge_coin, merge_coin_vector, multiply_coin_with_decimal, CoinOp, CoinVecOp, Operation,
};
use stader_utils::helpers::send_funds_msg;
use std::collections::HashMap;
use std::ops::Add;
use terra_cosmwasm::{create_swap_msg, SwapResponse, TerraMsgWrapper, TerraQuerier};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let mut state = State {
        manager: info.sender.clone(),
        scc_address: msg.scc_address,
        vault_denom: msg.vault_denom.clone(),
        contract_genesis_block_height: _env.block.height,
        contract_genesis_timestamp: _env.block.time,
        unbonding_period: msg
            .unbonding_period
            .unwrap_or_else(|| (21 * 24 * 3600 + 3600)),
        current_undelegation_batch_id: 0,
        current_undelegation_funds: Uint128::zero(),
        accumulated_airdrops: vec![],
        validator_pool: msg.initial_validators,
        unswapped_rewards: vec![],
        uninvested_rewards: Coin::new(0_u128, msg.vault_denom.clone()),

        total_staked_tokens: Uint128::zero(),
        total_slashed_amount: Uint128::zero(),
    };

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
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
        ExecuteMsg::WithdrawRewards {
            user,
            undelegation_batch_id,
            amount,
        } => try_withdraw_rewards(deps, _env, info, user, undelegation_batch_id, amount),
        ExecuteMsg::ReconcileUndelegationBatch {
            undelegation_batch_id,
        } => try_reconcile_undelegation_batch(deps, _env, info, undelegation_batch_id),
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
    }
}

pub fn try_claim_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    airdrop_token_contract: Addr,
    cw20_token_contract: Addr,
    airdrop_token: String,
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

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.accumulated_airdrops = merge_coin_vector(
            state.accumulated_airdrops,
            CoinVecOp {
                fund: vec![Coin::new(amount.u128(), airdrop_token)],
                operation: Operation::Add,
            },
        );
        Ok(state)
    })?;

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

pub fn try_reconcile_undelegation_batch(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    undelegation_batch_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage).unwrap();

    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    let undelegation_batch_option = UNDELEGATION_INFO_LEDGER
        .may_load(deps.storage, undelegation_batch_id.into())
        .unwrap();
    if undelegation_batch_option.is_none() {
        return Err(ContractError::NonExistentUndelegationBatch {});
    }

    let vault_denom = state.vault_denom;
    let mut undelegation_batch = undelegation_batch_option.unwrap();

    if _env.block.time.lt(&undelegation_batch.est_release_time) {
        return Err(ContractError::UndelegationBatchInUnbondingPeriod(
            undelegation_batch_id,
        ));
    }

    let total_base_funds_in_strategy = deps
        .querier
        .query_balance(_env.contract.address, vault_denom.clone())
        .unwrap()
        .amount;
    let current_uninvested_rewards = state.uninvested_rewards.amount;
    let base_funds_from_rewards = state
        .unswapped_rewards
        .iter()
        .find(|&x| x.denom.eq(&vault_denom))
        .cloned()
        .unwrap_or_else(|| Coin::new(0, vault_denom.clone()))
        .amount;
    let current_undelegation_funds = state.current_undelegation_funds;

    let unaccounted_base_funds = total_base_funds_in_strategy
        .checked_sub(current_uninvested_rewards)
        .unwrap()
        .checked_sub(base_funds_from_rewards)
        .unwrap()
        .checked_sub(current_undelegation_funds)
        .unwrap();

    let mut total_slashed_amount = Uint128::zero();
    let unbonding_slashing_ratio = if unaccounted_base_funds.lt(&undelegation_batch.amount.amount) {
        // Slashing has occured for unbonding funds from one of the validators.
        total_slashed_amount = undelegation_batch
            .amount
            .amount
            .checked_sub(unaccounted_base_funds)
            .unwrap();
        Decimal::from_ratio(unaccounted_base_funds, undelegation_batch.amount.amount)
    } else {
        Decimal::one()
    };

    UNDELEGATION_INFO_LEDGER.update(
        deps.storage,
        undelegation_batch_id.into(),
        |_x| -> StdResult<_> {
            undelegation_batch.unbonding_slashing_ratio = unbonding_slashing_ratio;
            undelegation_batch.slashing_checked = true;
            Ok(undelegation_batch)
        },
    );

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.current_undelegation_funds = state
            .current_undelegation_funds
            .checked_add(unaccounted_base_funds)
            .unwrap();
        state.total_slashed_amount = state
            .total_slashed_amount
            .checked_add(total_slashed_amount)
            .unwrap();
        Ok(state)
    })?;

    Ok(Response::default())
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
        return Err(ContractError::NoUnswappedRewards {});
    }

    // fetch the swapped money
    let vault_denom = state.vault_denom;
    let mut logs: Vec<Attribute> = vec![];
    let mut swapped_coin: Coin = Coin::new(0_u128, vault_denom.clone());
    let terra_querier = TerraQuerier::new(&deps.querier);
    let mut failed_coins: Vec<Coin> = vec![];
    let mut messages = vec![];
    for reward_coin in state.unswapped_rewards {
        let mut swapped_out_coin = reward_coin.clone();

        if swapped_out_coin.denom.eq(&vault_denom) {
            continue;
        }

        let coin_swap_wrapped = terra_querier.query_swap(reward_coin.clone(), vault_denom.clone());
        // TODO: bchain99 - I think this could mean that there is no swap possible for the pair.
        if coin_swap_wrapped.is_err() {
            // TODO: bchain99 - Check if this is needed. Check the cases when the query_swap can fail.
            logs.push(attr("failed_to_swap", reward_coin.to_string()));
            failed_coins.push(reward_coin);
            continue;
        }

        messages.push(create_swap_msg(reward_coin, vault_denom.clone()));

        let coin_swap: SwapResponse = coin_swap_wrapped.unwrap();
        swapped_out_coin = coin_swap.receive;

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
    });

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
        return Ok(Response::default());
    }

    // accept only one coin
    if info.funds.len() > 1 {
        return Err(ContractError::MultipleCoins {});
    }

    let transferred_coin = info.funds[0].clone();
    if transferred_coin.denom.ne(&state.vault_denom) {
        return Err(ContractError::WrongDenom(transferred_coin.denom));
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
    });

    Ok(Response::default())
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
        return Err(ContractError::ZeroUndelegation {});
    }

    // For each undelegation request to SIC, we create an undelegation batch. The main intent of the batch
    // is to account for undelegation slashing.
    let mut new_undelegation_batch_id = state.current_undelegation_batch_id.add(1_u64);
    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.current_undelegation_batch_id = new_undelegation_batch_id;
        Ok(state)
    })?;

    let u64key = new_undelegation_batch_id.into();
    UNDELEGATION_INFO_LEDGER.save(
        deps.storage,
        u64key,
        &BatchUndelegationRecord {
            amount: Coin::new(amount.u128(), state.vault_denom.clone()),
            unbonding_slashing_ratio: Decimal::one(),
            create_time: _env.block.time,
            est_release_time: _env.block.time.plus_seconds(state.unbonding_period),
            slashing_checked: false,
        },
    )?;

    let new_total_staked_tokens = state.total_staked_tokens.checked_sub(amount).unwrap();
    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.total_staked_tokens = new_total_staked_tokens;
        Ok(state)
    })?;

    // undelegate from each validator according to their staked fraction
    let mut messages: Vec<StakingMsg> = vec![];
    let vault_denom = state.vault_denom;
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
        let stake_fraction = validator_staked_quota.vault_stake_fraction;

        let mut unstake_amount = Uint128::zero();
        if !stake_fraction.is_zero() {
            unstake_amount = Uint128::new(
                amount.u128() * stake_fraction.numerator() / stake_fraction.denominator(),
            );

            messages.push(StakingMsg::Undelegate {
                validator: String::from(validator),
                amount: Coin {
                    denom: vault_denom.clone(),
                    amount: unstake_amount,
                },
            });
        }

        let new_validator_staked_amount = total_delegated_amount
            .checked_sub(unstake_amount)
            .unwrap()
            .u128();
        VALIDATORS_TO_STAKED_QUOTA.save(
            deps.storage,
            validator,
            &StakeQuota {
                amount: Coin::new(new_validator_staked_amount, vault_denom.clone()),
                vault_stake_fraction: Decimal::from_ratio(
                    new_validator_staked_amount,
                    new_total_staked_tokens,
                ),
            },
        )?;
    }

    Ok(Response::new().add_messages(messages))
}

pub fn try_withdraw_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    user: Addr,
    undelegation_batch_id: u64,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.scc_address {
        return Err(ContractError::Unauthorized {});
    }

    if amount.is_zero() {
        return Err(ContractError::ZeroWithdrawal {});
    }

    let mut undelegation_batch: BatchUndelegationRecord;
    match UNDELEGATION_INFO_LEDGER
        .may_load(deps.storage, U64Key::new(undelegation_batch_id))
        .unwrap()
    {
        None => return Err(ContractError::NonExistentUndelegationBatch {}),
        Some(undelegation_batch_unwrapped) => {
            undelegation_batch = undelegation_batch_unwrapped;
        }
    }

    if !undelegation_batch.slashing_checked {
        return Err(ContractError::SlashingNotChecked(undelegation_batch_id));
    }

    if _env.block.time.lt(&undelegation_batch.est_release_time) {
        return Err(ContractError::DepositInUnbondingPeriod {});
    }

    if amount.gt(&undelegation_batch.amount.amount) {
        return Err(ContractError::InsufficientFundsInUndelegationBatch(
            undelegation_batch_id,
        ));
    }

    let effective_withdrawable_amount = multiply_coin_with_decimal(
        &Coin::new(amount.u128(), state.vault_denom),
        undelegation_batch.unbonding_slashing_ratio,
    );

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.current_undelegation_funds = state
            .current_undelegation_funds
            .checked_sub(effective_withdrawable_amount.amount)
            .unwrap();
        Ok(state)
    })?;

    Ok(Response::new().add_message(send_funds_msg(&user, &vec![effective_withdrawable_amount])))
}

pub fn try_reinvest(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    if state.uninvested_rewards.amount.is_zero() {
        return Err(ContractError::NoUninvestedRewards {});
    }

    let vault_denom = state.vault_denom;
    let mut current_total_staked_tokens = Coin::new(0_u128, vault_denom.clone());
    let mut validator_to_delegation_map: HashMap<&Addr, Uint128> = HashMap::new();
    for validator in &state.validator_pool {
        let result = deps
            .querier
            .query_delegation(&_env.contract.address, validator)?;
        // TODO: bchain99 - should not happen
        if result.is_none() {
            continue;
        }

        let full_delegation = result.unwrap();

        validator_to_delegation_map.insert(validator, full_delegation.amount.amount);

        current_total_staked_tokens = merge_coin(
            current_total_staked_tokens,
            CoinOp {
                fund: full_delegation.amount,
                operation: Operation::Add,
            },
        );
    }

    let total_slashed_amount = state
        .total_staked_tokens
        .checked_sub(current_total_staked_tokens.amount)
        .unwrap();

    let rewards_to_invest = state.uninvested_rewards.amount;

    let new_current_staked_tokens = current_total_staked_tokens
        .amount
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
                    denom: vault_denom.clone(),
                    amount: delegation_amount,
                },
            });
        }

        let current_validator_staked_amount = *(validator_to_delegation_map.get(v).unwrap());
        let new_validator_staked_amount = current_validator_staked_amount
            .checked_add(delegation_amount)
            .unwrap();
        // validator stake quota will get updated as we are reconciling the validator stake
        let new_validator_stake_quota: StakeQuota = StakeQuota {
            amount: Coin {
                denom: vault_denom.clone(),
                amount: new_validator_staked_amount,
            },
            vault_stake_fraction: Decimal::from_ratio(
                new_validator_staked_amount,
                new_current_staked_tokens,
            ),
        };

        VALIDATORS_TO_STAKED_QUOTA.save(deps.storage, v, &new_validator_stake_quota);
        extra_split = 0_u128;
    });

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.total_staked_tokens = new_current_staked_tokens;
        if total_slashed_amount > Uint128::zero() {
            state.total_slashed_amount = state
                .total_slashed_amount
                .checked_add(total_slashed_amount)
                .unwrap();
        }
        state.uninvested_rewards = Coin::new(0_u128, vault_denom);
        Ok(state)
    })?;

    Ok(Response::new().add_messages(messages))
}

pub fn try_redeem_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;

    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    let mut total_rewards: Vec<Coin> = vec![];
    let mut messages: Vec<DistributionMsg> = vec![];

    for validator in &state.validator_pool {
        let result = deps
            .querier
            .query_delegation(&_env.contract.address, validator)?;
        if result.is_none() {
            continue;
        } else {
            let full_delegation = result.unwrap();
            total_rewards = merge_coin_vector(
                full_delegation.accumulated_rewards,
                CoinVecOp {
                    fund: total_rewards,
                    operation: Operation::Add,
                },
            );
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
    });

    Ok(Response::new().add_messages(messages))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetTotalTokens {} => to_binary(&query_total_tokens(deps, _env)?),
        QueryMsg::GetCurrentUndelegationBatchId {} => {
            to_binary(&query_current_undelegation_batch_id(deps, _env)?)
        }
        QueryMsg::GetState {} => to_binary(&query_state(deps)?),
        QueryMsg::GetUndelegationBatchInfo {
            undelegation_batch_id,
        } => to_binary(&query_undelegation_batch_info(deps, undelegation_batch_id)?),
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

fn query_current_undelegation_batch_id(
    deps: Deps,
    _env: Env,
) -> StdResult<GetCurrentUndelegationBatchIdResponse> {
    let state = STATE.load(deps.storage).unwrap();

    Ok(GetCurrentUndelegationBatchIdResponse {
        current_undelegation_batch_id: state.current_undelegation_batch_id,
    })
}

fn query_undelegation_batch_info(
    deps: Deps,
    undelegation_batch_id: u64,
) -> StdResult<GetUndelegationBatchInfoResponse> {
    let undelegation_batch_info = UNDELEGATION_INFO_LEDGER
        .may_load(deps.storage, U64Key::new(undelegation_batch_id))
        .unwrap();

    Ok(GetUndelegationBatchInfoResponse {
        undelegation_batch_info,
    })
}
