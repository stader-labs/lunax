#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
    SubMsg, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetConfigResponse, GetCw20ContractResponse, GetUserRewardResponse, InstantiateMsg,
    MigrateMsg, QueryMsg, UpdateUserAirdropsRequest, UpdateUserRewardsRequest,
};
use crate::state::{Config, UserInfo, CONFIG, CW20_CONTRACTS_MAP, USER_REWARDS};
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
        ExecuteMsg::WithdrawAirdrops {} => withdraw_airdrops(deps, _env, info),
        ExecuteMsg::RegisterCw20Contract {
            token,
            cw20_contract,
        } => register_cw20_contract(deps, _env, info, token, cw20_contract),
        ExecuteMsg::UpdateConfig { delegator_contract }
            => update_config(deps, info, _env, delegator_contract),
    }
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    delegator_contract: Option<Addr>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.delegator_contract = delegator_contract.unwrap_or_else(|| config.delegator_contract.clone());
        Ok(config)
    })?;

    Ok(Response::default())
}

pub fn withdraw_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let user_addr = info.sender;

    let mut user_info = if let Some(user_info) = USER_REWARDS.may_load(deps.storage, &user_addr)? {
        user_info
    } else {
        return Err(ContractError::UserInfoDoesNotExist {});
    };

    let mut messages: Vec<WasmMsg> = vec![];
    for airdrop in user_info.airdrops.iter_mut() {
        let denom = airdrop.denom.as_str();
        let amount = airdrop.amount;
        let cw20_contract =
            if let Some(contract) = CW20_CONTRACTS_MAP.may_load(deps.storage, denom)? {
                contract
            } else {
                return Err(ContractError::Cw20ContractNotRegistered(denom.to_string()));
            };

        if amount.is_zero() {
            continue;
        }

        messages.push(WasmMsg::Execute {
            contract_addr: cw20_contract.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: user_addr.to_string(),
                amount,
            })
            .unwrap(),
            funds: vec![],
        });

        airdrop.amount = Uint128::zero();
    }

    USER_REWARDS.save(deps.storage, &user_addr, &user_info)?;

    Ok(Response::new().add_messages(messages))
}

pub fn register_cw20_contract(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    token: String,
    cw20_contract: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    CW20_CONTRACTS_MAP.save(
        deps.storage,
        token.as_str(),
        &deps.api.addr_validate(cw20_contract.as_str())?,
    )?;

    Ok(Response::default())
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
            let mut user_info = user_info_opt.unwrap_or_else(UserInfo::new);
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
            let mut user_info = user_info_opt.unwrap_or_else(UserInfo::new);
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
        QueryMsg::GetCw20Contract { token } => to_binary(&query_cw20_contract(deps, token)?),
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

fn query_cw20_contract(deps: Deps, token: String) -> StdResult<GetCw20ContractResponse> {
    let cw20_contract = CW20_CONTRACTS_MAP.may_load(deps.storage, token.as_str())?;
    Ok(GetCw20ContractResponse { cw20_contract })
}
