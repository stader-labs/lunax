#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Addr, Binary, Coin, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128, attr, Attribute, DistributionMsg};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, GetTotalTokensResponse, GetCurrentUndelegationBatchIdResponse};
use crate::state::{Config, State, CONFIG, STATE};
use cw_storage_plus::U64Key;
use crate::utils::{merge_coin_vector, CoinVecOp, Operation, merge_coin, CoinOp};
use terra_cosmwasm::{TerraQuerier, create_swap_msg, SwapResponse, TerraMsgWrapper};

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
        vault_apr: Decimal::zero(),
        unbonding_period: (21 * 24 * 3600 + 3600),
        total_slashed_deposit: Uint128::zero(),
        accumulated_vault_airdrops: vec![],
        validator_pool: msg.initial_validators,
        unswapped_rewards: vec![],
        uninvested_rewards: Coin::new(0_u128, msg.vault_denom.clone()),
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
        ExecuteMsg::Reinvest {} => try_reinvest(deps, _env, info),
        ExecuteMsg::RedeemRewards {} => try_redeem_rewards(deps, _env, info),
        ExecuteMsg::Swap {} => try_swap(deps, _env, info),
    }
}

pub fn try_swap(deps: DepsMut, _env: Env, info: MessageInfo) -> Result<Response<TerraMsgWrapper>, ContractError> {
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
    Ok(Response::default())
}

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

    Ok(Response::new().add_messages(messages))}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetTotalTokens {} => to_binary(&query_total_tokens(deps, _env)?),
        QueryMsg::GetCurrentUndelegationBatchId {} => to_binary(&query_current_undelegation_batch_id(deps, _env)?),
    }
}

fn query_total_tokens(deps: Deps, _env: Env) -> StdResult<GetTotalTokensResponse> {
    Ok(GetTotalTokensResponse {
        total_tokens: None
    })
}

fn query_current_undelegation_batch_id(deps: Deps, _env: Env) -> StdResult<GetCurrentUndelegationBatchIdResponse> {
    Ok(GetCurrentUndelegationBatchIdResponse {
        current_undelegation_batch_id: 0
    })
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
//     use cosmwasm_std::{coins, from_binary};
//
//     #[test]
//     fn proper_initialization() {
//         let mut deps = mock_dependencies(&[]);
//
//         let msg = InstantiateMsg { count: 17 };
//         let info = mock_info("creator", &coins(1000, "earth"));
//
//         // we can just call .unwrap() to assert this was a success
//         let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
//         assert_eq!(0, res.messages.len());
//
//         // it worked, let's query the state
//         let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
//         let value: CountResponse = from_binary(&res).unwrap();
//         assert_eq!(17, value.count);
//     }
//
//     #[test]
//     fn increment() {
//         let mut deps = mock_dependencies(&coins(2, "token"));
//
//         let msg = InstantiateMsg { count: 17 };
//         let info = mock_info("creator", &coins(2, "token"));
//         let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
//
//         // beneficiary can release it
//         let info = mock_info("anyone", &coins(2, "token"));
//         let msg = ExecuteMsg::Increment {};
//         let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
//
//         // should increase counter by 1
//         let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
//         let value: CountResponse = from_binary(&res).unwrap();
//         assert_eq!(18, value.count);
//     }
//
//     #[test]
//     fn reset() {
//         let mut deps = mock_dependencies(&coins(2, "token"));
//
//         let msg = InstantiateMsg { count: 17 };
//         let info = mock_info("creator", &coins(2, "token"));
//         let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
//
//         // beneficiary can release it
//         let unauth_info = mock_info("anyone", &coins(2, "token"));
//         let msg = ExecuteMsg::Reset { count: 5 };
//         let res = execute(deps.as_mut(), mock_env(), unauth_info, msg);
//         match res {
//             Err(ContractError::Unauthorized {}) => {}
//             _ => panic!("Must return unauthorized error"),
//         }
//
//         // only the original creator can reset the counter
//         let auth_info = mock_info("creator", &coins(2, "token"));
//         let msg = ExecuteMsg::Reset { count: 5 };
//         let _res = execute(deps.as_mut(), mock_env(), auth_info, msg).unwrap();
//
//         // should now be 5
//         let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
//         let value: CountResponse = from_binary(&res).unwrap();
//         assert_eq!(5, value.count);
//     }
// }
