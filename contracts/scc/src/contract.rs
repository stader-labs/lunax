#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Attribute, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    Fraction, MessageInfo, QuerierResult, Response, StdError, StdResult, Timestamp, Uint128,
    WasmMsg,
};

use crate::error::ContractError;
use crate::helpers::get_strategy_shares_per_token_ratio;
use crate::msg::{
    ExecuteMsg, GetStateResponse, GetStrategyInfoResponse, GetUserRewardInfo, InstantiateMsg,
    QueryMsg, UpdateUserAirdropsRequest, UpdateUserRewardsRequest,
};
use crate::state::{
    Cw20TokenContractsInfo, State, StrategyInfo, UserRewardInfo, UserStrategyInfo,
    CW20_TOKEN_CONTRACTS_REGISTRY, STATE, STRATEGY_MAP, USER_REWARD_INFO_MAP,
};
use crate::user::get_user_airdrops;
use sic_base::msg::{ExecuteMsg as sic_execute_msg, QueryMsg as sic_query_msg};
use stader_utils::coin_utils::{
    decimal_division_in_256, decimal_multiplication_in_256, decimal_summation_in_256,
    get_decimal_from_uint128, merge_coin_vector, merge_dec_coin_vector, CoinVecOp, DecCoin,
    DecCoinVecOp, Operation,
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
            strategy_id,
            unbonding_period,
            sic_contract_address,
        } => try_register_strategy(
            deps,
            _env,
            info,
            strategy_id,
            unbonding_period,
            sic_contract_address,
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
            strategy_id,
        } => try_undelegate_rewards(deps, _env, info, amount, strategy_id),
        ExecuteMsg::ClaimAirdrops {
            strategy_id,
            amount,
            denom,
            claim_msg,
        } => try_claim_airdrops(deps, _env, info, strategy_id, amount, denom, claim_msg),
        ExecuteMsg::WithdrawRewards {
            undelegation_timestamp,
            strategy_id,
        } => try_withdraw_rewards(deps, _env, info, undelegation_timestamp, strategy_id),
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
    }
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

pub fn try_undelegate_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
    strategy_id: String,
) -> Result<Response, ContractError> {
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
    let mut successful_airdrops: Vec<Coin> = vec![];
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
        .add_attribute("airdrops_failed_to_transfer", failed_airdrops.join(","))
        .add_messages(messages))
}

pub fn try_withdraw_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    undelegation_timestamp: Timestamp,
    strategy_id: String,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

pub fn try_register_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_id: String,
    unbonding_period: Option<u64>,
    sic_contract_address: Addr,
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
        &StrategyInfo::new(strategy_id, sic_contract_address, unbonding_period),
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
    if info.sender != state.manager {
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
