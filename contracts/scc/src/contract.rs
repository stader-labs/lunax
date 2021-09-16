#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Attribute, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut,
    Env, Fraction, MessageInfo, Order, QuerierResult, Response, StdError, StdResult, Timestamp,
    Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::helpers::{
    get_sic_fulfillable_undelegated_funds, get_staked_amount, get_strategy_shares_per_token_ratio,
    get_strategy_split,
};
use crate::msg::{
    ExecuteMsg, GetAllStrategiesResponse, GetConfigResponse, GetStateResponse,
    GetStrategiesListResponse, GetStrategyInfoResponse, GetUndelegationBatchInfoResponse,
    GetUserResponse, GetUserRewardInfo, InstantiateMsg, QueryMsg, StrategyInfoQuery,
    UpdateUserAirdropsRequest, UpdateUserRewardsRequest, UserRewardInfoQuery,
    UserStrategyQueryInfo,
};
use crate::state::{
    BatchUndelegationRecord, Config, Cw20TokenContractsInfo, State, StrategyInfo, UserRewardInfo,
    UserStrategyInfo, UserStrategyPortfolio, UserUndelegationRecord, CONFIG,
    CW20_TOKEN_CONTRACTS_REGISTRY, STATE, STRATEGY_MAP, UNDELEGATION_BATCH_MAP,
    USER_REWARD_INFO_MAP,
};
use crate::user::{allocate_user_airdrops_across_strategies, get_user_airdrops};
use cw_storage_plus::U64Key;
use serde::Serialize;
use sic_base::msg::{ExecuteMsg as sic_execute_msg, QueryMsg as sic_query_msg};
use stader_utils::coin_utils::{
    decimal_division_in_256, decimal_multiplication_in_256, decimal_subtraction_in_256,
    decimal_summation_in_256, get_decimal_from_uint128, merge_coin_vector, merge_dec_coin_vector,
    u128_from_decimal, uint128_from_decimal, CoinVecOp, DecCoin, DecCoinVecOp, Operation,
};
use stader_utils::helpers::send_funds_msg;
use std::cmp::min;
use std::collections::HashMap;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let config = Config {
        default_user_portfolio: msg.default_user_portfolio.unwrap_or_default(),
    };

    let state = State {
        manager: info.sender.clone(),
        pool_contract: msg.pools_contract,
        scc_denom: msg.strategy_denom,
        contract_genesis_block_height: _env.block.height,
        contract_genesis_timestamp: _env.block.time,
        rewards_in_scc: Uint128::zero(),
        total_accumulated_airdrops: vec![],
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
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterStrategy {
            strategy_name,
            unbonding_period,
            unbonding_buffer,
            sic_contract_address,
        } => try_register_strategy(
            deps,
            _env,
            info,
            strategy_name,
            sic_contract_address,
            unbonding_period,
            unbonding_buffer,
        ),
        // TODO: bchain99 - deactivate/activate can come under update_strategy
        ExecuteMsg::DeactivateStrategy { strategy_name } => {
            try_deactivate_strategy(deps, _env, info, strategy_name)
        }
        ExecuteMsg::ActivateStrategy { strategy_name } => {
            try_activate_strategy(deps, _env, info, strategy_name)
        }
        ExecuteMsg::UpdateStrategy {
            strategy_name,
            unbonding_period,
            unbonding_buffer,
            is_active,
        } => try_update_strategy(
            deps,
            _env,
            info,
            strategy_name,
            unbonding_period,
            unbonding_buffer,
            is_active,
        ),
        ExecuteMsg::RemoveStrategy { strategy_name } => {
            try_remove_strategy(deps, _env, info, strategy_name)
        }
        ExecuteMsg::UpdateUserPortfolio { user_portfolio } => {
            try_update_user_portfolio(deps, _env, info, user_portfolio)
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
            strategy_name,
            amount,
            denom,
            claim_msg,
        } => try_claim_airdrops(deps, _env, info, strategy_name, amount, denom, claim_msg),
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
        ExecuteMsg::FetchUndelegatedRewardsFromStrategies { strategies } => {
            try_fetch_undelegated_rewards_from_strategies(deps, _env, info, strategies)
        }
        ExecuteMsg::WithdrawPendingRewards {} => try_withdraw_pending_rewards(deps, _env, info),
    }
}

pub fn try_update_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_name: String,
    unbonding_period: u64,
    unbonding_buffer: u64,
    is_active: bool,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    STRATEGY_MAP.update(
        deps.storage,
        &*strategy_name.clone(),
        |wrapped_strategy| -> Result<_, ContractError> {
            if wrapped_strategy.is_none() {
                return Err(ContractError::StrategyInfoDoesNotExist(strategy_name));
            }

            let mut strategy_info = wrapped_strategy.unwrap();
            strategy_info.unbonding_period = unbonding_period;
            strategy_info.unbonding_buffer = unbonding_buffer;
            strategy_info.is_active = is_active;

            Ok(strategy_info)
        },
    )?;

    Ok(Response::default())
}

pub fn try_withdraw_pending_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    let user_addr = info.sender;

    let mut user_reward_info: UserRewardInfo;
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

    user_reward_info.pending_rewards = Uint128::zero();

    USER_REWARD_INFO_MAP.save(deps.storage, &user_addr, &user_reward_info)?;

    Ok(Response::new().add_message(send_funds_msg(
        &user_addr,
        &vec![Coin::new(user_pending_rewards.u128(), state.scc_denom)],
    )))
}

pub fn try_fetch_undelegated_rewards_from_strategies(
    deps: DepsMut,
    env: Env,
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

    let mut messages: Vec<WasmMsg> = vec![];
    // I know, this is a lot of logging.
    // well, we need to know what went wrong right? :P
    let mut failed_strategies: Vec<String> = vec![];
    let mut failed_undelegation_batches: Vec<String> = vec![];
    let mut undelegation_batches_in_unbonding_period: Vec<String> = vec![];
    let mut undelegation_batches_slashing_checked: Vec<String> = vec![];
    let mut total_funds_transferred_to_scc: Uint128 = Uint128::zero();
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

        let last_reconciled_batch_id = strategy_info.reconciled_batch_id_pointer;
        let mut current_batch_being_reconciled = last_reconciled_batch_id;
        let strategy_name = strategy_info.name.clone();
        for i in (last_reconciled_batch_id)..(strategy_info.undelegation_batch_id_pointer) {
            let mut undelegation_batch: BatchUndelegationRecord;
            if let Some(undelegation_batch_map) = UNDELEGATION_BATCH_MAP
                .may_load(deps.storage, (U64Key::new(i), &*strategy_name))
                .unwrap()
            {
                undelegation_batch = undelegation_batch_map;
            } else {
                // move the pointer if there is no undelegation batch has been found.
                failed_undelegation_batches.push(format!(
                    "{}:{}",
                    strategy_name.as_str(),
                    i.to_string(),
                ));
                current_batch_being_reconciled += 1;
                continue;
            }

            // undelegation batch has been checked for slashing. we can move the pointer, if the
            // batch has been slashed
            if undelegation_batch.slashing_checked {
                undelegation_batches_slashing_checked.push(format!(
                    "{}:{}",
                    strategy_name.as_str(),
                    i.to_string(),
                ));
                current_batch_being_reconciled += 1;
                continue;
            }

            // undelegation batch is still in unbonding period
            // break out when we encounter a batch still in undelegation. we need to wait for it and not pass by it
            if undelegation_batch.est_release_time.gt(&env.block.time) {
                undelegation_batches_in_unbonding_period.push(format!(
                    "{}:{}",
                    strategy_name.as_str(),
                    i.to_string(),
                ));
                break;
            }

            let undelegation_batch_amount = undelegation_batch.amount;

            // query the undelegated funds which we will receive from the SIC on best effort
            let fulfillable_amount = get_sic_fulfillable_undelegated_funds(
                deps.querier,
                undelegation_batch_amount,
                &strategy_info.sic_contract_address,
            );
            let unbonding_slashing_ratio = if fulfillable_amount.lt(&undelegation_batch_amount) {
                Decimal::from_ratio(fulfillable_amount, undelegation_batch_amount)
            } else {
                Decimal::one()
            };

            undelegation_batch.unbonding_slashing_ratio = unbonding_slashing_ratio;
            undelegation_batch.slashing_checked = true;

            total_funds_transferred_to_scc = total_funds_transferred_to_scc
                .checked_add(fulfillable_amount)
                .unwrap();

            messages.push(WasmMsg::Execute {
                contract_addr: strategy_info.sic_contract_address.to_string(),
                msg: to_binary(&sic_execute_msg::TransferUndelegatedRewards {
                    amount: undelegation_batch_amount,
                })
                .unwrap(),
                funds: vec![],
            });

            UNDELEGATION_BATCH_MAP.save(
                deps.storage,
                (U64Key::new(i), &*strategy_name),
                &undelegation_batch,
            )?;

            current_batch_being_reconciled += 1;
        }

        strategy_info.reconciled_batch_id_pointer = current_batch_being_reconciled;
        STRATEGY_MAP.save(deps.storage, &*strategy_name, &strategy_info)?;
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("failed_strategies", failed_strategies.join(","))
        .add_attribute(
            "failed_undelegation_batches",
            failed_undelegation_batches.join(","),
        )
        .add_attribute(
            "undelegation_batches_in_unbonding_period",
            undelegation_batches_in_unbonding_period.join(","),
        )
        .add_attribute(
            "undelegation_batches_slashing_checked",
            undelegation_batches_slashing_checked.join(","),
        ))
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
    let mut failed_sics: Vec<String> = vec![];
    let mut strategies_with_no_undelegations: Vec<String> = vec![];
    let mut messages: Vec<WasmMsg> = vec![];
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

        let strategy_undelegation_shares: Decimal = strategy_info.current_undelegated_shares;
        if strategy_undelegation_shares.is_zero() {
            strategies_with_no_undelegations.push(strategy.clone());
            continue;
        }

        let strategy_s_t_ratio: Decimal;
        match get_strategy_shares_per_token_ratio(deps.querier, &strategy_info) {
            Ok(result) => {
                strategy_s_t_ratio = result;
            }
            Err(_) => {
                failed_sics.push(strategy_info.name);
                continue;
            }
        }

        let undelegation_amount = uint128_from_decimal(decimal_division_in_256(
            strategy_undelegation_shares,
            strategy_s_t_ratio,
        ));

        let current_undelegation_batch_id = strategy_info.undelegation_batch_id_pointer;
        let new_undelegation_batch_id = current_undelegation_batch_id + 1;

        UNDELEGATION_BATCH_MAP.save(
            deps.storage,
            (U64Key::new(current_undelegation_batch_id), &*strategy),
            &BatchUndelegationRecord {
                amount: undelegation_amount,
                shares: strategy_undelegation_shares,
                // we get to know this when we fetch the undelegated amount from sic
                unbonding_slashing_ratio: Decimal::one(),
                // this is the S/T ratio when the amount is undelegated.
                undelegation_s_t_ratio: strategy_s_t_ratio,
                create_time: _env.block.time,
                est_release_time: _env
                    .block
                    .time
                    .plus_seconds(strategy_info.unbonding_period + strategy_info.unbonding_buffer),
                slashing_checked: false,
            },
        )?;

        strategy_info.undelegation_batch_id_pointer = new_undelegation_batch_id;
        strategy_info.total_shares = decimal_subtraction_in_256(
            strategy_info.total_shares,
            strategy_info.current_undelegated_shares,
        );
        strategy_info.current_undelegated_shares = Decimal::zero();

        messages.push(WasmMsg::Execute {
            contract_addr: String::from(strategy_info.sic_contract_address.clone()),
            msg: to_binary(&sic_execute_msg::UndelegateRewards {
                amount: undelegation_amount,
            })
            .unwrap(),
            funds: vec![],
        });

        STRATEGY_MAP.save(deps.storage, &*strategy, &strategy_info)?;
    }

    Ok(Response::new()
        .add_attribute("failed_strategies", failed_strategies.join(","))
        .add_attribute(
            "strategies_with_no_undelegations",
            strategies_with_no_undelegations.join(","),
        )
        .add_attribute("failed_sics", failed_sics.join(","))
        .add_messages(messages))
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
    )?;

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
    let strategy_shares_per_token_ratio: Decimal;
    match get_strategy_shares_per_token_ratio(deps.querier, &strategy_info) {
        Ok(result) => {
            strategy_shares_per_token_ratio = result;
        }
        Err(_) => {
            return Err(ContractError::SICFailedToReturnResult {});
        }
    }

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
    strategy_info.current_undelegated_shares = decimal_summation_in_256(
        strategy_info.current_undelegated_shares,
        user_undelegated_shares,
    );

    user_reward_info
        .undelegation_records
        .push(UserUndelegationRecord {
            // there may multiple records with the same id. when withdrawing, use amount as the tie-breaker in such a case.
            id: _env.block.time,
            est_release_time: _env
                .block
                .time
                .plus_seconds(strategy_info.unbonding_period + strategy_info.unbonding_buffer),
            amount,
            shares: user_undelegated_shares,
            strategy_name: strategy_name.clone(),
            undelegation_batch_id: strategy_info.undelegation_batch_id_pointer,
        });

    STRATEGY_MAP.save(deps.storage, &*strategy_name, &strategy_info)?;
    USER_REWARD_INFO_MAP.save(deps.storage, &user_addr, &user_reward_info)?;

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

    let cw20_token_contracts: Cw20TokenContractsInfo;
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
    if let Some(user_reward_info_map) = USER_REWARD_INFO_MAP
        .may_load(deps.storage, &user_addr)
        .unwrap()
    {
        user_reward_info = user_reward_info_map;
    } else {
        return Err(ContractError::UserRewardInfoDoesNotExist {});
    }

    allocate_user_airdrops_across_strategies(deps.storage, &mut user_reward_info);

    let mut messages: Vec<WasmMsg> = vec![];
    let mut transfered_airdrops: Vec<Coin> = vec![];
    // iterate thru all airdrops and transfer ownership to them to the user
    user_reward_info
        .pending_airdrops
        .iter_mut()
        .for_each(|user_airdrop| {
            let airdrop_denom = user_airdrop.denom.clone();
            let airdrop_amount = user_airdrop.amount;

            let cw20_token_contracts = CW20_TOKEN_CONTRACTS_REGISTRY
                .may_load(deps.storage, airdrop_denom.clone())
                .unwrap();

            if cw20_token_contracts.is_none() || airdrop_amount.is_zero() {
                return;
            }

            messages.push(WasmMsg::Execute {
                contract_addr: String::from(cw20_token_contracts.unwrap().cw20_token_contract),
                msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                    recipient: String::from(user_addr.clone()),
                    amount: airdrop_amount,
                })
                .unwrap(),
                funds: vec![],
            });

            // the airdrop is completely transferred back to the user
            user_airdrop.amount = Uint128::zero();

            transfered_airdrops.push(Coin::new(airdrop_amount.u128(), airdrop_denom));
        });

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.total_accumulated_airdrops = merge_coin_vector(
            state.total_accumulated_airdrops,
            CoinVecOp {
                fund: transfered_airdrops,
                operation: Operation::Sub,
            },
        );
        Ok(state)
    })?;

    USER_REWARD_INFO_MAP.save(deps.storage, &user_addr, &user_reward_info)?;

    Ok(Response::new().add_messages(messages))
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

    let mut user_reward_info: UserRewardInfo;
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
    let undelegation_record_index: usize;
    if let Some(i) = (0..user_reward_info.undelegation_records.len()).find(|&i| {
        user_reward_info.undelegation_records[i]
            .id
            .eq(&undelegation_timestamp)
            && user_reward_info.undelegation_records[i]
                .strategy_name
                .eq(&strategy_name)
            && user_reward_info.undelegation_records[i].amount.eq(&amount)
    }) {
        user_undelegation_record = &user_reward_info.undelegation_records[i];
        undelegation_record_index = i;
    } else {
        return Err(ContractError::UndelegationRecordNotFound {});
    }

    if user_undelegation_record
        .est_release_time
        .gt(&env.block.time)
    {
        return Err(ContractError::UndelegationInUnbondingPeriod {});
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

    if !undelegation_batch.slashing_checked {
        return Err(ContractError::SlashingNotChecked {});
    }

    let withdrawable_amount = uint128_from_decimal(decimal_division_in_256(
        user_undelegation_record.shares,
        undelegation_batch.undelegation_s_t_ratio,
    ));
    // now apply unbonding slashing to the withdrawable amount
    let effective_withdrawable_amount = uint128_from_decimal(decimal_multiplication_in_256(
        Decimal::from_ratio(withdrawable_amount, 1_u128),
        undelegation_batch.unbonding_slashing_ratio,
    ));

    // remove undelegation record
    user_reward_info
        .undelegation_records
        .remove(undelegation_record_index);

    USER_REWARD_INFO_MAP.save(deps.storage, &user_addr, &user_reward_info)?;

    Ok(Response::new().add_message(BankMsg::Send {
        to_address: String::from(user_addr),
        amount: vec![Coin::new(
            effective_withdrawable_amount.u128(),
            state.scc_denom,
        )],
    }))
}

pub fn try_register_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_name: String,
    sic_contract_address: Addr,
    unbonding_period: Option<u64>,
    unbonding_buffer: Option<u64>,
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
        &StrategyInfo::new(
            strategy_name,
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
    user_portfolio: Vec<UserStrategyPortfolio>,
) -> Result<Response, ContractError> {
    let user_addr = info.sender;

    // check if the entire portfolio deposit fraction is less than 1. else abort the tx
    // do bunch of sanity checks
    let mut total_deposit_fraction = Decimal::zero();
    for portfolio in &user_portfolio {
        let strategy_name = portfolio.strategy_name.clone();

        if STRATEGY_MAP
            .may_load(deps.storage, &*strategy_name)
            .unwrap()
            .is_none()
        {
            return Err(ContractError::StrategyInfoDoesNotExist(strategy_name));
        }

        total_deposit_fraction =
            decimal_summation_in_256(total_deposit_fraction, portfolio.deposit_fraction);
    }

    if total_deposit_fraction > Decimal::one() {
        return Err(ContractError::InvalidPortfolioDepositFraction {});
    }

    USER_REWARD_INFO_MAP.update(
        deps.storage,
        &user_addr,
        |reward_info_opt| -> Result<_, ContractError> {
            let mut user_reward_info = reward_info_opt.unwrap_or_else(UserRewardInfo::default);
            user_reward_info.user_portfolio = user_portfolio;
            Ok(user_reward_info)
        },
    )?;

    Ok(Response::default())
}

pub fn try_update_user_rewards(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    update_user_rewards_requests: Vec<UpdateUserRewardsRequest>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    if update_user_rewards_requests.is_empty() {
        return Ok(Response::new().add_attribute("zero_update_user_rewards_requests", "1"));
    }

    let mut failed_strategies: Vec<String> = vec![];
    let mut failed_sics: Vec<String> = vec![];
    let mut inactive_strategies: Vec<String> = vec![];
    let mut users_with_zero_deposits: Vec<String> = vec![];
    // cache for the S/T ratio per strategy. We fetch it once and cache it up.
    let mut strategy_to_s_t_ratio: HashMap<String, Decimal> = HashMap::new();
    let mut strategy_to_funds: HashMap<Addr, Uint128> = HashMap::new();
    let mut total_surplus_rewards: Uint128 = Uint128::zero();
    let mut messages: Vec<WasmMsg> = vec![];
    for update_user_rewards_request in update_user_rewards_requests {
        let user_addr = update_user_rewards_request.user;
        let funds = update_user_rewards_request.funds;
        let strategy_opt = update_user_rewards_request.strategy_name;

        if funds.is_zero() {
            users_with_zero_deposits.push(user_addr.to_string());
            continue;
        }

        let mut user_reward_info = USER_REWARD_INFO_MAP
            .may_load(deps.storage, &user_addr)
            .unwrap()
            .unwrap_or_else(|| UserRewardInfo::new(config.default_user_portfolio.clone()));

        // this is the amount to be split per strategy
        let strategy_split: Vec<(String, Uint128)>;
        // surplus is the amount of rewards which does not go into any strategy and just sits in SCC.
        // surplus is basically retain rewards
        let mut surplus: Uint128 = Uint128::zero();

        match strategy_opt {
            None => {
                let strategy_split_with_surplus = get_strategy_split(&user_reward_info, funds);
                strategy_split = strategy_split_with_surplus.0;
                surplus = strategy_split_with_surplus.1;
            }
            Some(strategy_name) => {
                strategy_split = vec![(strategy_name, funds)];
            }
        }

        // add the surplus to the pending rewards
        user_reward_info.pending_rewards = user_reward_info
            .pending_rewards
            .checked_add(surplus)
            .unwrap();
        total_surplus_rewards = total_surplus_rewards.checked_add(surplus).unwrap();

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
                match get_strategy_shares_per_token_ratio(deps.querier, &strategy_info) {
                    Ok(result) => {
                        current_strategy_shares_per_token_ratio = result;
                    }
                    Err(_) => {
                        failed_sics.push(strategy_info.name);
                        continue;
                    }
                }
                strategy_info.shares_per_token_ratio = current_strategy_shares_per_token_ratio;
                strategy_to_s_t_ratio.insert(
                    strategy_name.clone(),
                    current_strategy_shares_per_token_ratio,
                );
            } else {
                current_strategy_shares_per_token_ratio =
                    *strategy_to_s_t_ratio.get(&strategy_name).unwrap();
            }

            // println!("shares_per_token_ratio is {:?}", current_strategy_shares_per_token_ratio);
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
        state.rewards_in_scc = state
            .rewards_in_scc
            .checked_add(total_surplus_rewards)
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
        )
        .add_attribute("failed_sics", failed_sics.join(",")))
}

// This assumes that the validator contract will transfer ownership of the airdrops
// from the validator contract to the SCC contract.
pub fn try_update_user_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    update_user_airdrops_requests: Vec<UpdateUserAirdropsRequest>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    // check for manager?
    let state = STATE.load(deps.storage)?;
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
        let mut user_reward_info = UserRewardInfo::new(config.default_user_portfolio.clone());
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
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::GetStrategyInfo { strategy_name } => {
            to_binary(&query_strategy_info(deps, strategy_name)?)
        }
        QueryMsg::GetUserRewardInfo { user } => to_binary(&query_user_reward_info(deps, user)?),
        QueryMsg::GetUndelegationBatchInfo {
            strategy_name,
            batch_id,
        } => to_binary(&query_undelegation_batch_info(
            deps,
            strategy_name,
            batch_id,
        )?),
        QueryMsg::GetStrategiesList {} => to_binary(&query_strategies_list(deps)?),
        QueryMsg::GetAllStrategies {} => to_binary(&query_get_all_strategies(deps)?),
        QueryMsg::GetUser { user } => to_binary(&query_user(deps, user)?),
    }
}

fn query_user(deps: Deps, user: Addr) -> StdResult<GetUserResponse> {
    let user_reward_info_opt = USER_REWARD_INFO_MAP.may_load(deps.storage, &user).unwrap();
    if user_reward_info_opt.is_none() {
        return Ok(GetUserResponse { user: None });
    }

    let user_reward_info = user_reward_info_opt.unwrap();

    let mut user_strategy_query: UserRewardInfoQuery = UserRewardInfoQuery {
        total_airdrops: vec![],
        retained_rewards: user_reward_info.pending_rewards,
        undelegation_records: user_reward_info.undelegation_records,
        user_strategy_info: vec![],
        user_portfolio: user_reward_info.user_portfolio,
    };

    let mut user_strategy_info: Vec<UserStrategyQueryInfo> = vec![];
    let mut total_airdrops: Vec<Coin> = vec![];
    for user_strategy in user_reward_info.strategies {
        let strategy_name = user_strategy.strategy_name.clone();

        let strategy_info: StrategyInfo;
        if let Some(si) = STRATEGY_MAP
            .may_load(deps.storage, &*strategy_name)
            .unwrap()
        {
            strategy_info = si;
        } else {
            continue;
        }

        let user_shares = user_strategy.shares;
        let user_airdrop_pointer = &user_strategy.airdrop_pointer;

        // if we fail to fetch the tokens from the strategy, just default it to 10
        let mut strategy_s_t_ratio: Decimal = Decimal::from_ratio(10_u128, 1_u128);
        if let Ok(result) = get_strategy_shares_per_token_ratio(deps.querier, &strategy_info) {
            strategy_s_t_ratio = result;
        }
        let user_rewards = get_staked_amount(strategy_s_t_ratio, user_shares);
        let user_airdrops = get_user_airdrops(
            &strategy_info.global_airdrop_pointer,
            user_airdrop_pointer,
            user_shares,
        )
        .unwrap_or_default();

        user_strategy_info.push(UserStrategyQueryInfo {
            strategy_name,
            total_rewards: user_rewards,
            total_airdrops: user_airdrops.clone(),
        });

        total_airdrops = merge_coin_vector(
            total_airdrops,
            CoinVecOp {
                fund: user_airdrops,
                operation: Operation::Add,
            },
        );
    }

    user_strategy_query.total_airdrops = total_airdrops;
    user_strategy_query.user_strategy_info = user_strategy_info;

    Ok(GetUserResponse {
        user: Some(user_strategy_query),
    })
}

fn query_get_all_strategies(deps: Deps) -> StdResult<GetAllStrategiesResponse> {
    let state = STATE.load(deps.storage)?;

    // seed with retain-rewards
    let mut all_strategies_info: Vec<StrategyInfoQuery> = vec![StrategyInfoQuery {
        strategy_name: "retain-rewards".to_string(),
        total_rewards: state.rewards_in_scc,
        rewards_in_undelegation: Uint128::zero(),
        is_active: true,
        total_airdrops_accumulated: vec![],
        unbonding_period: 0,
        unbonding_buffer: 0,
        // special case
        sic_contract_address: Addr::unchecked(""),
    }];

    STRATEGY_MAP
        .range(deps.storage, None, None, Order::Ascending)
        .for_each(|x_wrapped| {
            if x_wrapped.is_err() {
                return;
            }

            let x = x_wrapped.unwrap();
            let strategy_name = String::from_utf8(x.0).unwrap_or_default();
            let strategy_info = x.1;

            let mut strategy_s_t_ratio: Decimal = Decimal::from_ratio(10_u128, 1_u128);
            if let Ok(result) = get_strategy_shares_per_token_ratio(deps.querier, &strategy_info) {
                strategy_s_t_ratio = result;
            }
            let total_strategy_tokens =
                get_staked_amount(strategy_s_t_ratio, strategy_info.total_shares);
            let total_tokens_in_undelegation =
                get_staked_amount(strategy_s_t_ratio, strategy_info.current_undelegated_shares);

            all_strategies_info.push(StrategyInfoQuery {
                strategy_name,
                total_rewards: total_strategy_tokens,
                rewards_in_undelegation: total_tokens_in_undelegation,
                is_active: strategy_info.is_active,
                total_airdrops_accumulated: strategy_info.total_airdrops_accumulated,
                unbonding_period: strategy_info.unbonding_period,
                unbonding_buffer: strategy_info.unbonding_buffer,
                sic_contract_address: strategy_info.sic_contract_address,
            });
        });

    Ok(GetAllStrategiesResponse {
        all_strategies: Some(all_strategies_info),
    })
}

fn query_undelegation_batch_info(
    deps: Deps,
    strategy_name: String,
    batch_id: u64,
) -> StdResult<GetUndelegationBatchInfoResponse> {
    let undelegation_batch_info =
        UNDELEGATION_BATCH_MAP.may_load(deps.storage, (U64Key::new(batch_id), &*strategy_name))?;
    Ok(GetUndelegationBatchInfoResponse {
        undelegation_batch_info,
    })
}

fn query_strategies_list(deps: Deps) -> StdResult<GetStrategiesListResponse> {
    let mut strategies_list: Vec<String> = vec![];
    STRATEGY_MAP
        .keys(deps.storage, None, None, Order::Ascending)
        .for_each(|x| {
            strategies_list.push(String::from_utf8(x).unwrap_or_default());
        });

    Ok(GetStrategiesListResponse {
        strategies_list: Some(strategies_list),
    })
}

fn query_config(deps: Deps) -> StdResult<GetConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(GetConfigResponse {
        config: Some(config),
    })
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
