#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, GetConfigResponse, InstantiateMsg, MigrateMsg, QueryMsg};

use crate::state::{Config, CONFIG};
use cw2::set_contract_version;

use cosmwasm_std::{
    attr, to_binary, Addr, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
    Uint128,
};
use stader_utils::helpers::send_funds_msg;
use terra_cosmwasm::{create_swap_msg, ExchangeRatesResponse, TerraMsgWrapper, TerraQuerier};

const CONTRACT_NAME: &str = "reward";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = Config {
        manager: info.sender.clone(),
        reward_denom: msg.reward_denom,
        pools_contract: deps.api.addr_validate(msg.pools_contract.as_str())?,
    };
    CONFIG.save(deps.storage, &config)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(
    _deps: DepsMut,
    _env: Env,
    _msg: MigrateMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    match msg {
        ExecuteMsg::Swap {} => swap(deps, info, env),
        ExecuteMsg::Transfer {
            reward_amount,
            reward_withdraw_contract,
            protocol_fee_amount: protocol_fee,
            protocol_fee_contract,
        } => transfer(
            deps,
            info,
            env,
            reward_amount,
            reward_withdraw_contract,
            protocol_fee,
            protocol_fee_contract,
        ),

        ExecuteMsg::UpdateConfig { pools_contract } => {
            update_config(deps, info, env, pools_contract)
        }
    }
}
// Swaps all rewards accrued in this contract to reward denom - luna.
pub fn swap(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.pools_contract {
        return Err(ContractError::Unauthorized {});
    }

    let mut messages = vec![];
    let total_rewards = deps
        .querier
        .query_all_balances(env.contract.address)
        .unwrap();
    let denoms: Vec<String> = total_rewards
        .iter()
        .map(|item| item.denom.clone())
        .collect();

    let exchange_rates = query_exchange_rates(&deps, config.reward_denom.clone(), denoms)?;
    let known_denoms: Vec<String> = exchange_rates
        .exchange_rates
        .iter()
        .map(|item| item.quote_denom.clone())
        .collect();

    for coin in total_rewards {
        if coin.denom == config.reward_denom.clone() || !known_denoms.contains(&coin.denom) {
            continue;
        }

        messages.push(create_swap_msg(coin, config.reward_denom.to_string()));
    }

    let res = Response::new()
        .add_messages(messages)
        .add_attributes(vec![attr("action", "swap")]);

    Ok(res)
}

// Transfers luna to SCC at the behest of Pools contract
pub fn transfer(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    reward_amount: Uint128,
    reward_withdraw_contract: Addr,
    protocol_fee: Uint128,
    protocol_fee_contract: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.pools_contract {
        return Err(ContractError::Unauthorized {});
    }

    let total_withdrawal_amount = reward_amount.checked_add(protocol_fee).unwrap();
    if deps
        .querier
        .query_balance(env.contract.address, config.reward_denom.clone())
        .unwrap()
        .amount
        .lt(&total_withdrawal_amount)
    {
        return Err(ContractError::InSufficientFunds {});
    }
    let mut msgs = vec![];
    if !reward_amount.is_zero() {
        msgs.push(send_funds_msg(
            &reward_withdraw_contract,
            &vec![Coin::new(reward_amount.u128(), config.reward_denom.clone())],
        ));
    }

    if !protocol_fee.is_zero() {
        msgs.push(send_funds_msg(
            &protocol_fee_contract,
            &vec![Coin::new(protocol_fee.u128(), config.reward_denom)],
        ));
    }
    Ok(Response::new().add_messages(msgs))
}

pub fn query_exchange_rates(
    deps: &DepsMut,
    base_denom: String,
    quote_denoms: Vec<String>,
) -> StdResult<ExchangeRatesResponse> {
    let querier = TerraQuerier::new(&deps.querier);
    let res: ExchangeRatesResponse = querier.query_exchange_rates(base_denom, quote_denoms)?;
    Ok(res)
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    pools_contract: Option<String>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    if pools_contract.is_some() {
        config.pools_contract = deps.api.addr_validate(pools_contract.unwrap().as_str())?;
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<GetConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(GetConfigResponse { config })
}