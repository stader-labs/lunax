#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};

    use crate::error::ContractError::TokenEmpty;
    use crate::msg::{ExecuteMsg, GetConfigResponse, InstantiateMsg, QueryMsg};
    use crate::state::{
        AirdropRegistryInfo, Config, TmpManagerStore, AIRDROP_REGISTRY, CONFIG, TMP_MANAGER_STORE,
    };
    use crate::ContractError;
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use cosmwasm_std::{
        coins, from_binary, to_binary, Addr, Attribute, BankMsg, Coin, Empty, Env, MessageInfo,
        OwnedDeps, Response, SubMsg, Uint128, WasmMsg,
    };
    use cw20::Cw20ExecuteMsg;

    fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
        info: &MessageInfo,
        env: &Env,
    ) -> Response<Empty> {
        let msg = InstantiateMsg {};

        return instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        // query the config
        let config_response: GetConfigResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetConfig {}).unwrap())
                .unwrap();
        let config = config_response.config;
        assert_eq!(
            config,
            Config {
                manager: Addr::unchecked("creator"),
            }
        );
    }

    #[test]
    fn test_update_airdrop_registry() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::UpdateAirdropRegistry {
                airdrop_token: "".to_string(),
                airdrop_contract: "".to_string(),
                cw20_contract: "".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
            Test - 2. Token empty
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateAirdropRegistry {
                airdrop_token: "".to_string(),
                airdrop_contract: "".to_string(),
                cw20_contract: "".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::TokenEmpty {}));

        /*
           Test - 3. Success
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateAirdropRegistry {
                airdrop_token: "anc".to_string(),
                airdrop_contract: "anc_airdrop_contract".to_string(),
                cw20_contract: "anc_cw20_contract".to_string(),
            },
        )
        .unwrap();
        let anc_info = AIRDROP_REGISTRY
            .load(deps.as_mut().storage, "anc".to_string())
            .unwrap();
        assert_eq!(
            anc_info,
            AirdropRegistryInfo {
                token: "anc".to_string(),
                airdrop_contract: Addr::unchecked("anc_airdrop_contract"),
                cw20_contract: Addr::unchecked("anc_cw20_contract")
            }
        );
    }

    #[test]
    fn test_set_manager() {
        let mut deps = mock_dependencies(&[]);
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let _res = instantiate_contract(&mut deps, &info, &env);

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
        let res = execute(
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
        let mut deps = mock_dependencies(&[]);
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let _res = instantiate_contract(&mut deps, &info, &env);

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
        let res = execute(
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
}
