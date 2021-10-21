#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};
    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, GetConfigResponse, InstantiateMsg, QueryMsg};
    use crate::state::{Config,CONFIG};
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use cosmwasm_std::{
        from_binary, Addr, BankMsg, Coin, Env, MessageInfo, OwnedDeps, Response,
        SubMsg, Uint128,
    };
    use terra_cosmwasm::TerraMsgWrapper;

    pub fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
        info: &MessageInfo,
        env: &Env,
        vault_denom: Option<String>,
    ) -> Response<TerraMsgWrapper> {
        let instantiate_msg = InstantiateMsg {
            reward_denom: vault_denom.unwrap_or_else(|| "utest".to_string()),
            pools_contract: Addr::unchecked("pools_addr"),
            scc_contract: Addr::unchecked("scc_addr"),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            reward_denom: "utest".to_string(),
            pools_contract: Addr::unchecked("pools_addr"),
            scc_contract: Addr::unchecked("scc_addr"),
        };
        let expected_config = Config {
            manager: Addr::unchecked("creator"),
            reward_denom: "utest".to_string(),
            pools_contract: Addr::unchecked("pools_addr"),
            scc_contract: Addr::unchecked("scc_addr"),
        };
        let info = mock_info("creator", &[]);

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let value: GetConfigResponse = from_binary(&res).unwrap();
        assert_eq!(value.config, expected_config);
    }

    #[test]
    fn test_transfer() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        instantiate_contract(&mut deps, &info, &env, None);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Transfer {
                amount: Uint128::new(300)
            },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[]),
            ExecuteMsg::Transfer {
                amount: Uint128::new(0)
            },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::ZeroAmount {}));

        deps.querier.update_balance(env.contract.address.clone(), vec![Coin::new(100, "utest")]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[]),
            ExecuteMsg::Transfer {
                amount: Uint128::new(200)
            },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::InSufficientFunds {}));

        deps.querier.update_balance(env.contract.address.clone(), vec![Coin::new(2000, "utest")]);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[]),
            ExecuteMsg::Transfer {
                amount: Uint128::new(200)
            },
        )
            .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0], SubMsg::new(BankMsg::Send { to_address: "scc_addr".to_string(), amount: vec![Coin::new(200, "utest")] }));
    }

    #[test]
    fn test_update_config() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_msg = ExecuteMsg::UpdateConfig {
            pools_contract: None,
            scc_contract: None
        };
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            initial_msg.clone(),
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let mut expected_config = Config {
            manager: Addr::unchecked("creator"),
            reward_denom: "utest".to_string(),
            pools_contract: Addr::unchecked("pools_addr"),
            scc_contract: Addr::unchecked("scc_addr")
        };
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config, expected_config);

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            initial_msg.clone(),
        )
            .unwrap();
        assert!(res.messages.is_empty());
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config, expected_config);

        expected_config = Config {
            manager: Addr::unchecked("creator"),
            reward_denom: "utest".to_string(),
            pools_contract: Addr::unchecked("new_pools_addr"),
            scc_contract: Addr::unchecked("new_scc_addr")
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                pools_contract: Some(Addr::unchecked("new_pools_addr")),
                scc_contract: Some(Addr::unchecked("new_scc_addr"))
            }
                .clone(),
        )
            .unwrap();
        assert!(res.messages.is_empty());
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config, expected_config);
    }

}
