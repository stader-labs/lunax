#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Attribute, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut,
    Env, Fraction, MessageInfo, Order, QuerierResult, Response, StdError, StdResult, Timestamp,
    Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::helpers::{
    get_strategy_current_undelegation_batch_id, get_strategy_shares_per_token_ratio,
    get_user_staked_amount,
};
use crate::msg::{
    ExecuteMsg, GetStateResponse, GetStrategyInfoResponse, GetUserRewardInfo, InstantiateMsg,
    QueryMsg, UpdateUserAirdropsRequest, UpdateUserRewardsRequest,
};
use crate::state::{
    BatchUndelegationRecord, Cw20TokenContractsInfo, State, StrategyInfo, UserRewardInfo,
    UserStrategyInfo, UserUndelegationRecord, UserUnprocessedUndelegationInfo,
    CW20_TOKEN_CONTRACTS_REGISTRY, STATE, STRATEGY_MAP, STRATEGY_UNPROCESSED_UNDELEGATIONS,
    UNDELEGATION_BATCH_MAP, USER_REWARD_INFO_MAP,
};
use crate::user::get_user_airdrops;
use cw_storage_plus::U64Key;
use serde::Serialize;
use sic_base::msg::{ExecuteMsg as sic_execute_msg, QueryMsg as sic_query_msg};
use stader_utils::coin_utils::{
    decimal_division_in_256, decimal_multiplication_in_256, decimal_subtraction_in_256,
    decimal_summation_in_256, get_decimal_from_uint128, merge_coin_vector, merge_dec_coin_vector,
    u128_from_decimal, uint128_from_decimal, CoinVecOp, DecCoin, DecCoinVecOp, Operation,
};
use std::borrow::Borrow;
use std::collections::HashMap;

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
        event_loop_size: 20,
        total_accumulated_rewards: Uint128::zero(),
        current_rewards_in_scc: Uint128::zero(),
        total_accumulated_airdrops: vec![],
        current_undelegated_strategies: vec![],
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
            strategy_id,
            unbonding_period,
            unbonding_buffer,
            sic_contract_address,
        } => try_register_strategy(
            deps,
            _env,
            info,
            strategy_id,
            sic_contract_address,
            unbonding_period,
            unbonding_buffer,
        ),
        ExecuteMsg::DeactivateStrategy { strategy_id } => {
            try_deactivate_strategy(deps, _env, info, strategy_id)
        }
        ExecuteMsg::ActivateStrategy { strategy_id } => {
            try_activate_strategy(deps, _env, info, strategy_id)
        }
        ExecuteMsg::RemoveStrategy { strategy_id } => {
            try_remove_strategy(deps, _env, info, strategy_id)
        }
        ExecuteMsg::UpdateUserRewards {
            update_user_rewards_requests,
        } => try_update_user_rewards(deps, _env, info, update_user_rewards_requests),
        ExecuteMsg::UpdateUserAirdrops {
            update_user_airdrops_requests,
        } => try_update_user_airdrops(deps, _env, info, update_user_airdrops_requests),
        ExecuteMsg::UndelegateRewards {
            amount,
            strategy_name,
        } => try_undelegate_user_rewards(deps, _env, info, amount, strategy_name),
        ExecuteMsg::ClaimAirdrops {
            strategy_id,
            amount,
            denom,
            claim_msg,
        } => try_claim_airdrops(deps, _env, info, strategy_id, amount, denom, claim_msg),
        ExecuteMsg::WithdrawRewards {
            undelegation_id,
            strategy_name,
            amount,
        } => try_withdraw_rewards(deps, _env, info, undelegation_id, strategy_name, amount),
        ExecuteMsg::WithdrawAirdrops {} => try_withdraw_airdrops(deps, _env, info),
        ExecuteMsg::RegisterCw20Contracts {
            denom,
            cw20_contract,
            airdrop_contract,
        } => try_update_cw20_contracts_registry(
            deps,
            _env,
            info,
            denom,
            cw20_contract,
            airdrop_contract,
        ),
        ExecuteMsg::UndelegateFromStrategies { strategies } => {
            try_undelegate_from_strategies(deps, _env, info, strategies)
        }
        ExecuteMsg::CreateUndelegationBatches { strategies } => {
            try_create_undelegation_batches(deps, _env, info, strategies)
        }
    }
}

pub fn try_undelegate_from_strategies(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategies: Vec<String>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    if strategies.is_empty() {
        return Ok(Response::new().add_attribute("no_strategies", "1"));
    }

    let mut failed_strategies: Vec<String> = vec![];
    let mut strategies_with_no_undelegations: Vec<String> = vec![];
    let mut messages: Vec<WasmMsg> = vec![];
    for strategy in strategies {
        let strategy_info: StrategyInfo;
        if let Some(strategy_info_mapping) =
            STRATEGY_MAP.may_load(deps.storage, &*strategy).unwrap()
        {
            strategy_info = strategy_info_mapping;
        } else {
            failed_strategies.push(strategy);
            continue;
        }

        let strategy_undelegation: Uint128;
        if let Some(undelegation) = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.storage, &*strategy)
            .unwrap()
        {
            strategy_undelegation = undelegation;
        } else {
            strategies_with_no_undelegations.push(strategy);
            continue;
        }

        messages.push(WasmMsg::Execute {
            contract_addr: String::from(strategy_info.sic_contract_address),
            msg: to_binary(&sic_execute_msg::UndelegateRewards {
                amount: strategy_undelegation,
            })
            .unwrap(),
            funds: vec![],
        });

        STRATEGY_UNPROCESSED_UNDELEGATIONS.remove(deps.storage, &*strategy);
    }

    Ok(Response::new()
        .add_attribute("failed_strategies", failed_strategies.join(","))
        .add_attribute(
            "strategies_with_no_undelegations",
            strategies_with_no_undelegations.join(","),
        )
        .add_messages(messages))
}

pub fn try_create_undelegation_batches(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategies: Vec<String>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    if strategies.is_empty() {
        return Ok(Response::new().add_attribute("no_strategies", "1"));
    }

    let mut failed_strategies: Vec<String> = vec![];
    for strategy in strategies {
        let mut strategy_info: StrategyInfo;
        if let Some(strategy_info_mapping) =
            STRATEGY_MAP.may_load(deps.storage, &*strategy).unwrap()
        {
            strategy_info = strategy_info_mapping;
        } else {
            failed_strategies.push(strategy);
            continue;
        }

        let new_current_undelegation_batch_id = strategy_info.current_undelegation_batch_id + 1;
        strategy_info.current_undelegation_batch_id = new_current_undelegation_batch_id;

        UNDELEGATION_BATCH_MAP.save(
            deps.storage,
            (U64Key::new(new_current_undelegation_batch_id), &*strategy),
            &BatchUndelegationRecord {
                amount: Uint128::zero(),
                unbonding_slashing_ratio: Decimal::one(),
                create_time: _env.block.time,
                est_release_time: _env
                    .block
                    .time
                    .plus_seconds(strategy_info.unbonding_period + strategy_info.unbonding_buffer),
                slashing_checked: false,
            },
        )?;
        STRATEGY_MAP.save(deps.storage, &*strategy, &strategy_info)?;
    }

    Ok(Response::new().add_attribute("failed_strategies", failed_strategies.join(",")))
}

// pub fn try_undelegate_from_strategies(
//     deps: DepsMut,
//     _env: Env,
//     info: MessageInfo,
// ) -> Result<Response, ContractError> {
//     let state = STATE.load(deps.storage)?;
//
//     // only manager or the current contract can call it.
//     // create_user_undelegation_records calls undelegate_from_strategies once all user undelegations
//     // have been settled.
//     if info.sender != state.manager && info.sender != _env.contract.address {
//         return Err(ContractError::Unauthorized {});
//     }
//
//     let mut strategy_to_undelegation: HashMap<String, Uint128> = HashMap::new();
//
//     // go thru all strategies and undelegate from them
//     let mut failed_strategies: Vec<String> = vec![];
//     let mut messages: Vec<WasmMsg> = vec![];
//     STRATEGY_UNPROCESSED_UNDELEGATIONS
//         .range(deps.storage, None, None, Order::Ascending)
//         .for_each(|res| {
//             let unwrapped = res.unwrap();
//             let strategy_name = String::from_utf8(unwrapped.0).unwrap();
//             let strategy_undelegation_amount = unwrapped.1;
//
//             if state
//                 .current_undelegated_strategies
//                 .contains(&strategy_name)
//             {
//                 strategy_to_undelegation.insert(strategy_name, strategy_undelegation_amount);
//             }
//         });
//
//     if strategy_to_undelegation.is_empty() {
//         return Ok(Response::new().add_attribute("no_strategies_to_undelegate_from", "0"));
//     }
//
//     strategy_to_undelegation.iter().for_each(|s2u| {
//         let strategy_name = s2u.0;
//         let undelegation_amount = s2u.1;
//
//         let mut strategy_info: StrategyInfo;
//         if let Some(strategy_info_mapping) = STRATEGY_MAP
//             .may_load(deps.storage, &*strategy_name)
//             .unwrap()
//         {
//             strategy_info = strategy_info_mapping;
//         } else {
//             failed_strategies.push(strategy_name.clone());
//             return;
//         }
//
//         // strategy shares reduction will be done when we are creating the user undelegation records.
//         // strategy_info.last_undelegated_time = _env.block.time;
//         messages.push(WasmMsg::Execute {
//             contract_addr: strategy_info.sic_contract_address.to_string(),
//             msg: to_binary(&sic_execute_msg::UndelegateRewards {
//                 amount: undelegation_amount.clone(),
//             })
//             .unwrap(),
//             funds: vec![],
//         });
//
//         STRATEGY_MAP.save(deps.storage, &*strategy_name, &strategy_info);
//         STRATEGY_UNPROCESSED_UNDELEGATIONS.remove(deps.storage, &*strategy_name);
//     });
//
//     // clear the current undelegated strategies vector
//     STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
//         state.current_undelegated_strategies = vec![];
//         Ok(state)
//     })?;
//
//     Ok(Response::new()
//         .add_messages(messages)
//         .add_attribute("failed_strategies", failed_strategies.join(",")))
// }

pub fn try_create_user_undelegation_records(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // let state = STATE.load(deps.storage)?;
    // if info.sender != state.manager {
    //     return Err(ContractError::Unauthorized {});
    // }
    //
    // // iterate thru all the user unprocessed undelegations and create the user undelegation
    // // records for the users currently undelegated.
    // let undelegation_user_addresses: Vec<Addr> = USER_UNPROCESSED_UNDELEGATIONS
    //     .prefix(())
    //     .range(deps.storage, None, None, Order::Ascending)
    //     .take(state.event_loop_size as usize)
    //     .map(|res| {
    //         deps.api
    //             .addr_validate(&String::from_utf8(res.unwrap().0).unwrap())
    //             .unwrap()
    //     })
    //     .collect();
    //
    // if undelegation_user_addresses.is_empty() {
    //     // Now undelegate from all strategies
    //     // when there are no undelegations to process, it could mean 2 things:
    //     // 1. No user has undelegated in which case there will be an empty queue for unprocessed strategies
    //     // which will exit early
    //     // 2. All user undelegations have been settled in which case we can start undelegating from the strategies
    //     // This will execute undelegate from strategies in the same tx which will not cause any problems
    //     // with undelegations being added b/w txs which can happen if create_user_undelegation_records and
    //     // undelegate_from_strategies execute in different txs.
    //     return Ok(Response::new()
    //         .add_message(WasmMsg::Execute {
    //             contract_addr: String::from(env.contract.address),
    //             msg: to_binary(&ExecuteMsg::UndelegateFromStrategies {}).unwrap(),
    //             funds: vec![],
    //         })
    //         .add_attribute("undelegated_from_strategies", "1"));
    // }
    //
    // // IMPORTANT: only create the records for users if the strategy has crossed its cooling period
    // let mut failed_strategies: Vec<String> = vec![];
    // let mut failed_users: Vec<String> = vec![];
    // let mut strategies_in_cooldown: Vec<String> = vec![];
    // let mut undelegated_strategies: Vec<String> = vec![];
    // let mut strategy_to_s_t_ratio: HashMap<String, Decimal> = HashMap::new();
    // undelegation_user_addresses.iter().for_each(|user_addr| {
    //     let user_unprocessed_undelegation_records: Vec<UserUnprocessedUndelegationInfo>;
    //     if let Some(uuur_map) = USER_UNPROCESSED_UNDELEGATIONS
    //         .may_load(deps.storage, user_addr)
    //         .unwrap()
    //     {
    //         user_unprocessed_undelegation_records = uuur_map;
    //     } else {
    //         return;
    //     }
    //
    //     let mut user_reward_info: UserRewardInfo;
    //     if let Some(user_reward_info_map) = USER_REWARD_INFO_MAP
    //         .may_load(deps.storage, user_addr)
    //         .unwrap()
    //     {
    //         user_reward_info = user_reward_info_map;
    //     } else {
    //         failed_users.push(user_addr.clone().to_string());
    //         return;
    //     }
    //
    //     for user_unprocessed_undelegation_record in user_unprocessed_undelegation_records {
    //         let strategy_name = user_unprocessed_undelegation_record.strategy_name;
    //         let undelegation_amount = user_unprocessed_undelegation_record.undelegation_amount;
    //
    //         if strategies_in_cooldown.contains(&strategy_name)
    //             || failed_strategies.contains(&strategy_name)
    //         {
    //             continue;
    //         }
    //
    //         let mut strategy_info: StrategyInfo;
    //         if let Some(strategy_info_mapping) = STRATEGY_MAP
    //             .may_load(deps.storage, &*strategy_name)
    //             .unwrap()
    //         {
    //             strategy_info = strategy_info_mapping;
    //         } else {
    //             failed_strategies.push(strategy_name);
    //             continue;
    //         }
    //
    //         if strategy_info
    //             .last_undelegated_time
    //             .plus_seconds(strategy_info.cooling_period)
    //             .lt(&env.block.time)
    //         {
    //             strategies_in_cooldown.push(strategy_name);
    //             continue;
    //         }
    //
    //         let user_strategy_info: &mut UserStrategyInfo;
    //         if let Some(user_strategy_info_mapping) = user_reward_info
    //             .strategies
    //             .iter_mut()
    //             .find(|x| x.strategy_name.eq(&strategy_name))
    //         {
    //             user_strategy_info = user_strategy_info_mapping;
    //         } else {
    //             continue;
    //         }
    //
    //         // compute the S/T ratio only once.
    //         let current_strategy_shares_per_token_ratio: Decimal;
    //         if !strategy_to_s_t_ratio.contains_key(&strategy_name) {
    //             current_strategy_shares_per_token_ratio =
    //                 get_strategy_shares_per_token_ratio(deps.querier, &strategy_info);
    //             strategy_info.shares_per_token_ratio = current_strategy_shares_per_token_ratio;
    //             strategy_to_s_t_ratio.insert(
    //                 strategy_name.clone(),
    //                 current_strategy_shares_per_token_ratio,
    //             );
    //         } else {
    //             current_strategy_shares_per_token_ratio =
    //                 *strategy_to_s_t_ratio.get(&strategy_name).unwrap();
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
    //         let total_user_undelegated_shares = decimal_multiplication_in_256(
    //             current_strategy_shares_per_token_ratio,
    //             Decimal::from_ratio(undelegation_amount.u128(), 1_u128),
    //         );
    //         user_strategy_info.shares = decimal_subtraction_in_256(
    //             user_strategy_info.shares,
    //             total_user_undelegated_shares,
    //         );
    //         strategy_info.total_shares = decimal_subtraction_in_256(
    //             strategy_info.total_shares,
    //             total_user_undelegated_shares,
    //         );
    //
    //         let strategy_undelegation_batch_id =
    //             get_strategy_current_undelegation_batch_id(deps.querier, &strategy_info);
    //         let strategy_unbonding_period = strategy_info.unbonding_period;
    //
    //         STRATEGY_MAP.save(deps.storage, &*strategy_name, &strategy_info);
    //         undelegated_strategies.push(strategy_name.clone());
    //
    //         user_reward_info
    //             .undelegation_records
    //             .push(UserUndelegationRecord {
    //                 id: env.block.time,
    //                 amount: undelegation_amount,
    //                 strategy_name,
    //                 est_release_time: env.block.time.plus_seconds(strategy_unbonding_period),
    //                 // TODO: bchain99 - discuss this once with someone. We can have a separate create_undelegation_batch interface
    //                 // in the SIC. maybe we can avoid this.
    //                 undelegation_batch_id: strategy_undelegation_batch_id + 1,
    //             });
    //     }
    //     USER_REWARD_INFO_MAP.save(deps.storage, &user_addr, &user_reward_info);
    //     USER_UNPROCESSED_UNDELEGATIONS.remove(deps.storage, user_addr);
    // });
    //
    // STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
    //     state.current_undelegated_strategies = undelegated_strategies;
    //     Ok(state)
    // })?;
    //
    // Ok(Response::new()
    //     .add_attribute("failed_strategies", failed_strategies.join(","))
    //     .add_attribute("strategies_in_cooldown", strategies_in_cooldown.join(","))
    //     .add_attribute("failed_users", failed_users.join(",")))
    Ok(Response::default())
}

pub fn try_update_cw20_contracts_registry(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    denom: String,
    cw20_token_contract: Addr,
    airdrop_contract: Addr,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if state.manager != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    CW20_TOKEN_CONTRACTS_REGISTRY.save(
        deps.storage,
        denom,
        &Cw20TokenContractsInfo {
            airdrop_contract,
            cw20_token_contract,
        },
    );

    Ok(Response::default())
}

pub fn try_undelegate_user_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
    strategy_name: String,
) -> Result<Response, ContractError> {
    if amount.is_zero() {
        return Err(ContractError::CannotUndelegateZeroFunds {});
    }

    let user_addr = info.sender;

    let mut strategy_info: StrategyInfo;
    if let Some(strategy_info_mapping) = STRATEGY_MAP.may_load(deps.storage, &*strategy_name)? {
        strategy_info = strategy_info_mapping;
    } else {
        return Err(ContractError::StrategyInfoDoesNotExist(strategy_name));
    }

    let mut user_reward_info: UserRewardInfo;
    if let Some(user_reward_info_mapping) =
        USER_REWARD_INFO_MAP.may_load(deps.storage, &user_addr)?
    {
        user_reward_info = user_reward_info_mapping;
    } else {
        return Err(ContractError::UserRewardInfoDoesNotExist {});
    }

    // check if the undelegation amount crosses the user total undelegatable amount
    let user_strategy_info: &mut UserStrategyInfo;
    if let Some(user_strategy_info_mapping) = user_reward_info
        .strategies
        .iter_mut()
        .find(|x| x.strategy_name.eq(&strategy_name))
    {
        user_strategy_info = user_strategy_info_mapping;
    } else {
        return Err(ContractError::UserNotInStrategy {});
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

    // subtract the user shares based on how much they want to withdraw.
    let strategy_shares_per_token_ratio =
        get_strategy_shares_per_token_ratio(deps.querier, &strategy_info);
    strategy_info.shares_per_token_ratio = strategy_shares_per_token_ratio;
    let user_total_shares = user_strategy_info.shares;
    let user_undelegated_shares = decimal_multiplication_in_256(
        strategy_shares_per_token_ratio,
        Decimal::from_ratio(amount, 1_u128),
    );

    if user_undelegated_shares.gt(&user_total_shares) {
        return Err(ContractError::UserDoesNotHaveEnoughRewards {});
    }

    user_strategy_info.shares =
        decimal_subtraction_in_256(user_total_shares, user_undelegated_shares);
    strategy_info.total_shares =
        decimal_subtraction_in_256(strategy_info.total_shares, user_undelegated_shares);

    user_reward_info
        .undelegation_records
        .push(UserUndelegationRecord {
            // there may multiple records with the same id. when withdrawing, use amount as the tie-breaker in such a case.
            id: _env.block.time,
            amount,
            strategy_name: strategy_name.clone(),
            est_release_time: _env
                .block
                .time
                .plus_seconds(strategy_info.unbonding_period + strategy_info.unbonding_buffer),
            undelegation_batch_id: strategy_info.current_undelegation_batch_id,
        });

    STRATEGY_MAP.save(deps.storage, &*strategy_name, &strategy_info)?;
    USER_REWARD_INFO_MAP.save(deps.storage, &user_addr, &user_reward_info)?;

    STRATEGY_UNPROCESSED_UNDELEGATIONS.update(
        deps.storage,
        &*strategy_name,
        |suu| -> Result<_, ContractError> {
            Ok(suu.unwrap_or(Uint128::zero()).checked_add(amount).unwrap())
        },
    )?;

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.total_accumulated_rewards =
            state.total_accumulated_rewards.checked_sub(amount).unwrap();
        Ok(state)
    })?;

    Ok(Response::default())
}

pub fn try_claim_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_id: String,
    amount: Uint128,
    denom: String,
    claim_msg: Binary,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    let mut cw20_token_contracts: Cw20TokenContractsInfo;
    // TODO: bchain99 - abstract these into functions
    if let Some(cw20_token_contracts_mapping) = CW20_TOKEN_CONTRACTS_REGISTRY
        .may_load(deps.storage, denom.clone())
        .unwrap()
    {
        cw20_token_contracts = cw20_token_contracts_mapping;
    } else {
        return Err(ContractError::AirdropNotRegistered {});
    }

    let mut strategy_info: StrategyInfo;
    if let Some(strategy_info_mapping) = STRATEGY_MAP.may_load(deps.storage, &*strategy_id).unwrap()
    {
        // while registering the strategy, we need to update the airdrops the strategy supports.
        strategy_info = strategy_info_mapping;
    } else {
        return Err(ContractError::StrategyInfoDoesNotExist(strategy_id));
    }

    let total_shares = strategy_info.total_shares;
    let sic_address = strategy_info.sic_contract_address.clone();
    let airdrop_coin = Coin::new(amount.u128(), denom.clone());

    strategy_info.total_airdrops_accumulated = merge_coin_vector(
        strategy_info.total_airdrops_accumulated,
        CoinVecOp {
            fund: vec![airdrop_coin.clone()],
            operation: Operation::Add,
        },
    );

    strategy_info.global_airdrop_pointer = merge_dec_coin_vector(
        &strategy_info.global_airdrop_pointer,
        DecCoinVecOp {
            fund: vec![DecCoin {
                amount: decimal_division_in_256(Decimal::from_ratio(amount, 1_u128), total_shares),
                denom: denom.clone(),
            }],
            operation: Operation::Add,
        },
    );

    STRATEGY_MAP.save(deps.storage, &*strategy_id, &strategy_info)?;

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.total_accumulated_airdrops = merge_coin_vector(
            state.total_accumulated_airdrops,
            CoinVecOp {
                fund: vec![airdrop_coin],
                operation: Operation::Add,
            },
        );
        Ok(state)
    })?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: String::from(sic_address),
        msg: to_binary(&sic_execute_msg::ClaimAirdrops {
            airdrop_token_contract: cw20_token_contracts.airdrop_contract,
            cw20_token_contract: cw20_token_contracts.cw20_token_contract,
            airdrop_token: denom,
            amount,
            claim_msg,
        })
        .unwrap(),
        funds: vec![],
    }))
}

pub fn try_withdraw_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // transfer the airdrops in pending_airdrops vector in user_reward_info to the user
    let user_addr = info.sender;

    let mut user_reward_info: UserRewardInfo;
    match USER_REWARD_INFO_MAP
        .may_load(deps.storage, &user_addr)
        .unwrap()
    {
        None => return Err(ContractError::UserRewardInfoDoesNotExist {}),
        Some(user_reward_info_mapping) => {
            user_reward_info = user_reward_info_mapping;
        }
    }

    // allocate user airdrops across strategies
    let mut failed_strategies: Vec<String> = vec![];
    let mut total_allocated_user_airdrops: Vec<Coin> = vec![];
    for user_strategy in &mut user_reward_info.strategies {
        let strategy_name = user_strategy.strategy_name.clone();
        let user_airdrop_pointer = &user_strategy.airdrop_pointer;
        let user_shares = user_strategy.shares;

        let strategy_info: StrategyInfo;
        if let Some(strategy_info_mapping) = STRATEGY_MAP
            .may_load(deps.storage, &*strategy_name)
            .unwrap()
        {
            strategy_info = strategy_info_mapping;
        } else {
            failed_strategies.push(strategy_name);
            continue;
        }

        let strategy_global_airdrop_pointer = strategy_info.global_airdrop_pointer;
        let user_airdrops_for_strategy = get_user_airdrops(
            &strategy_global_airdrop_pointer,
            user_airdrop_pointer,
            user_shares,
        );
        if user_airdrops_for_strategy.is_some() {
            total_allocated_user_airdrops = merge_coin_vector(
                total_allocated_user_airdrops,
                CoinVecOp {
                    fund: user_airdrops_for_strategy.unwrap(),
                    operation: Operation::Add,
                },
            );
        }
        user_strategy.airdrop_pointer = strategy_global_airdrop_pointer;
    }

    let mut messages: Vec<WasmMsg> = vec![];
    let mut failed_airdrops: Vec<String> = vec![];
    let total_airdrops = merge_coin_vector(
        user_reward_info.pending_airdrops,
        CoinVecOp {
            fund: total_allocated_user_airdrops,
            operation: Operation::Add,
        },
    );
    // iterate thru all airdrops and transfer ownership to them to the user
    for user_airdrop in total_airdrops.iter() {
        let airdrop_denom = user_airdrop.denom.clone();
        let airdrop_amount = user_airdrop.amount;

        let cw20_token_contracts = CW20_TOKEN_CONTRACTS_REGISTRY
            .may_load(deps.storage, airdrop_denom.clone())
            .unwrap();

        messages.push(WasmMsg::Execute {
            contract_addr: String::from(cw20_token_contracts.unwrap().cw20_token_contract),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                recipient: String::from(user_addr.clone()),
                amount: airdrop_amount,
            })
            .unwrap(),
            funds: vec![],
        });
    }

    user_reward_info.pending_airdrops = vec![];

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.total_accumulated_airdrops = merge_coin_vector(
            state.total_accumulated_airdrops,
            CoinVecOp {
                fund: total_airdrops,
                operation: Operation::Sub,
            },
        );
        Ok(state)
    })?;

    USER_REWARD_INFO_MAP.save(deps.storage, &user_addr, &user_reward_info)?;

    Ok(Response::new()
        .add_attribute(
            "strategies_failed_airdrop_allocation",
            failed_strategies.join(","),
        )
        .add_messages(messages))
}

pub fn try_withdraw_rewards(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    undelegation_id: String,
    strategy_name: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let user_addr = info.sender;

    let strategy_info: StrategyInfo;
    if let Some(strategy_info_mapping) = STRATEGY_MAP
        .may_load(deps.storage, &*strategy_name)
        .unwrap()
    {
        strategy_info = strategy_info_mapping;
    } else {
        return Err(ContractError::StrategyInfoDoesNotExist(strategy_name));
    }

    let user_reward_info: UserRewardInfo;
    if let Some(user_reward_info_mapping) = USER_REWARD_INFO_MAP
        .may_load(deps.storage, &user_addr)
        .unwrap()
    {
        user_reward_info = user_reward_info_mapping;
    } else {
        return Err(ContractError::UserRewardInfoDoesNotExist {});
    }

    let undelegation_timestamp = Timestamp::from_nanos(undelegation_id.parse::<u64>().unwrap());

    let user_undelegation_record: &UserUndelegationRecord;
    if let Some(user_undelegation_record_mappings) =
        user_reward_info.undelegation_records.iter().find(|x| {
            x.id.eq(&undelegation_timestamp)
                && x.strategy_name.eq(&strategy_name)
                && x.amount.eq(&amount)
        })
    {
        user_undelegation_record = user_undelegation_record_mappings;
    } else {
        return Err(ContractError::UndelegationRecordNotFound {});
    }

    let undelegation_batch_id = U64Key::new(user_undelegation_record.undelegation_batch_id);
    let undelegation_batch: BatchUndelegationRecord;
    if let Some(undelegation_batch_map) = UNDELEGATION_BATCH_MAP
        .may_load(deps.storage, (undelegation_batch_id, &*strategy_name))
        .unwrap()
    {
        undelegation_batch = undelegation_batch_map;
    } else {
        return Err(ContractError::UndelegationBatchNotFound {});
    }

    if undelegation_batch.est_release_time.gt(&env.block.time) {
        return Err(ContractError::UndelegationInUnbondingPeriod {});
    }

    if !undelegation_batch.slashing_checked {
        return Err(ContractError::SlashingNotChecked {});
    }

    let effective_user_withdrawable_amount = u128_from_decimal(decimal_multiplication_in_256(
        Decimal::from_ratio(amount, 1_u128),
        undelegation_batch.unbonding_slashing_ratio,
    ));

    let mut failed_message: Vec<Attribute> = vec![];
    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.current_rewards_in_scc = state
            .current_rewards_in_scc
            .checked_sub(Uint128::new(effective_user_withdrawable_amount))
            .unwrap_or_else(|x| {
                failed_message.push(attr("current_rewards_in_scc_overflow_error", "1"));
                state.current_rewards_in_scc
            });
        Ok(state)
    })?;

    Ok(Response::new()
        .add_message(BankMsg::Send {
            to_address: String::from(user_addr),
            amount: vec![Coin::new(
                effective_user_withdrawable_amount,
                state.scc_denom,
            )],
        })
        .add_attributes(failed_message))
}

pub fn try_register_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_id: String,
    sic_contract_address: Addr,
    unbonding_period: Option<u64>,
    unbonding_buffer: Option<u64>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    if STRATEGY_MAP
        .may_load(deps.storage, &*strategy_id)
        .unwrap()
        .is_some()
    {
        return Err(ContractError::StrategyInfoAlreadyExists {});
    }

    STRATEGY_MAP.save(
        deps.storage,
        &*strategy_id.clone(),
        &StrategyInfo::new(
            strategy_id,
            sic_contract_address,
            unbonding_period,
            unbonding_buffer,
        ),
    )?;

    Ok(Response::default())
}

pub fn try_deactivate_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_id: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    STRATEGY_MAP.update(
        deps.storage,
        &*strategy_id.clone(),
        |strategy_info_opt| -> Result<_, ContractError> {
            if strategy_info_opt.is_none() {
                return Err(ContractError::StrategyInfoDoesNotExist(strategy_id));
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
    strategy_id: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    STRATEGY_MAP.update(
        deps.storage,
        &*strategy_id.clone(),
        |strategy_info_option| -> Result<_, ContractError> {
            if strategy_info_option.is_none() {
                return Err(ContractError::StrategyInfoDoesNotExist(strategy_id));
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
    strategy_id: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    STRATEGY_MAP.remove(deps.storage, &*strategy_id);

    Ok(Response::default())
}

pub fn try_update_user_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    update_user_rewards_requests: Vec<UpdateUserRewardsRequest>,
) -> Result<Response, ContractError> {
    // check for manager?
    let state = STATE.load(deps.storage).unwrap();
    // TODO: bchain99 - can we make access control better?
    if info.sender != state.pool_contract {
        return Err(ContractError::Unauthorized {});
    }

    if update_user_rewards_requests.is_empty() {
        return Ok(Response::default());
    }

    let mut messages: Vec<WasmMsg> = vec![];
    let mut strategy_to_amount: HashMap<Addr, Uint128> = HashMap::new();
    let mut strategy_to_s_t_ratio: HashMap<String, Decimal> = HashMap::new();
    // iterate thru all requests. This is technically a paginated batch job running
    for user_request in update_user_rewards_requests {
        let user_strategy = user_request.strategy_id;
        let user_amount = user_request.rewards;
        let user_addr = user_request.user;

        let mut strategy_info: StrategyInfo;
        if let Some(strategy_info_mapping) = STRATEGY_MAP
            .may_load(deps.storage, &*user_strategy)
            .unwrap()
        {
            strategy_info = strategy_info_mapping;
        } else {
            // TODO: bchain99 - Review if we can exit gracefully over here
            return Err(ContractError::StrategyInfoDoesNotExist(user_strategy));
        }

        // fetch the total tokens from the SIC contract and update the S/T ratio for the strategy
        // compute the S/T ratio only once as we are batching up reward transfer messages. Jus' cache
        // it up in a map like we always do.
        let current_strategy_shares_per_token_ratio: Decimal;
        if !strategy_to_s_t_ratio.contains_key(&user_strategy) {
            current_strategy_shares_per_token_ratio =
                get_strategy_shares_per_token_ratio(deps.querier, &strategy_info);
            strategy_info.shares_per_token_ratio = current_strategy_shares_per_token_ratio;
            strategy_to_s_t_ratio.insert(
                user_strategy.clone(),
                current_strategy_shares_per_token_ratio,
            );
        } else {
            current_strategy_shares_per_token_ratio =
                *strategy_to_s_t_ratio.get(&user_strategy).unwrap();
        }

        // fetch user reward info. If it is not there, create a new UserRewardInfo
        let mut user_reward_info = UserRewardInfo::new();
        if let Some(user_reward_info_mapping) = USER_REWARD_INFO_MAP
            .may_load(deps.storage, &user_addr)
            .unwrap()
        {
            user_reward_info = user_reward_info_mapping;
        }

        let mut user_strategy_info: &mut UserStrategyInfo;
        if let Some(i) = (0..user_reward_info.strategies.len())
            .find(|&i| user_reward_info.strategies[i].strategy_name == strategy_info.name.clone())
        {
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
            get_decimal_from_uint128(user_amount),
        );
        user_strategy_info.shares =
            decimal_summation_in_256(user_strategy_info.shares, user_shares);
        // update total strategy shares by adding up the user_shares
        strategy_info.total_shares =
            decimal_summation_in_256(strategy_info.total_shares, user_shares);

        // do statewise book-keeping like adding up accumulated_rewards
        STATE.update(deps.storage, |mut state| -> StdResult<_> {
            state.total_accumulated_rewards = state
                .total_accumulated_rewards
                .checked_add(user_amount)
                .unwrap();
            Ok(state)
        })?;

        // batch up the rewards sent to sic
        let amount_to_insert: Uint128 = user_amount
            .checked_add(
                *strategy_to_amount
                    .get(&strategy_info.sic_contract_address)
                    .unwrap_or(&Uint128::zero()),
            )
            .unwrap();
        strategy_to_amount.insert(strategy_info.sic_contract_address.clone(), amount_to_insert);

        // save up the states
        STRATEGY_MAP.save(deps.storage, &*user_strategy, &strategy_info)?;
        USER_REWARD_INFO_MAP.save(deps.storage, &user_addr, &user_reward_info)?;
    }

    strategy_to_amount.iter().for_each(|s2a| {
        let sic_address = s2a.0;
        let amount = s2a.1;

        messages.push(WasmMsg::Execute {
            contract_addr: String::from(sic_address),
            msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
            funds: vec![Coin::new(amount.u128(), state.scc_denom.clone())],
        })
    });

    Ok(Response::new().add_messages(messages))
}

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
        return Ok(Response::default());
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
