#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetConfigResponse, InstantiateMsg, MigrateMsg, QueryMsg,
};

use crate::state::{Config, CONFIG};
use cw2::set_contract_version;

use terra_cosmwasm::{create_swap_msg, TerraMsgWrapper, ExchangeRatesResponse, TerraQuerier};
use stader_utils::helpers::send_funds_msg;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, attr, Uint128, Coin, StdResult, Addr, Deps, Binary, to_binary};

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
        pools_contract: msg.pools_contract,
        scc_contract: msg.scc_contract,
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
        ExecuteMsg::Transfer { amount } => transfer(deps, info, env, amount),

        ExecuteMsg::UpdateConfig {
            pools_contract,
            scc_contract,
        } => update_config(
            deps,
            info,
            env,
            pools_contract,
            scc_contract,
        ),
    }
}
// Swaps all rewards accrued in this contract to reward denom - luna.
pub fn swap(deps: DepsMut, info: MessageInfo, env: Env, ) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.pools_contract {
        return Err(ContractError::Unauthorized {});
    }

    let mut messages = vec![];
    let total_rewards = deps.querier.query_all_balances(env.contract.address).unwrap();
    let denoms: Vec<String> = total_rewards.iter().map(|item| item.denom.clone()).collect();

    let mut is_listed = true;
    if query_exchange_rates(&deps, config.reward_denom.clone(), denoms).is_err() {
        is_listed = false;
    }

    for coin in total_rewards {
        if coin.denom == config.reward_denom.clone() {
            continue;
        }
        if is_listed {
            messages.push(create_swap_msg(coin, config.reward_denom.to_string()));
        } else if query_exchange_rates(&deps, config.reward_denom.clone(), vec![coin.denom.clone()])
            .is_ok()
        {
            messages.push(create_swap_msg(coin, config.reward_denom.to_string()));
        }
    }

    let res = Response::new()
        .add_messages(messages)
        .add_attributes(vec![attr("action", "swap")]);

    Ok(res)
}

// Transfers luna to SCC at the behest of SCC contract
pub fn transfer(deps: DepsMut, info: MessageInfo, env: Env, amount: Uint128) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.pools_contract {
        return Err(ContractError::Unauthorized {});
    }

    if amount.is_zero() {
        return Err(ContractError::ZeroAmount {});
    }

    if deps.querier.query_balance(env.contract.address, config.reward_denom.clone()).unwrap().amount.lt(&amount) {
        return Err(ContractError::InSufficientFunds {});
    }

    Ok(Response::new().add_message(
        send_funds_msg(&config.scc_contract, &vec![Coin::new(amount.u128(), config.reward_denom)]))
    )
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
    pools_contract: Option<Addr>,
    scc_contract: Option<Addr>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.pools_contract = pools_contract.unwrap_or(config.pools_contract);
        config.scc_contract = scc_contract.unwrap_or(config.scc_contract);
        Ok(config)
    })?;

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
