#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Attribute, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    Fraction, MessageInfo, Response, StdError, StdResult, Timestamp, Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::helpers::{get_sic_total_tokens, get_user_strategy_data, strategy_supports_airdrops};
use crate::msg::{
    ExecuteMsg, GetStateResponse, InstantiateMsg, QueryMsg, UpdateUserAirdropsRequest,
    UpdateUserRewardsRequest,
};
use crate::state::{
    Cw20TokenContractsInfo, DecCoin, State, StrategyInfo, StrategyMetadata, UserRewardInfo,
    UserStrategyInfo, CW20_TOKEN_CONTRACTS_REGISTRY, STATE, STRATEGY_INFO_MAP,
    STRATEGY_METADATA_MAP, USER_REWARD_INFO_MAP,
};
use crate::user::get_user_airdrops;
use crate::utils::{
    decimal_division_in_256, decimal_multiplication_in_256, decimal_summation_in_256,
    merge_coin_vector, merge_dec_coin_vector, CoinVecOp, DecCoinVecOp, Operation,
};
use sic_base::msg::{ExecuteMsg as sic_execute_msg, QueryMsg as sic_query_msg};
use std::borrow::Borrow;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        manager: info.sender.clone(),
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
            supported_airdrops,
        } => try_register_strategy(
            deps,
            _env,
            info,
            strategy_id,
            unbonding_period,
            sic_contract_address,
            supported_airdrops,
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
        ExecuteMsg::RegsiterCW20Contract {
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
    if let Some(cw20_token_contracts_mapping) = CW20_TOKEN_CONTRACTS_REGISTRY
        .may_load(deps.storage, denom.clone())
        .unwrap()
    {
        cw20_token_contracts = cw20_token_contracts_mapping;
    } else {
        return Err(ContractError::AirdropNotRegistered {});
    }

    let sic_address: Addr;
    if let Some(strategy_info) = STRATEGY_INFO_MAP
        .may_load(deps.storage, denom.clone())
        .unwrap()
    {
        // while registering the strategy, we need to update the airdrops the strategy supports.
        if !strategy_supports_airdrops(&strategy_info, Some(denom.clone())) {
            return Err(ContractError::StrategyDoesNotSupportAirdrop {});
        }
        sic_address = strategy_info.sic_contract_address;
    } else {
        return Err(ContractError::StrategyInfoDoesNotExist {});
    }

    let mut strategy_metadata: StrategyMetadata;
    if let Some(strategy_metadata_mapping) = STRATEGY_METADATA_MAP
        .may_load(deps.storage, denom.clone())
        .unwrap()
    {
        strategy_metadata = strategy_metadata_mapping;
    } else {
        return Err(ContractError::StrategyMetadataDoesNotExist {});
    }

    let total_shares = strategy_metadata.total_shares;

    strategy_metadata.total_airdrops_accumulated = merge_coin_vector(
        strategy_metadata.total_airdrops_accumulated,
        CoinVecOp {
            fund: vec![Coin::new(amount.u128(), denom.clone())],
            operation: Operation::Add,
        },
    );

    strategy_metadata.global_airdrop_pointer = merge_dec_coin_vector(
        &strategy_metadata.global_airdrop_pointer,
        DecCoinVecOp {
            fund: vec![DecCoin {
                amount: decimal_multiplication_in_256(
                    Decimal::from_ratio(amount, 1_u128),
                    total_shares.inv().unwrap(),
                ),
                denom: denom.clone(),
            }],
            operation: Operation::Add,
        },
    );

    STRATEGY_METADATA_MAP.save(deps.storage, strategy_id, &strategy_metadata);

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: String::from(sic_address),
        msg: to_binary(&sic_execute_msg::ClaimAirdrops {
            airdrop_token_contract: cw20_token_contracts.airdrop_contract,
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

    let user_reward_info: UserRewardInfo;
    match USER_REWARD_INFO_MAP
        .may_load(deps.storage, &user_addr)
        .unwrap()
    {
        None => return Err(ContractError::UserRewardInfoDoesNotExist {}),
        Some(user_reward_info_mapping) => {
            user_reward_info = user_reward_info_mapping;
        }
    }

    let mut messages: Vec<WasmMsg> = vec![];
    let mut logs: Vec<Attribute> = vec![];
    let user_airdrops = user_reward_info.pending_airdrops;
    for user_airdrop in user_airdrops {
        let airdrop_denom = user_airdrop.denom;
        let airdrop_amount = user_airdrop.amount;

        let cw20_token_contracts = CW20_TOKEN_CONTRACTS_REGISTRY
            .may_load(deps.storage, airdrop_denom.clone())
            .unwrap();
        if cw20_token_contracts.is_none() {
            logs.push(attr("failed_to_get_contract", airdrop_denom));
            continue;
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
    }

    Ok(Response::new().add_attributes(logs).add_messages(messages))
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
    supported_airdrops: Vec<String>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    if STRATEGY_INFO_MAP
        .may_load(deps.storage, strategy_id.clone())
        .unwrap()
        .is_some()
    {
        return Err(ContractError::StrategyInfoAlreadyExists {});
    }

    if STRATEGY_METADATA_MAP
        .may_load(deps.storage, strategy_id.clone())
        .unwrap()
        .is_some()
    {
        return Err(ContractError::StrategyMetadataAlreadyExists {});
    }

    STRATEGY_INFO_MAP.save(
        deps.storage,
        strategy_id.clone(),
        &StrategyInfo {
            name: strategy_id.clone(),
            sic_contract_address,
            unbonding_period,
            supported_airdrops,
            is_active: false,
        },
    )?;
    STRATEGY_METADATA_MAP.save(
        deps.storage,
        strategy_id.clone(),
        &StrategyMetadata {
            name: strategy_id,
            total_shares: Decimal::zero(),
            global_airdrop_pointer: vec![],
            total_airdrops_accumulated: vec![],
            shares_per_token_ratio: Decimal::zero(),
            current_unprocessed_undelegations: Uint128::zero(),
        },
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

    STRATEGY_INFO_MAP.update(
        deps.storage,
        strategy_id,
        |strategy_info_option| -> Result<_, ContractError> {
            if strategy_info_option.is_none() {
                return Err(ContractError::StrategyInfoDoesNotExist {});
            }

            let mut strategy_info = strategy_info_option.unwrap();
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

    STRATEGY_INFO_MAP.update(
        deps.storage,
        strategy_id,
        |strategy_info_option| -> Result<_, ContractError> {
            if strategy_info_option.is_none() {
                return Err(ContractError::StrategyInfoDoesNotExist {});
            }

            let mut strategy_info = strategy_info_option.unwrap();
            strategy_info.is_active = false;
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

    STRATEGY_INFO_MAP.remove(deps.storage, strategy_id);

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
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    if update_user_rewards_requests.is_empty() {
        return Ok(Response::default());
    }

    let mut messages: Vec<WasmMsg> = vec![];
    // iterate thru all requests
    for user_request in update_user_rewards_requests {
        let user_strategy = user_request.strategy_id;
        let user_amount = user_request.rewards;
        let user_addr = user_request.user;

        let mut strategy_info: StrategyInfo = StrategyInfo::default();
        if let Some(strategy_info_mapping) = STRATEGY_INFO_MAP
            .may_load(deps.storage, user_strategy.clone())
            .unwrap()
        {
            strategy_info = strategy_info_mapping;
        } else {
            // TODO: bchain99 - log something out here
            continue;
        }

        let mut strategy_metadata: StrategyMetadata = StrategyMetadata::default();
        if let Some(strategy_metadata_mapping) = STRATEGY_METADATA_MAP
            .may_load(deps.storage, user_strategy.clone())
            .unwrap()
        {
            strategy_metadata = strategy_metadata_mapping;
        } else {
            // TODO: bchain99 - log something out here
            continue;
        }

        // fetch the total tokens from the SIC contract and update the S/T ratio for the strategy
        let total_tokens = get_sic_total_tokens(deps.querier, &strategy_info.sic_contract_address)
            .total_tokens
            .unwrap_or_else(|| Uint128::zero());
        let mut shares_per_token_ratio = Decimal::from_ratio(100_000_000_u128, 1_u128);
        if !total_tokens.is_zero() {
            shares_per_token_ratio = decimal_division_in_256(
                strategy_metadata.total_shares,
                Decimal::from_ratio(total_tokens, 1_u128),
            );
        }
        strategy_metadata.shares_per_token_ratio = shares_per_token_ratio;

        let mut user_reward_info = UserRewardInfo::new();
        if let Some(user_reward_info_mapping) = USER_REWARD_INFO_MAP
            .may_load(deps.storage, &user_addr)
            .unwrap()
        {
            user_reward_info = user_reward_info_mapping;
        }

        // update the user airdrop pointer and allocate the user pending airdrops for each strategy
        let mut user_strategy_data: UserStrategyInfo;
        if let Some(user_strategy_data_mapping) =
            get_user_strategy_data(user_reward_info.strategies.clone(), user_strategy.clone())
        {
            user_strategy_data = user_strategy_data_mapping;
        } else {
            user_strategy_data = UserStrategyInfo::new(
                user_strategy.clone(),
                strategy_metadata.global_airdrop_pointer.clone(),
            );
        }

        // update the user_airdrops
        let mut user_airdrops: Vec<Coin> = vec![];
        if strategy_supports_airdrops(&strategy_info, None) {
            if let Some(new_user_airdrops) = get_user_airdrops(
                strategy_metadata.global_airdrop_pointer.clone(),
                user_strategy_data.airdrop_pointer,
                user_strategy_data.shares,
            ) {
                user_airdrops = new_user_airdrops;
            }
        }
        user_strategy_data.airdrop_pointer = strategy_metadata.global_airdrop_pointer.clone();
        user_reward_info.pending_airdrops = merge_coin_vector(
            user_reward_info.pending_airdrops,
            CoinVecOp {
                fund: user_airdrops,
                operation: Operation::Add,
            },
        );

        // update user shares based on the S/T ratio
        let user_shares = decimal_multiplication_in_256(
            shares_per_token_ratio,
            Decimal::from_ratio(user_amount, 1_u128),
        );
        user_strategy_data.shares =
            decimal_summation_in_256(user_strategy_data.shares, user_shares);
        // update total strategy shares by adding up the user_shares
        strategy_metadata.total_shares =
            decimal_summation_in_256(strategy_metadata.total_shares, user_shares);

        // do statewise book-keeping like adding up accumulated_rewards
        STATE.update(deps.storage, |mut state| -> StdResult<_> {
            state.total_accumulated_rewards = state
                .total_accumulated_rewards
                .checked_add(user_amount)
                .unwrap();
            Ok(state)
        });

        // send the rewards to sic
        messages.push(WasmMsg::Execute {
            contract_addr: String::from(strategy_info.sic_contract_address),
            msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
            funds: vec![Coin::new(user_amount.u128(), state.scc_denom.clone())],
        });

        // save up the states
        STRATEGY_METADATA_MAP.save(deps.storage, user_strategy.clone(), &strategy_metadata);

        user_reward_info.strategies.push(user_strategy_data);
        USER_REWARD_INFO_MAP.save(deps.storage, &user_addr, &user_reward_info);
    }

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

        USER_REWARD_INFO_MAP.save(deps.storage, &user, &user_reward_info);
    }

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.total_accumulated_airdrops = total_scc_airdrops;
        Ok(state)
    });

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetState {} => to_binary(&query_state(deps)?),
    }
}

fn query_state(deps: Deps) -> StdResult<GetStateResponse> {
    let state = STATE.load(deps.storage)?;
    Ok(GetStateResponse {
        state: Option::from(state),
    })
}
