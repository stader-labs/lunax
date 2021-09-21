#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response, StdError, StdResult,
    Storage,
};

use crate::error::ContractError;
use crate::msg::{ContractResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{State, CONTRACTS, STATE};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        manager: info.sender.clone(),
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
    let state = STATE.load(deps.storage)?;
    // can only be called by manager
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {});
    }

    match msg {
        ExecuteMsg::AddContract { name, addr } => add_contract(deps, name, addr),
        ExecuteMsg::RemoveContract { name } => remove_contract(deps, name),
    }
}

fn remove_contract(deps: DepsMut, name: String) -> Result<Response, ContractError> {
    CONTRACTS.remove(deps.storage, name.clone());
    Ok(Response::new()
        .add_attribute("method", "remove_contract")
        .add_attribute("name", name))
}

pub fn add_contract(deps: DepsMut, name: String, addr: Addr) -> Result<Response, ContractError> {
    let existing_contract = get_contract_by_name_or_addr(deps.storage, name.clone(), &addr)?;

    if existing_contract.is_some() {
        return Err(ContractError::AlreadyExists {
            contract: existing_contract.unwrap(),
        });
    }

    CONTRACTS.save(deps.storage, name.clone(), &addr)?;

    Ok(Response::new()
        .add_attribute("method", "add_contract")
        .add_attribute("name", name)
        .add_attribute("addr", addr))
}

fn get_contract_by_name_or_addr(
    storage: &dyn Storage,
    name: String,
    addr: &Addr,
) -> Result<Option<ContractResponse>, ContractError> {
    let addr_by_name_search = CONTRACTS.may_load(storage, name.clone())?;
    if addr_by_name_search.is_some() {
        return Ok(Some(ContractResponse {
            name,
            addr: addr_by_name_search.unwrap(),
        }));
    }

    get_contract_by_addr(storage, addr)
}

fn get_contract_by_addr(
    storage: &dyn Storage,
    addr: &Addr,
) -> Result<Option<ContractResponse>, ContractError> {
    let tuple_by_addr_search_opt = CONTRACTS
        .prefix(())
        .range(storage, None, None, Order::Ascending)
        .find(|x| x.is_ok() && x.as_ref().unwrap().1.eq(addr));

    if tuple_by_addr_search_opt.is_none() {
        return Ok(None);
    }

    let (contract_name_in_vecu8, _) = tuple_by_addr_search_opt.unwrap()?;
    return Ok(Some(ContractResponse {
        name: String::from_utf8(contract_name_in_vecu8).unwrap(),
        addr: addr.clone(),
    }));
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetAllContracts {} => to_binary(&query_contracts(deps)?),
        QueryMsg::GetContractByAddr { addr } => to_binary(&query_contract_by_addr(deps, addr)?),
        QueryMsg::GetContractByName { name } => to_binary(&query_contract_by_name(deps, name)?),
    }
}

fn query_contract_by_name(deps: Deps, name: String) -> StdResult<ContractResponse> {
    let addr_by_name_search_opt = CONTRACTS.may_load(deps.storage, name.clone())?;

    match addr_by_name_search_opt {
        Some(addr_by_name_search) => Ok(ContractResponse {
            name,
            addr: addr_by_name_search,
        }),
        None => Err(StdError::GenericErr {
            msg: "Contract not found".to_string(),
        }),
    }
}

fn query_contracts(deps: Deps) -> StdResult<Vec<ContractResponse>> {
    Ok(CONTRACTS
        .prefix(())
        .range(deps.storage, None, None, Order::Ascending)
        .map(|x| {
            let (name_in_utf8, addr) = x.unwrap();
            ContractResponse {
                name: String::from_utf8(name_in_utf8).unwrap(),
                addr,
            }
        })
        .collect())
}

fn query_contract_by_addr(deps: Deps, addr: Addr) -> StdResult<ContractResponse> {
    let contract_opt = get_contract_by_addr(deps.storage, &addr).unwrap();

    match contract_opt {
        Some(contract) => Ok(contract),
        None => Err(StdError::GenericErr {
            msg: "Contract not found".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary};

    #[test]
    fn entire_flow() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {};
        let info = mock_info("creator", &coins(1000, "earth"));

        // TEST::Initiate message
        let res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(0, res.messages.len());

        // TEST::No contracts should be added yet - Test for empty vec response
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllContracts {}).unwrap();
        let value: Vec<ContractResponse> = from_binary(&res).unwrap();

        let empty_vec_res: Vec<ContractResponse> = vec![];
        assert_eq!(empty_vec_res, value);

        let contract1 = ContractResponse {
            name: String::from("1"),
            addr: Addr::unchecked("1"),
        };
        let contract2 = ContractResponse {
            name: String::from("2"),
            addr: Addr::unchecked("2"),
        };

        // TEST::Add a contract and make sure it is success
        let msg = ExecuteMsg::AddContract {
            name: contract1.name.clone(),
            addr: contract1.addr.clone(),
        };
        let _res: Response = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        // TEST::Check the response of GetAllContracts {} ( Should have contract1 )
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllContracts {}).unwrap();
        let value: Vec<ContractResponse> = from_binary(&res).unwrap();
        assert_eq!(vec![contract1.clone()], value);

        // TEST::Add another contract and make sure it is success
        let msg = ExecuteMsg::AddContract {
            name: contract2.name.clone(),
            addr: contract2.addr.clone(),
        };
        let _res: Response = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        // TEST::Try adding same contact twice (Same Name and Addr) and make sure it fails
        let msg = ExecuteMsg::AddContract {
            name: contract1.name.clone(),
            addr: contract1.addr.clone(),
        };
        let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();
        assert!(matches!(
            res,
            ContractError::AlreadyExists {
                contract: _contract1
            }
        ));

        // TEST::Add new Contract with already existing Name and make sure it fails
        let msg = ExecuteMsg::AddContract {
            name: contract1.name.clone(),
            addr: Addr::unchecked("RandomAddr"),
        };
        let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();
        assert!(matches!(
            res,
            ContractError::AlreadyExists {
                contract: _contract1
            }
        ));

        // TEST::Add new Contract with already existing Address and make sure it fails
        let msg = ExecuteMsg::AddContract {
            name: String::from("RandomName"),
            addr: contract2.addr.clone(),
        };
        let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();
        assert!(matches!(
            res,
            ContractError::AlreadyExists {
                contract: _contract1
            }
        ));

        // TEST::Check the response of GetAllContracts {} ( Should have both the contracts )
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllContracts {}).unwrap();
        let value: Vec<ContractResponse> = from_binary(&res).unwrap();
        assert_eq!(vec![contract1.clone(), contract2.clone()], value);

        // TEST::Get contract1 By name and assert the result is correct
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetContractByName {
                name: contract1.name.clone(),
            },
        )
        .unwrap();
        let value: ContractResponse = from_binary(&res).unwrap();
        assert_eq!(contract1, value);

        // TEST::Get contract2 By addr and assert the result is correct
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetContractByAddr {
                addr: contract2.addr.clone(),
            },
        )
        .unwrap();
        let value: ContractResponse = from_binary(&res).unwrap();
        assert_eq!(contract2, value);

        // TEST: Remove contract 1
        let _res = execute(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            ExecuteMsg::RemoveContract {
                name: contract1.name.clone(),
            },
        )
        .unwrap();

        // TEST::Check the response of GetAllContracts {} ( Should have only the contract2 )
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllContracts {}).unwrap();
        let value: Vec<ContractResponse> = from_binary(&res).unwrap();
        assert_eq!(vec![contract2.clone()], value);
    }
}
