#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Attribute, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    QuerierResult, Response, StdError, StdResult, Timestamp, Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::helpers::{get_strategy_shares_per_token_ratio, get_strategy_split};
use crate::msg::{
    ExecuteMsg, GetStateResponse, GetStrategyInfoResponse, GetUserRewardInfo, InstantiateMsg,
    QueryMsg, UpdateUserAirdropsRequest, UpdateUserRewardsRequest,
};
use crate::state::{
    State, StrategyInfo, UserRewardInfo, UserStrategyInfo, UserStrategyPortfolio, STATE,
    STRATEGY_MAP, USER_REWARD_INFO_MAP,
};
use crate::user::get_user_airdrops;
use schemars::_serde_json::ser::CharEscape::ReverseSolidus;
use sic_base::msg::{ExecuteMsg as sic_execute_msg, QueryMsg as sic_query_msg};
use stader_utils::coin_utils::{
    decimal_division_in_256, decimal_multiplication_in_256, decimal_summation_in_256,
    get_decimal_from_uint128, merge_coin_vector, CoinVecOp, Operation,
};
use stader_utils::helpers::send_funds_msg;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::mem::forget;
use std::ops::Deref;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        manager: info.sender.clone(),
        pool_contract: msg.pools_contract,
        scc_denom: msg.strategy_denom,
        contract_genesis_block_height: _env.block.height,
        contract_genesis_timestamp: _env.block.time,
        total_accumulated_rewards: Uint128::zero(),
        current_rewards_in_scc: Uint128::zero(),
        total_accumulated_airdrops: vec![],
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
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterStrategy {
            strategy_name,
            unbonding_period,
            sic_contract_address,
        } => try_register_strategy(
            deps,
            _env,
            info,
            strategy_name,
            unbonding_period,
            sic_contract_address,
        ),
        ExecuteMsg::DeactivateStrategy { strategy_name } => {
            try_deactivate_strategy(deps, _env, info, strategy_name)
        }
        ExecuteMsg::ActivateStrategy { strategy_name } => {
            try_activate_strategy(deps, _env, info, strategy_name)
        }
        ExecuteMsg::RemoveStrategy { strategy_name } => {
            try_remove_strategy(deps, _env, info, strategy_name)
        }
        ExecuteMsg::UpdateUserPortfolio {
            strategy_name,
            deposit_fraction,
        } => try_update_user_portfolio(deps, _env, info, strategy_name, deposit_fraction),
        ExecuteMsg::UpdateUserRewards {
            update_user_rewards_requests,
        } => try_update_user_rewards(deps, _env, info, update_user_rewards_requests),
        ExecuteMsg::UpdateUserAirdrops {
            update_user_airdrops_requests,
        } => try_update_user_airdrops(deps, _env, info, update_user_airdrops_requests),
        ExecuteMsg::UndelegateRewards {
            amount,
            strategy_name,
        } => try_undelegate_rewards(deps, _env, info, amount, strategy_name),
        ExecuteMsg::ClaimAirdrops { strategy_name } => {
            try_claim_airdrops(deps, _env, info, strategy_name)
        }
        ExecuteMsg::WithdrawRewards {
            undelegation_timestamp,
            strategy_name,
        } => try_withdraw_rewards(deps, _env, info, undelegation_timestamp, strategy_name),
        ExecuteMsg::WithdrawAirdrops {} => try_withdraw_airdrops(deps, _env, info),
        ExecuteMsg::WithdrawPendingRewards {} => try_withdraw_pending_rewards(deps, _env, info),
    }
}

pub fn try_withdraw_pending_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    let user_addr = info.sender;

    let user_reward_info: UserRewardInfo;
    if let Some(user_reward_info_map) = USER_REWARD_INFO_MAP
        .may_load(deps.storage, &user_addr)
        .unwrap()
    {
        user_reward_info = user_reward_info_map;
    } else {
        return Err(ContractError::UserRewardInfoDoesNotExist {});
    }

    let user_pending_rewards = user_reward_info.pending_rewards;
    if user_pending_rewards.is_zero() {
        return Ok(Response::new().add_attribute("zero_pending_rewards", "1"));
    }

    Ok(Response::new().add_message(send_funds_msg(
        &user_addr,
        &vec![Coin::new(user_pending_rewards.u128(), state.scc_denom)],
    )))
}

pub fn try_undelegate_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
    strategy_name: String,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

pub fn try_claim_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_name: String,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

pub fn try_withdraw_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    undelegation_timestamp: Timestamp,
    strategy_name: String,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

pub fn try_withdraw_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

pub fn try_register_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_name: String,
    unbonding_period: Option<u64>,
    sic_contract_address: Addr,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    if STRATEGY_MAP
        .may_load(deps.storage, &*strategy_name)
        .unwrap()
        .is_some()
    {
        return Err(ContractError::StrategyInfoAlreadyExists {});
    }

    STRATEGY_MAP.save(
        deps.storage,
        &*strategy_name.clone(),
        &StrategyInfo::new(strategy_name, sic_contract_address, unbonding_period),
    )?;

    Ok(Response::default())
}

pub fn try_deactivate_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_name: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    STRATEGY_MAP.update(
        deps.storage,
        &*strategy_name.clone(),
        |strategy_info_opt| -> Result<_, ContractError> {
            if strategy_info_opt.is_none() {
                return Err(ContractError::StrategyInfoDoesNotExist(strategy_name));
            }

            let mut strategy_info = strategy_info_opt.unwrap();
            strategy_info.is_active = false;
            Ok(strategy_info)
        },
    )?;

    Ok(Response::default())
}

pub fn try_activate_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_name: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    STRATEGY_MAP.update(
        deps.storage,
        &*strategy_name.clone(),
        |strategy_info_option| -> Result<_, ContractError> {
            if strategy_info_option.is_none() {
                return Err(ContractError::StrategyInfoDoesNotExist(strategy_name));
            }

            let mut strategy_info = strategy_info_option.unwrap();
            strategy_info.is_active = true;
            Ok(strategy_info)
        },
    )?;

    Ok(Response::default())
}

pub fn try_remove_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_name: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    STRATEGY_MAP.remove(deps.storage, &*strategy_name);

    Ok(Response::default())
}

pub fn try_update_user_portfolio(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_name: String,
    deposit_fraction: Decimal,
) -> Result<Response, ContractError> {
    let user_addr = info.sender;

    if let None = STRATEGY_MAP
        .may_load(deps.storage, &*strategy_name)
        .unwrap()
    {
        return Err(ContractError::StrategyInfoDoesNotExist(
            strategy_name.clone(),
        ));
    }

    let mut user_reward_info: UserRewardInfo = UserRewardInfo::new();
    if let Some(user_reward_info_map) = USER_REWARD_INFO_MAP
        .may_load(deps.storage, &user_addr)
        .unwrap()
    {
        user_reward_info = user_reward_info_map;
    }

    // find existing portfolio and get a mutable reference to it
    let mut user_portfolio: &mut UserStrategyPortfolio;
    if let Some(i) = (0..user_reward_info.user_portfolio.len()).find(|&i| {
        user_reward_info.user_portfolio[i]
            .strategy_name
            .eq(&strategy_name)
    }) {
        user_portfolio = &mut user_reward_info.user_portfolio[i];
    } else {
        let new_user_portfolio = UserStrategyPortfolio::new(strategy_name, deposit_fraction);
        user_reward_info.user_portfolio.push(new_user_portfolio);
        user_portfolio = user_reward_info.user_portfolio.last_mut().unwrap();
    }

    // redundant if the portfolio was newly created
    user_portfolio.deposit_fraction = deposit_fraction;

    // check if the entire portfolio deposit fraction is less than 1. else abort the tx
    let mut total_deposit_fraction = Decimal::zero();
    user_reward_info.user_portfolio.iter().for_each(|x| {
        total_deposit_fraction =
            decimal_summation_in_256(total_deposit_fraction, x.deposit_fraction);
    });

    if total_deposit_fraction > Decimal::one() {
        return Err(ContractError::InvalidPortfolioDepositFraction {});
    }

    USER_REWARD_INFO_MAP.save(deps.storage, &user_addr, &user_reward_info)?;

    Ok(Response::default())
}

pub fn try_update_user_rewards(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    update_user_rewards_requests: Vec<UpdateUserRewardsRequest>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();

    if update_user_rewards_requests.is_empty() {
        return Ok(Response::new().add_attribute("zero_update_user_rewards_requests", "1"));
    }

    let mut failed_strategies: Vec<String> = vec![];
    let mut inactive_strategies: Vec<String> = vec![];
    let mut users_with_zero_deposits: Vec<String> = vec![];
    // cache for the S/T ratio per strategy. We fetch it once and cache it up.
    let mut strategy_to_s_t_ratio: HashMap<String, Decimal> = HashMap::new();
    let mut strategy_to_funds: HashMap<Addr, Uint128> = HashMap::new();
    let mut total_rewards: Uint128 = Uint128::zero();
    let mut messages: Vec<WasmMsg> = vec![];
    for update_user_rewards_request in update_user_rewards_requests {
        let user_addr = update_user_rewards_request.user;
        let funds = update_user_rewards_request.funds;
        let strategy_opt = update_user_rewards_request.strategy_name;

        if funds.is_zero() {
            users_with_zero_deposits.push(user_addr.to_string());
            continue;
        }

        total_rewards = total_rewards.checked_add(funds).unwrap();

        let mut user_reward_info = USER_REWARD_INFO_MAP
            .may_load(deps.storage, &user_addr)
            .unwrap()
            .unwrap_or(UserRewardInfo::new());

        // this is the amount to be split per strategy
        let mut strategy_split: Vec<(String, Uint128)> = vec![];
        // surplus is the amount of rewards which does not go into any strategy and just sits in SCC.
        let mut surplus: Uint128 = Uint128::zero();

        if strategy_opt.is_none() {
            let strategy_split_with_surplus = get_strategy_split(&user_reward_info, funds);
            strategy_split = strategy_split_with_surplus.0;
            surplus = strategy_split_with_surplus.1;
        } else {
            let strategy_name = strategy_opt.unwrap();
            strategy_split = vec![(strategy_name, funds)];
        }

        // add the surplus to the pending rewards
        user_reward_info.pending_rewards = user_reward_info
            .pending_rewards
            .checked_add(surplus)
            .unwrap();

        for split in strategy_split {
            let strategy_name = split.0;
            let amount = split.1;

            if amount.is_zero() {
                continue;
            }

            let mut strategy_info: StrategyInfo;
            if let Some(strategy_info_mapping) = STRATEGY_MAP
                .may_load(deps.storage, &*strategy_name)
                .unwrap()
            {
                strategy_info = strategy_info_mapping;
            } else {
                // TODO: bchain99 - will cause duplicates
                failed_strategies.push(strategy_name);
                continue;
            }

            if !strategy_info.is_active {
                inactive_strategies.push(strategy_name);
                continue;
            }

            // fetch the total tokens from the SIC contract and update the S/T ratio for the strategy
            // compute the S/T ratio only once as we are batching up reward transfer messages. Jus' cache
            // it up in a map like we always do.
            let current_strategy_shares_per_token_ratio: Decimal;
            if !strategy_to_s_t_ratio.contains_key(&strategy_name) {
                current_strategy_shares_per_token_ratio =
                    get_strategy_shares_per_token_ratio(deps.querier, &strategy_info);
                strategy_info.shares_per_token_ratio = current_strategy_shares_per_token_ratio;
                strategy_to_s_t_ratio.insert(
                    strategy_name.clone(),
                    current_strategy_shares_per_token_ratio,
                );
            } else {
                current_strategy_shares_per_token_ratio =
                    *strategy_to_s_t_ratio.get(&strategy_name).unwrap();
            }

            let mut user_strategy_info: &mut UserStrategyInfo;
            if let Some(i) = (0..user_reward_info.strategies.len()).find(|&i| {
                user_reward_info.strategies[i].strategy_name == strategy_info.name.clone()
            }) {
                user_strategy_info = &mut user_reward_info.strategies[i];
            } else {
                let new_user_strategy_info: UserStrategyInfo =
                    UserStrategyInfo::new(strategy_info.name.clone(), vec![]);
                user_reward_info.strategies.push(new_user_strategy_info);
                user_strategy_info = user_reward_info.strategies.last_mut().unwrap();
            }

            // update the user airdrop pointer and allocate the user pending airdrops for the strategy
            let mut user_airdrops: Vec<Coin> = vec![];
            if let Some(new_user_airdrops) = get_user_airdrops(
                &strategy_info.global_airdrop_pointer,
                &user_strategy_info.airdrop_pointer,
                user_strategy_info.shares,
            ) {
                user_airdrops = new_user_airdrops;
            }
            user_strategy_info.airdrop_pointer = strategy_info.global_airdrop_pointer.clone();
            user_reward_info.pending_airdrops = merge_coin_vector(
                user_reward_info.pending_airdrops,
                CoinVecOp {
                    fund: user_airdrops,
                    operation: Operation::Add,
                },
            );

            // update user shares based on the S/T ratio
            let user_shares = decimal_multiplication_in_256(
                current_strategy_shares_per_token_ratio,
                get_decimal_from_uint128(amount),
            );
            user_strategy_info.shares =
                decimal_summation_in_256(user_strategy_info.shares, user_shares);
            // update total strategy shares by adding up the user_shares
            strategy_info.total_shares =
                decimal_summation_in_256(strategy_info.total_shares, user_shares);

            strategy_to_funds
                .entry(strategy_info.sic_contract_address.clone())
                .and_modify(|x| {
                    *x = x.checked_add(amount).unwrap();
                })
                .or_insert(amount);

            STRATEGY_MAP.save(deps.storage, &*strategy_name, &strategy_info)?;
        }

        USER_REWARD_INFO_MAP.save(deps.storage, &user_addr, &user_reward_info)?;
    }

    strategy_to_funds.iter().for_each(|s2a| {
        let sic_address = s2a.0;
        let amount = s2a.1;

        messages.push(WasmMsg::Execute {
            contract_addr: String::from(sic_address),
            msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
            funds: vec![Coin::new(amount.u128(), state.scc_denom.clone())],
        })
    });

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.total_accumulated_rewards = state
            .total_accumulated_rewards
            .checked_add(total_rewards)
            .unwrap();
        Ok(state)
    })?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("failed_strategies", failed_strategies.join(","))
        .add_attribute("inactive_strategies", inactive_strategies.join(","))
        .add_attribute(
            "users_with_zero_deposits",
            users_with_zero_deposits.join(","),
        ))
}

// pub fn update_user_rewards(
//     deps: DepsMut,
//     _env: Env,
//     info: MessageInfo,
//     update_user_rewards_requests: Vec<UpdateUserRewardsRequest>,
// ) -> Result<Response, ContractError> {
//     // check for manager?
//     let state = STATE.load(deps.storage).unwrap();
//     // TODO: bchain99 - can we make access control better?
//     if info.sender != state.pool_contract {
//         return Err(ContractError::Unauthorized {});
//     }
//
//     if update_user_rewards_requests.is_empty() {
//         return Ok(Response::new().add_attribute("zero_update_user_rewards_requests", "1"));
//     }
//
//     let mut messages: Vec<WasmMsg> = vec![];
//     let mut strategy_to_amount: HashMap<Addr, Uint128> = HashMap::new();
//     let mut inactive_strategies: Vec<String> = vec![];
//     let mut failed_strategies: Vec<String> = vec![];
//     let mut zero_amount_users: Vec<String> = vec![];
//     // cache for the S/T ratio per strategy. We fetch it once and cache it up.
//     let mut strategy_to_s_t_ratio: HashMap<String, Decimal> = HashMap::new();
//     // iterate thru all requests. This is technically a paginated batch job running
//     for user_request in update_user_rewards_requests {
//         let user_strategy = user_request.strategy_name;
//         let user_amount = user_request.funds;
//         let user_addr = user_request.user;
//
//         if user_amount.is_zero() {
//             zero_amount_users.push(format!("{}:{}", user_strategy, user_addr.to_string()));
//             continue;
//         }
//
//         let mut strategy_info: StrategyInfo;
//         if let Some(strategy_info_mapping) = STRATEGY_MAP
//             .may_load(deps.storage, &*user_strategy)
//             .unwrap()
//         {
//             strategy_info = strategy_info_mapping;
//         } else {
//             failed_strategies.push(user_strategy);
//             continue;
//         }
//
//         if !strategy_info.is_active {
//             inactive_strategies.push(user_strategy);
//             continue;
//         }
//
//         // fetch the total tokens from the SIC contract and update the S/T ratio for the strategy
//         // compute the S/T ratio only once as we are batching up reward transfer messages. Jus' cache
//         // it up in a map like we always do.
//         let current_strategy_shares_per_token_ratio: Decimal;
//         if !strategy_to_s_t_ratio.contains_key(&user_strategy) {
//             current_strategy_shares_per_token_ratio =
//                 get_strategy_shares_per_token_ratio(deps.querier, &strategy_info);
//             strategy_info.shares_per_token_ratio = current_strategy_shares_per_token_ratio;
//             strategy_to_s_t_ratio.insert(
//                 user_strategy.clone(),
//                 current_strategy_shares_per_token_ratio,
//             );
//         } else {
//             current_strategy_shares_per_token_ratio =
//                 *strategy_to_s_t_ratio.get(&user_strategy).unwrap();
//         }
//
//         // fetch user reward info. If it is not there, create a new UserRewardInfo
//         let mut user_reward_info = UserRewardInfo::new();
//         if let Some(user_reward_info_mapping) = USER_REWARD_INFO_MAP
//             .may_load(deps.storage, &user_addr)
//             .unwrap()
//         {
//             user_reward_info = user_reward_info_mapping;
//         }
//
//         let mut user_strategy_info: &mut UserStrategyInfo;
//         if let Some(i) = (0..user_reward_info.strategies.len())
//             .find(|&i| user_reward_info.strategies[i].strategy_name == strategy_info.name.clone())
//         {
//             user_strategy_info = &mut user_reward_info.strategies[i];
//         } else {
//             let new_user_strategy_info: UserStrategyInfo =
//                 UserStrategyInfo::new(strategy_info.name.clone(), vec![]);
//             user_reward_info.strategies.push(new_user_strategy_info);
//             user_strategy_info = user_reward_info.strategies.last_mut().unwrap();
//         }
//
//         // update the user airdrop pointer and allocate the user pending airdrops for the strategy
//         let mut user_airdrops: Vec<Coin> = vec![];
//         if let Some(new_user_airdrops) = get_user_airdrops(
//             &strategy_info.global_airdrop_pointer,
//             &user_strategy_info.airdrop_pointer,
//             user_strategy_info.shares,
//         ) {
//             user_airdrops = new_user_airdrops;
//         }
//         user_strategy_info.airdrop_pointer = strategy_info.global_airdrop_pointer.clone();
//         user_reward_info.pending_airdrops = merge_coin_vector(
//             user_reward_info.pending_airdrops,
//             CoinVecOp {
//                 fund: user_airdrops,
//                 operation: Operation::Add,
//             },
//         );
//
//         // update user shares based on the S/T ratio
//         let user_shares = decimal_multiplication_in_256(
//             current_strategy_shares_per_token_ratio,
//             get_decimal_from_uint128(user_amount),
//         );
//         user_strategy_info.shares =
//             decimal_summation_in_256(user_strategy_info.shares, user_shares);
//         // update total strategy shares by adding up the user_shares
//         strategy_info.total_shares =
//             decimal_summation_in_256(strategy_info.total_shares, user_shares);
//
//         // do statewise book-keeping like adding up accumulated_rewards
//         STATE.update(deps.storage, |mut state| -> StdResult<_> {
//             state.total_accumulated_rewards = state
//                 .total_accumulated_rewards
//                 .checked_add(user_amount)
//                 .unwrap();
//             Ok(state)
//         })?;
//
//         // batch up the rewards sent to sic
//         let amount_to_insert: Uint128 = user_amount
//             .checked_add(
//                 *strategy_to_amount
//                     .get(&strategy_info.sic_contract_address)
//                     .unwrap_or(&Uint128::zero()),
//             )
//             .unwrap();
//         strategy_to_amount.insert(strategy_info.sic_contract_address.clone(), amount_to_insert);
//
//         // save up the states
//         STRATEGY_MAP.save(deps.storage, &*user_strategy, &strategy_info)?;
//         USER_REWARD_INFO_MAP.save(deps.storage, &user_addr, &user_reward_info)?;
//     }
//
//     strategy_to_amount.iter().for_each(|s2a| {
//         let sic_address = s2a.0;
//         let amount = s2a.1;
//
//         messages.push(WasmMsg::Execute {
//             contract_addr: String::from(sic_address),
//             msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
//             funds: vec![Coin::new(amount.u128(), state.scc_denom.clone())],
//         })
//     });
//
//     Ok(Response::new()
//         .add_messages(messages)
//         .add_attribute("inactive_strategies", inactive_strategies.join(","))
//         .add_attribute("zero_amount_users", zero_amount_users.join(","))
//         .add_attribute("failed_strategies", failed_strategies.join(",")))
// }

// This assumes that the validator contract will transfer ownership of the airdrops
// from the validator contract to the SCC contract.
pub fn try_update_user_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    update_user_airdrops_requests: Vec<UpdateUserAirdropsRequest>,
) -> Result<Response, ContractError> {
    // check for manager?
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.pool_contract {
        return Err(ContractError::Unauthorized {});
    }

    if update_user_airdrops_requests.is_empty() {
        return Ok(Response::new().add_attribute("zero_user_airdrop_requests", "1"));
    }

    // iterate thru update_user_airdrops_request
    let mut total_scc_airdrops: Vec<Coin> = state.total_accumulated_airdrops;
    // accumulate the airdrops in the SCC state.
    for user_request in update_user_airdrops_requests {
        let user = user_request.user;
        let user_airdrops = user_request.pool_airdrops;

        total_scc_airdrops = merge_coin_vector(
            total_scc_airdrops.clone(),
            CoinVecOp {
                fund: user_airdrops.clone(),
                operation: Operation::Add,
            },
        );

        // fetch the user rewards info
        let mut user_reward_info = UserRewardInfo::new();
        if let Some(user_reward_info_mapping) =
            USER_REWARD_INFO_MAP.may_load(deps.storage, &user).unwrap()
        {
            user_reward_info = user_reward_info_mapping;
        }

        user_reward_info.pending_airdrops = merge_coin_vector(
            user_reward_info.pending_airdrops,
            CoinVecOp {
                fund: user_airdrops,
                operation: Operation::Add,
            },
        );

        USER_REWARD_INFO_MAP.save(deps.storage, &user, &user_reward_info)?;
    }

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.total_accumulated_airdrops = total_scc_airdrops;
        Ok(state)
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetState {} => to_binary(&query_state(deps)?),
        QueryMsg::GetStrategyInfo { strategy_name } => {
            to_binary(&query_strategy_info(deps, strategy_name)?)
        }
        QueryMsg::GetUserRewardInfo { user } => to_binary(&query_user_reward_info(deps, user)?),
    }
}

fn query_user_reward_info(deps: Deps, user: Addr) -> StdResult<GetUserRewardInfo> {
    let user_reward_info = USER_REWARD_INFO_MAP.may_load(deps.storage, &user).unwrap();
    Ok(GetUserRewardInfo { user_reward_info })
}

fn query_strategy_info(deps: Deps, strategy_name: String) -> StdResult<GetStrategyInfoResponse> {
    let strategy_info = STRATEGY_MAP
        .may_load(deps.storage, &*strategy_name)
        .unwrap();
    Ok(GetStrategyInfoResponse { strategy_info })
}

fn query_state(deps: Deps) -> StdResult<GetStateResponse> {
    let state = STATE.load(deps.storage)?;
    Ok(GetStateResponse {
        state: Option::from(state),
    })
}
