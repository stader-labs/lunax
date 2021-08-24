#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Attribute, BankMsg, Binary, Coin, Decimal, Deps, DepsMut,
    DistributionMsg, Env, Fraction, MessageInfo, Order, Response, StakingMsg, StdResult, Uint128,
};

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetConfigResponse, GetCurrentUndelegationBatchIdResponse, GetStateResponse,
    GetTotalTokensResponse, GetUndelegationBatchInfoResponse, InstantiateMsg, QueryMsg,
};
use crate::state::{
    BatchUndelegationRecord, Config, StakeQuota, State, CONFIG, STATE, UNDELEGATION_INFO_LEDGER,
    VALIDATORS_TO_STAKED_QUOTA,
};
use crate::utils::{
    decimal_multiplication_in_256, merge_coin, merge_coin_vector, multiply_coin_with_decimal,
    CoinOp, CoinVecOp, Operation,
};
use cw_storage_plus::U64Key;
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
        contract_genesis_block_height: _env.block.height,
        contract_genesis_timestamp: _env.block.time,
        contract_genesis_shares_per_token_ratio: Decimal::from_ratio(100_000_000_u128, 1_u128),
        unbonding_period: (21 * 24 * 3600 + 3600),
        current_undelegation_batch_id: 0,
        accumulated_vault_airdrops: vec![],
        validator_pool: msg.initial_validators,
        unswapped_rewards: vec![],
        uninvested_rewards: Coin::new(0_u128, msg.vault_denom.clone()),

        total_staked_tokens: Uint128::zero(),
        total_slashed_amount: Uint128::zero()
    };
    if msg.unbonding_period.is_some() {
        state.unbonding_period = msg.unbonding_period.unwrap();
    }

    let config = Config {
        manager: info.sender.clone(),
        scc_contract_address: msg.scc_contract_address,
        vault_denom: msg.vault_denom,
    };

    STATE.save(deps.storage, &state)?;
    CONFIG.save(deps.storage, &config)?;

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
        ExecuteMsg::CompensateSlashing {} => try_compensate_slashing(deps, _env, info),
    }
}

pub fn try_compensate_slashing(deps: DepsMut, _env: Env, info: MessageInfo) -> Result<Response<TerraMsgWrapper>, ContractError> {
    // TODO: bchain99 - Go to the slashing contract and compensate for state.total_slashed_amount
    Ok(Response::default())
}

pub fn try_reconcile_undelegation_batch(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    undelegation_batch_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    Ok(Response::default())
}

pub fn try_swap(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    if state.unswapped_rewards.is_empty() {
        return Err(ContractError::NoUnstakedRewards {});
    }

    // fetch the swapped money
    let vault_denom = config.vault_denom;
    let mut logs: Vec<Attribute> = vec![];
    let mut swapped_coin: Coin = Coin::new(0_u128, vault_denom.clone());
    let terra_querier = TerraQuerier::new(&deps.querier);
    let mut failed_coins: Vec<Coin> = vec![];
    let mut messages = vec![];
    for reward_coin in state.unswapped_rewards {
        let mut swapped_out_coin = reward_coin.clone();

        if swapped_out_coin.denom.ne(&vault_denom) {
            let coin_swap_wrapped =
                terra_querier.query_swap(reward_coin.clone(), vault_denom.clone());
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

    let config = CONFIG.load(deps.storage).unwrap();
    if info.sender != config.scc_contract_address {
        return Err(ContractError::Unauthorized {});
    }

    // check if any money is being sent
    if info.funds.is_empty() {
        return Err(ContractError::NoFundsSent {});
    }

    // accept only one coin
    if info.funds.len() > 1 {
        return Err(ContractError::MultipleCoins {});
    }

    let transferred_coin = info.funds[0].clone();

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
pub fn try_undelegate_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    Ok(Response::default())
}

pub fn try_withdraw_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    user: Addr,
    undelegation_batch_id: u64,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    Ok(Response::default())
}

pub fn try_reinvest(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;
    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    if state.uninvested_rewards.amount.is_zero() {
        return Err(ContractError::NoUnstakedRewards {});
    }

    let vault_denom = config.vault_denom;
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

    let total_slashed_amount = state.total_staked_tokens.checked_sub(current_total_staked_tokens.amount).unwrap();

    let new_current_staked_tokens = current_total_staked_tokens
        .amount
        .checked_add(state.uninvested_rewards.amount)
        .unwrap();

    let validator_pool_length = state.validator_pool.len();
    let even_split = new_current_staked_tokens.u128() / validator_pool_length as u128;
    let mut extra_split = new_current_staked_tokens.u128() % validator_pool_length as u128;
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
            state.total_slashed_amount = state.total_slashed_amount.checked_add(total_slashed_amount).unwrap();
        }
        Ok(state)
    })?;

    Ok(Response::default())
}

pub fn try_redeem_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.manager {
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
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::GetUndelegationBatchInfo {
            undelegation_batch_id,
        } => to_binary(&query_undelegation_batch_info(deps, undelegation_batch_id)?),
    }
}

fn query_state(deps: Deps) -> StdResult<GetStateResponse> {
    let state = STATE.may_load(deps.storage).unwrap();

    Ok(GetStateResponse { state })
}

fn query_config(deps: Deps) -> StdResult<GetConfigResponse> {
    let config = CONFIG.may_load(deps.storage).unwrap();

    Ok(GetConfigResponse { config })
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

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR};
    use cosmwasm_std::{coins, from_binary, FullDelegation, SubMsg, Validator};

    fn get_validators() -> Vec<Validator> {
        vec![
            Validator {
                address: "valid0001".to_string(),
                commission: Decimal::zero(),
                max_commission: Decimal::zero(),
                max_change_rate: Decimal::zero(),
            },
            Validator {
                address: "valid0002".to_string(),
                commission: Decimal::zero(),
                max_commission: Decimal::zero(),
                max_change_rate: Decimal::zero(),
            },
        ]
    }

    fn get_delegations() -> Vec<FullDelegation> {
        vec![
            FullDelegation {
                delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                validator: "valid0001".to_string(),
                amount: Coin::new(2000, "uluna".to_string()),
                can_redelegate: Coin::new(1000, "uluna".to_string()),
                accumulated_rewards: vec![
                    Coin::new(20, "uluna".to_string()),
                    Coin::new(30, "urew1"),
                ],
            },
            FullDelegation {
                delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                validator: "valid0002".to_string(),
                amount: Coin::new(2000, "uluna".to_string()),
                can_redelegate: Coin::new(0, "uluna".to_string()),
                accumulated_rewards: vec![
                    Coin::new(40, "uluna".to_string()),
                    Coin::new(60, "urew1"),
                ],
            },
        ]
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            scc_contract_address: Addr::unchecked("scc-contract-address"),
            vault_denom: "uluna".to_string(),
            initial_validators: vec![],
            unbonding_period: None,
        };
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(17, value.count);
    }

    #[test]
    fn test__try_redeem_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::RedeemRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test__try_redeem_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemRewards {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages,
            vec![
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0001".to_string(),
                }),
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0002".to_string(),
                })
            ]
        );
        let mut state = STATE.load(deps.as_mut().storage).unwrap();
        assert!(check_equal_vec(
            state.unswapped_rewards,
            vec![Coin::new(90, "urew1"), Coin::new(60, "uluna")]
        ));
    }
}
