#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, to_vec, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response, StdError,
    StdResult, Storage,
};
use cw_storage_plus::Bound;

use crate::error::ContractError;
use crate::msg::{ContractResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{State, CONTRACTS, DEFAULT_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT, STATE};

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

pub fn add_contract(deps: DepsMut, name: String, addr: String) -> Result<Response, ContractError> {
    let contract_addr = deps.api.addr_validate(addr.as_str())?;
    let existing_contract =
        get_contract_by_name_or_addr(deps.storage, name.clone(), &contract_addr)?;

    if let Some(ec) = existing_contract {
        return Err(ContractError::AlreadyExists { contract: ec });
    }

    CONTRACTS.save(deps.storage, name.clone(), &contract_addr)?;

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
    if let Some(result) = addr_by_name_search {
        return Ok(Some(ContractResponse { name, addr: result }));
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
    Ok(Some(ContractResponse {
        name: String::from_utf8(contract_name_in_vecu8).unwrap(),
        addr: addr.clone(),
    }))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetAllContracts { start_after, limit } => {
            to_binary(&query_contracts(deps, start_after, limit)?)
        }
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

fn query_contracts(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<ContractResponse>> {
    let limit = limit
        .unwrap_or(DEFAULT_PAGINATION_LIMIT)
        .min(MAX_PAGINATION_LIMIT) as usize;
    let start = if let Some(sa) = start_after {
        Some(Bound::exclusive(sa))
    } else {
        None
    };

    Ok(CONTRACTS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|x| {
            let (name_in_utf8, addr) = x.unwrap();
            ContractResponse {
                name: String::from_utf8(name_in_utf8).unwrap(),
                addr,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary};

    #[test]
    fn test_get_all_contracts() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {};
        let info = mock_info("creator", &[]);

        // TEST::Initiate message
        let res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(0, res.messages.len());

        /*
            Test pagination
        */
        let contract1 = ContractResponse {
            name: String::from("contract_A"),
            addr: Addr::unchecked("contract_A"),
        };
        let contract2 = ContractResponse {
            name: String::from("contract_B"),
            addr: Addr::unchecked("contract_B"),
        };
        let contract3 = ContractResponse {
            name: String::from("contract_C"),
            addr: Addr::unchecked("contract_C"),
        };
        let contract4 = ContractResponse {
            name: String::from("contract_D"),
            addr: Addr::unchecked("contract_D"),
        };
        let contract5 = ContractResponse {
            name: String::from("contract_E"),
            addr: Addr::unchecked("contract_E"),
        };
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_A".to_string(),
                &Addr::unchecked("contract_A"),
            )
            .unwrap();
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_B".to_string(),
                &Addr::unchecked("contract_B"),
            )
            .unwrap();
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_C".to_string(),
                &Addr::unchecked("contract_C"),
            )
            .unwrap();
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_D".to_string(),
                &Addr::unchecked("contract_D"),
            )
            .unwrap();
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_E".to_string(),
                &Addr::unchecked("contract_E"),
            )
            .unwrap();
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetAllContracts {
                start_after: None,
                limit: None,
            },
        )
            .unwrap();
        let value: Vec<ContractResponse> = from_binary(&res).unwrap();
        assert_eq!(
            vec![
                contract1.clone(),
                contract2.clone(),
                contract3.clone(),
                contract4.clone(),
                contract5.clone()
            ],
            value
        );

        /*
           Test - 2. Pagination
        */
        let contract1 = ContractResponse {
            name: String::from("contract_A"),
            addr: Addr::unchecked("contract_A"),
        };
        let contract2 = ContractResponse {
            name: String::from("contract_B"),
            addr: Addr::unchecked("contract_B"),
        };
        let contract3 = ContractResponse {
            name: String::from("contract_C"),
            addr: Addr::unchecked("contract_C"),
        };
        let contract4 = ContractResponse {
            name: String::from("contract_D"),
            addr: Addr::unchecked("contract_D"),
        };
        let contract5 = ContractResponse {
            name: String::from("contract_E"),
            addr: Addr::unchecked("contract_E"),
        };
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_A".to_string(),
                &Addr::unchecked("contract_A"),
            )
            .unwrap();
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_B".to_string(),
                &Addr::unchecked("contract_B"),
            )
            .unwrap();
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_C".to_string(),
                &Addr::unchecked("contract_C"),
            )
            .unwrap();
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_D".to_string(),
                &Addr::unchecked("contract_D"),
            )
            .unwrap();
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_E".to_string(),
                &Addr::unchecked("contract_E"),
            )
            .unwrap();
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetAllContracts {
                start_after: Some("contract_B".to_string()),
                limit: Some(2),
            },
        )
            .unwrap();
        let value: Vec<ContractResponse> = from_binary(&res).unwrap();
        assert_eq!(vec![contract3.clone(), contract4.clone()], value);

        /*
           Test - 3
        */
        let contract1 = ContractResponse {
            name: String::from("contract_A"),
            addr: Addr::unchecked("contract_A"),
        };
        let contract2 = ContractResponse {
            name: String::from("contract_B"),
            addr: Addr::unchecked("contract_B"),
        };
        let contract3 = ContractResponse {
            name: String::from("contract_C"),
            addr: Addr::unchecked("contract_C"),
        };
        let contract4 = ContractResponse {
            name: String::from("contract_D"),
            addr: Addr::unchecked("contract_D"),
        };
        let contract5 = ContractResponse {
            name: String::from("contract_E"),
            addr: Addr::unchecked("contract_E"),
        };
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_A".to_string(),
                &Addr::unchecked("contract_A"),
            )
            .unwrap();
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_B".to_string(),
                &Addr::unchecked("contract_B"),
            )
            .unwrap();
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_C".to_string(),
                &Addr::unchecked("contract_C"),
            )
            .unwrap();
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_D".to_string(),
                &Addr::unchecked("contract_D"),
            )
            .unwrap();
        CONTRACTS
            .save(
                deps.as_mut().storage,
                "contract_E".to_string(),
                &Addr::unchecked("contract_E"),
            )
            .unwrap();
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetAllContracts {
                start_after: Some("contract_D".to_string()),
                limit: Some(2),
            },
        )
            .unwrap();
        let value: Vec<ContractResponse> = from_binary(&res).unwrap();
        assert_eq!(value, vec![contract5.clone()]);
    }

    #[test]
    fn entire_flow() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {};
        let info = mock_info("creator", &[]);

        // TEST::Initiate message
        let res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(0, res.messages.len());

        // TEST::No contracts should be added yet - Test for empty vec response
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetAllContracts {
                start_after: None,
                limit: None,
            },
        )
            .unwrap();
        let value: Vec<ContractResponse> = from_binary(&res).unwrap();

        let empty_vec_res: Vec<ContractResponse> = vec![];
        assert_eq!(empty_vec_res, value);

        let contract1 = ContractResponse {
            name: String::from("contract_A"),
            addr: Addr::unchecked("contract_A"),
        };
        let contract2 = ContractResponse {
            name: String::from("contract_B"),
            addr: Addr::unchecked("contract_B"),
        };

        // TEST::Add a contract and make sure it is success
        let msg = ExecuteMsg::AddContract {
            name: contract1.name.clone(),
            addr: contract1.addr.to_string(),
        };
        let _res: Response = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        // TEST::Check the response of GetAllContracts {} ( Should have contract1 )
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetAllContracts {
                start_after: None,
                limit: None,
            },
        )
            .unwrap();
        let value: Vec<ContractResponse> = from_binary(&res).unwrap();
        assert_eq!(vec![contract1.clone()], value);

        // TEST::Add another contract and make sure it is success
        let msg = ExecuteMsg::AddContract {
            name: contract2.name.clone(),
            addr: contract2.addr.to_string(),
        };
        let _res: Response = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        // TEST::Try adding same contact twice (Same Name and Addr) and make sure it fails
        let msg = ExecuteMsg::AddContract {
            name: contract1.name.clone(),
            addr: contract1.addr.to_string(),
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
            addr: "RandomAddr".to_string(),
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
            addr: contract2.addr.to_string(),
        };
        let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();
        assert!(matches!(
            res,
            ContractError::AlreadyExists {
                contract: _contract1
            }
        ));

        // TEST::Check the response of GetAllContracts {} ( Should have both the contracts )
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetAllContracts {
                start_after: None,
                limit: None,
            },
        )
            .unwrap();
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
    }
}
