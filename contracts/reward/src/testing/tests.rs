#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};
    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, GetConfigResponse, InstantiateMsg, QueryMsg};
    use crate::state::{Config, TmpManagerStore, CONFIG, TMP_MANAGER_STORE};
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use cosmwasm_std::{
        from_binary, Addr, BankMsg, Coin, Env, MessageInfo, OwnedDeps, Response, SubMsg, Uint128,
    };
    use terra_cosmwasm::TerraMsgWrapper;

    pub fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
        info: &MessageInfo,
        env: &Env,
        _vault_denom: Option<String>,
    ) -> Response<TerraMsgWrapper> {
        let instantiate_msg = InstantiateMsg {
            staking_contract: "pools_addr".to_string(),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            staking_contract: "pools_addr".to_string(),
        };
        let expected_config = Config {
            manager: Addr::unchecked("creator"),
            reward_denom: "uluna".to_string(),
            staking_contract: Addr::unchecked("pools_addr"),
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
        let mut deps = mock_dependencies();
        let info = mock_info("creator", &[]);
        let env = mock_env();
        instantiate_contract(&mut deps, &info, &env, None);
        let reward_withdraw_contract = Addr::unchecked("reward_withdraw_contract");
        let protocol_fee_contract = Addr::unchecked("protocol_fee_contract");
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Transfer {
                reward_amount: Uint128::new(300),
                reward_withdraw_contract: reward_withdraw_contract.clone(),
                protocol_fee_amount: Uint128::zero(),
                protocol_fee_contract: protocol_fee_contract.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        deps.querier
            .update_balance(env.contract.address.clone(), vec![Coin::new(100, "uluna")]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[]),
            ExecuteMsg::Transfer {
                reward_amount: Uint128::new(300),
                reward_withdraw_contract: reward_withdraw_contract.clone(),
                protocol_fee_amount: Uint128::zero(),
                protocol_fee_contract: protocol_fee_contract.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InSufficientFunds {}));

        deps.querier
            .update_balance(env.contract.address.clone(), vec![Coin::new(2000, "uluna")]);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[]),
            ExecuteMsg::Transfer {
                reward_amount: Uint128::zero(),
                reward_withdraw_contract: reward_withdraw_contract.clone(),
                protocol_fee_amount: Uint128::zero(),
                protocol_fee_contract: protocol_fee_contract.clone(),
            },
        )
        .unwrap();
        assert!(res.messages.is_empty());

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[]),
            ExecuteMsg::Transfer {
                reward_amount: Uint128::new(200),
                reward_withdraw_contract: reward_withdraw_contract.clone(),
                protocol_fee_amount: Uint128::new(2),
                protocol_fee_contract: protocol_fee_contract.clone(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages[0],
            SubMsg::new(BankMsg::Send {
                to_address: reward_withdraw_contract.to_string(),
                amount: vec![Coin::new(200, "uluna")]
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(BankMsg::Send {
                to_address: protocol_fee_contract.to_string(),
                amount: vec![Coin::new(2, "uluna")]
            })
        );
    }

    #[test]
    fn test_set_manager() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);

        instantiate_contract(&mut deps, &info, &env, None);

        /*
           Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::SetManager {
                manager: "test_manager".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
            Successful
        */
        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::SetManager {
                manager: "test_manager".to_string(),
            },
        )
        .unwrap();
        let tmp_manager_store = TMP_MANAGER_STORE.load(deps.as_mut().storage).unwrap();
        assert_eq!(tmp_manager_store.manager, "test_manager".to_string())
    }

    #[test]
    fn test_accept_manager() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);

        instantiate_contract(&mut deps, &info, &env, None);

        /*
           Empty tmp store
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AcceptManager {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::TmpManagerStoreEmpty {}));

        /*
            Successful
        */
        TMP_MANAGER_STORE
            .save(
                deps.as_mut().storage,
                &TmpManagerStore {
                    manager: "new_manager".to_string(),
                },
            )
            .unwrap();
        /*
            Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::AcceptManager {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Successful
        */
        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("new_manager", &[]),
            ExecuteMsg::AcceptManager {},
        )
        .unwrap();
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config.manager, Addr::unchecked("new_manager"));
        let tmp_manager_store = TMP_MANAGER_STORE.may_load(deps.as_mut().storage).unwrap();
        assert_eq!(tmp_manager_store, None);
    }

    #[test]
    fn test_update_config() {
        let mut deps = mock_dependencies();
        let info = mock_info("creator", &[]);
        let env = mock_env();

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_msg = ExecuteMsg::UpdateConfig {
            staking_contract: None,
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
            reward_denom: "uluna".to_string(),
            staking_contract: Addr::unchecked("pools_addr"),
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
            reward_denom: "uluna".to_string(),
            staking_contract: Addr::unchecked("new_pools_addr"),
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                staking_contract: Some("new_pools_addr".to_string()),
            }
            .clone(),
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config, expected_config);
    }
}
