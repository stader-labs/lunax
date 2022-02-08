#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetConfigResponse, InstantiateMsg, MigrateMsg, QueryMsg, TmpManagerStoreResponse,
};

use crate::state::{Config, TmpManagerStore, CONFIG, TMP_MANAGER_STORE};
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
        manager: info.sender,
        reward_denom: "uluna".to_string(),
        staking_contract: deps.api.addr_validate(msg.staking_contract.as_str())?,
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
        ExecuteMsg::UpdateConfig {
            staking_contract: pools_contract,
        } => update_config(deps, info, env, pools_contract),
        ExecuteMsg::SetManager { manager } => set_manager(deps, info, env, manager),
        ExecuteMsg::AcceptManager {} => accept_manager(deps, info, env),
    }
}

pub fn set_manager(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    manager: String,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    TMP_MANAGER_STORE.save(deps.storage, &TmpManagerStore { manager })?;

    Ok(Response::default())
}

pub fn accept_manager(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    let tmp_manager_store =
        if let Some(tmp_manager_store) = TMP_MANAGER_STORE.may_load(deps.storage)? {
            tmp_manager_store
        } else {
            return Err(ContractError::TmpManagerStoreEmpty {});
        };

    config.manager = deps.api.addr_validate(tmp_manager_store.manager.as_str())?;

    TMP_MANAGER_STORE.remove(deps.storage);
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

// Swaps all rewards accrued in this contract to reward denom - luna.
pub fn swap(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.staking_contract {
        return Err(ContractError::Unauthorized {});
    }

    let mut messages = vec![];
    let total_rewards = deps.querier.query_all_balances(env.contract.address)?;
    // let denoms: Vec<String> = total_rewards
    //     .iter()
    //     .map(|item| item.denom.clone())
    //     .collect();

    // got this list from https://fcd.terra.dev/v1/txs/gas_prices. These are native Terra
    let denoms = vec![
        "uluna".to_string(),
        "usdr".to_string(),
        "uusd".to_string(),
        "ukrw".to_string(),
        "umnt".to_string(),
        "ueur".to_string(),
        "ucny".to_string(),
        "ujpy".to_string(),
        "ugbp".to_string(),
        "uinr".to_string(),
        "ucad".to_string(),
        "uchf".to_string(),
        "uaud".to_string(),
        "usgd".to_string(),
        "uthb".to_string(),
        "usek".to_string(),
        "unok".to_string(),
        "udkk".to_string(),
        "uidr".to_string(),
        "uphp".to_string(),
        "uhkd".to_string(),
    ];

    let mut is_listed = true;
    if query_exchange_rates(&deps, config.reward_denom.clone(), denoms.clone()).is_err() {
        is_listed = false;
    }

    for coin in total_rewards {
        // No need to swap uluna and Don't process rewards that aren't native to Terra
        if coin.denom == config.reward_denom.clone() || !denoms.contains(&coin.denom) {
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

    if info.sender != config.staking_contract {
        return Err(ContractError::Unauthorized {});
    }

    let total_withdrawal_amount = reward_amount.checked_add(protocol_fee).unwrap();
    if deps
        .querier
        .query_balance(env.contract.address, config.reward_denom.clone())?
        .amount
        .lt(&total_withdrawal_amount)
    {
        return Err(ContractError::InSufficientFunds {});
    }
    let mut msgs = vec![];
    if !reward_amount.is_zero() {
        msgs.push(send_funds_msg(
            &reward_withdraw_contract,
            &[Coin::new(reward_amount.u128(), config.reward_denom.clone())],
        ));
    }

    if !protocol_fee.is_zero() {
        msgs.push(send_funds_msg(
            &protocol_fee_contract,
            &[Coin::new(protocol_fee.u128(), config.reward_denom)],
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
        config.staking_contract = deps.api.addr_validate(pools_contract.unwrap().as_str())?;
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::TmpManagerStore {} => to_binary(&query_tmp_manager_store(deps)?),
    }
}

pub fn query_tmp_manager_store(deps: Deps) -> StdResult<TmpManagerStoreResponse> {
    let tmp_manager_store = TMP_MANAGER_STORE.may_load(deps.storage)?;
    Ok(TmpManagerStoreResponse { tmp_manager_store })
}

pub fn query_config(deps: Deps) -> StdResult<GetConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(GetConfigResponse { config })
}
