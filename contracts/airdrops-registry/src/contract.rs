#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetAirdropContractsResponse, GetConfigResponse, InstantiateMsg, MigrateMsg,
    QueryMsg, TmpManagerStoreResponse,
};
use crate::state::{
    AirdropRegistryInfo, Config, TmpManagerStore, AIRDROP_REGISTRY, CONFIG, TMP_MANAGER_STORE,
};
use cw2::set_contract_version;

const CONTRACT_NAME: &str = "airdrops-registry";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let config = Config {
        manager: info.sender,
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
        ExecuteMsg::UpdateAirdropRegistry {
            airdrop_token: airdrop_token_str,
            airdrop_contract: airdrop_contract_str,
            cw20_contract: cw20_contract_str,
        } => update_airdrop_registry(
            deps,
            info,
            airdrop_token_str,
            airdrop_contract_str,
            cw20_contract_str,
        ),
        ExecuteMsg::SetManager { manager } => set_manager(deps, info, _env, manager),
        ExecuteMsg::AcceptManager {} => accept_manager(deps, info, _env),
    }
}

pub fn set_manager(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    manager: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    TMP_MANAGER_STORE.save(deps.storage, &TmpManagerStore { manager: manager })?;

    Ok(Response::default())
}

pub fn accept_manager(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    let tmp_manager_store =
        if let Some(tmp_manager_store) = TMP_MANAGER_STORE.may_load(deps.storage)? {
            tmp_manager_store
        } else {
            return Err(ContractError::TmpManagerStoreEmpty {});
        };

    let manager = deps.api.addr_validate(tmp_manager_store.manager.as_str())?;
    if info.sender != manager {
        return Err(ContractError::Unauthorized {});
    }

    config.manager = deps.api.addr_validate(tmp_manager_store.manager.as_str())?;
    TMP_MANAGER_STORE.remove(deps.storage);

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

pub fn update_airdrop_registry(
    deps: DepsMut,
    info: MessageInfo,
    airdrop_token_str: String,
    airdrop_contract_str: String,
    cw20_contract_str: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    if airdrop_token_str.is_empty() {
        return Err(ContractError::TokenEmpty {});
    }

    let airdrop_token = airdrop_token_str;
    let airdrop_contract = deps.api.addr_validate(airdrop_contract_str.as_str())?;
    let cw20_contract = deps.api.addr_validate(cw20_contract_str.as_str())?;
    AIRDROP_REGISTRY.save(
        deps.storage,
        airdrop_token.clone(),
        &AirdropRegistryInfo {
            token: airdrop_token,
            airdrop_contract,
            cw20_contract,
        },
    )?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::GetAirdropContracts { token } => {
            to_binary(&query_airdrop_contracts(deps, token)?)
        }
        QueryMsg::TmpManagerStore {} => to_binary(&query_tmp_manager_store(deps)?),
    }
}

pub fn query_tmp_manager_store(deps: Deps) -> StdResult<TmpManagerStoreResponse> {
    let tmp_manager_store = TMP_MANAGER_STORE.may_load(deps.storage)?;
    Ok(TmpManagerStoreResponse { tmp_manager_store })
}

fn query_config(deps: Deps) -> StdResult<GetConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(GetConfigResponse { config })
}

fn query_airdrop_contracts(deps: Deps, token: String) -> StdResult<GetAirdropContractsResponse> {
    let contracts = AIRDROP_REGISTRY.may_load(deps.storage, token)?;
    Ok(GetAirdropContractsResponse { contracts })
}
