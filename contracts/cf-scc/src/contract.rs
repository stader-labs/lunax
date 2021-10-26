#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
    SubMsg, Uint128,
};

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetConfigResponse, GetUserRewardResponse, InstantiateMsg, MigrateMsg, QueryMsg,
    UpdateUserAirdropsRequest, UpdateUserRewardsRequest,
};
use crate::state::{Config, UserInfo, CONFIG, USER_REWARDS};
use cw2::set_contract_version;
use stader_utils::coin_utils::{merge_coin_vector, CoinVecOp, Operation};

const CONTRACT_NAME: &str = "cf-scc";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let config = Config {
        manager: info.sender,
        delegator_contract: msg.delegator_contract,
    };
    CONFIG.save(deps.storage, &config)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new().add_attribute("method", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateUserRewards {
            update_user_rewards_requests,
        } => update_user_rewards(deps, _env, info, update_user_rewards_requests),
        ExecuteMsg::UpdateUserAirdrops {
            update_user_airdrops_requests,
        } => update_user_airdrops(deps, _env, info, update_user_airdrops_requests),
        ExecuteMsg::WithdrawFunds {
            withdraw_address,
            amount,
            denom,
        } => withdraw_funds(deps, _env, info, withdraw_address, amount, denom),
    }
}

pub fn update_user_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    update_user_airdrops_requests: Vec<UpdateUserAirdropsRequest>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.delegator_contract {
        return Err(ContractError::Unauthorized {});
    }

    if update_user_airdrops_requests.is_empty() {
        return Ok(Response::new().add_attribute("zero_user_airdrops_requests", "1"));
    }

    for user_request in update_user_airdrops_requests {
        let user_addr = user_request.user;
        let user_airdrops = user_request.pool_airdrops;

        USER_REWARDS.update(deps.storage, &user_addr, |user_info_opt| -> StdResult<_> {
            let mut user_info = user_info_opt.unwrap_or(UserInfo::new());
            user_info.airdrops = merge_coin_vector(
                user_info.airdrops.as_slice(),
                CoinVecOp {
                    fund: user_airdrops,
                    operation: Operation::Add,
                },
            );
            Ok(user_info)
        })?;
    }

    Ok(Response::default())
}

// Can only be called delegator contract
pub fn update_user_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    update_user_rewards_requests: Vec<UpdateUserRewardsRequest>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.delegator_contract {
        return Err(ContractError::Unauthorized {});
    }

    if update_user_rewards_requests.is_empty() {
        return Ok(Response::new().add_attribute("zero_user_rewards_requests", "1"));
    }

    for user_request in update_user_rewards_requests {
        let user_addr = user_request.user;
        let user_balance = user_request.funds;

        USER_REWARDS.update(deps.storage, &user_addr, |user_info_opt| -> StdResult<_> {
            let mut user_info = user_info_opt.unwrap_or(UserInfo::new());
            user_info.amount = user_info.amount.checked_add(user_balance).unwrap();
            Ok(user_info)
        })?;
    }

    Ok(Response::default())
}

pub fn withdraw_funds(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    withdraw_address: Addr,
    amount: Uint128,
    denom: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }
    if amount == Uint128::zero() {
        return Err(ContractError::AmountZero {});
    }

    let msg = SubMsg::new(BankMsg::Send {
        to_address: withdraw_address.to_string(),
        amount: vec![Coin::new(amount.u128(), denom)],
    });

    Ok(Response::new().add_submessage(msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::GetUserRewardInfo { user } => to_binary(&query_user_reward_info(deps, user)?),
    }
}

fn query_config(deps: Deps) -> StdResult<GetConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(GetConfigResponse { config })
}

fn query_user_reward_info(deps: Deps, user: Addr) -> StdResult<GetUserRewardResponse> {
    let user_reward_info = USER_REWARDS.may_load(deps.storage, &user)?;
    Ok(GetUserRewardResponse { user_reward_info })
}
