#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};

    use crate::msg::{
        ExecuteMsg, GetConfigResponse, InstantiateMsg, QueryMsg, UpdateUserAirdropsRequest,
        UpdateUserRewardsRequest,
    };
    use crate::state::{Config, UserInfo, CONFIG, CW20_CONTRACTS_MAP, USER_REWARDS};
    use crate::testing::test_helpers::check_equal_vec;
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
        delegator_contract: Option<String>,
    ) -> Response<Empty> {
        let msg = InstantiateMsg {
            delegator_contract: Addr::unchecked(
                delegator_contract.unwrap_or("delegator_contract".to_string()),
            ),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("delegator_contract")),
        );

        // query the config
        let config_response: GetConfigResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetConfig {}).unwrap())
                .unwrap();
        let config = config_response.config;
        assert_eq!(
            config,
            Config {
                manager: Addr::unchecked("creator"),
                delegator_contract: Addr::unchecked("delegator_contract")
            }
        );
    }

    #[test]
    fn test_register_cw20_contract() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("delegator_contract")),
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-delegator", &[]),
            ExecuteMsg::RegisterCw20Contract {
                token: "".to_string(),
                cw20_contract: "".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
            Test - 2. Register a cw20 contract
        */
        let anc_contract = Addr::unchecked("anc_contract_addr");
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RegisterCw20Contract {
                token: "anc".to_string(),
                cw20_contract: anc_contract.to_string(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(res.attributes.len(), 0);

        let cw20_contract_addr = CW20_CONTRACTS_MAP
            .load(deps.as_mut().storage, "anc")
            .unwrap();
        assert_eq!(cw20_contract_addr, anc_contract);
    }

    #[test]
    fn test_withdraw_airdrops_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("delegator_contract")),
        );

        /*
           Test - 1. No user info
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::WithdrawAirdrops {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserInfoDoesNotExist {}));

        /*
           Test - 2. Cw20 Contracts are not registered
        */
        let user1 = Addr::unchecked("user1");
        let user1 = USER_REWARDS
            .save(
                deps.as_mut().storage,
                &user1,
                &UserInfo {
                    amount: Uint128::new(100),
                    airdrops: vec![Coin::new(100_u128, "anc".to_string())],
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("user1", &[]),
            ExecuteMsg::WithdrawAirdrops {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Cw20ContractNotRegistered(_)));
    }

    #[test]
    fn test_withdraw_airdrops_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("delegator_contract")),
        );

        let user1 = Addr::unchecked("user1");
        let anc_contract = Addr::unchecked("anc_contract_addr");
        let mir_contract = Addr::unchecked("mir_contract_addr");

        USER_REWARDS
            .save(
                deps.as_mut().storage,
                &user1,
                &UserInfo {
                    amount: Uint128::new(100_u128),
                    airdrops: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(200_u128, "mir".to_string()),
                    ],
                },
            )
            .unwrap();
        CW20_CONTRACTS_MAP
            .save(deps.as_mut().storage, "anc", &anc_contract)
            .unwrap();
        CW20_CONTRACTS_MAP
            .save(deps.as_mut().storage, "mir", &mir_contract)
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("user1", &[]),
            ExecuteMsg::WithdrawAirdrops {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: anc_contract.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: user1.to_string(),
                        amount: Uint128::new(100_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: mir_contract.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: user1.to_string(),
                        amount: Uint128::new(200_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                })
            ]
        ));

        let user1_info = USER_REWARDS.load(deps.as_mut().storage, &user1).unwrap();
        assert!(check_equal_vec(
            user1_info.airdrops,
            vec![
                Coin::new(0, "anc".to_string()),
                Coin::new(0, "mir".to_string())
            ]
        ));
    }

    #[test]
    fn test_update_user_airdrops() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("delegator_contract")),
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-delegator", &[]),
            ExecuteMsg::UpdateUserAirdrops {
                update_user_airdrops_requests: vec![],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. No requests sent
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("delegator_contract", &[]),
            ExecuteMsg::UpdateUserAirdrops {
                update_user_airdrops_requests: vec![],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "zero_user_airdrops_requests".to_string(),
                value: "1".to_string()
            }]
        ));

        /*
           Test - 3. Airdrops come in
        */
        let user1 = Addr::unchecked("user1");
        let user2 = Addr::unchecked("user2");

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("delegator_contract", &[]),
            ExecuteMsg::UpdateUserAirdrops {
                update_user_airdrops_requests: vec![
                    UpdateUserAirdropsRequest {
                        user: user1.clone(),
                        pool_airdrops: vec![
                            Coin::new(1000_u128, "anc".to_string()),
                            Coin::new(2000_u128, "mir".to_string()),
                        ],
                    },
                    UpdateUserAirdropsRequest {
                        user: user2.clone(),
                        pool_airdrops: vec![
                            Coin::new(2000_u128, "anc".to_string()),
                            Coin::new(3000_u128, "mir".to_string()),
                        ],
                    },
                ],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(res.attributes.len(), 0);

        let user1_info = USER_REWARDS.load(deps.as_mut().storage, &user1).unwrap();
        let user2_info = USER_REWARDS.load(deps.as_mut().storage, &user2).unwrap();

        assert!(check_equal_vec(
            user1_info.airdrops,
            vec![
                Coin::new(1000_u128, "anc".to_string()),
                Coin::new(2000_u128, "mir".to_string())
            ]
        ));
        assert!(check_equal_vec(
            user2_info.airdrops,
            vec![
                Coin::new(2000_u128, "anc".to_string()),
                Coin::new(3000_u128, "mir".to_string())
            ]
        ));
    }

    #[test]
    fn test_update_user_rewards() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("delegator_contract")),
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-delegator", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("delegator_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![],
            },
        )
        .unwrap();
        assert!(res.messages.is_empty());
        assert_eq!(res.attributes.len(), 1);

        let user1 = Addr::unchecked("user1");
        let user2 = Addr::unchecked("user2");

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("delegator_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![
                    UpdateUserRewardsRequest {
                        user: user1.clone(),
                        funds: Uint128::new(200),
                    },
                    UpdateUserRewardsRequest {
                        user: user2.clone(),
                        funds: Uint128::new(300),
                    },
                ],
            },
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let user1_rewards = USER_REWARDS
            .load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        let user2_rewards = USER_REWARDS
            .load(deps.as_mut().storage, &user2.clone())
            .unwrap();
        assert_eq!(user1_rewards.amount, Uint128::new(200));
        assert_eq!(user2_rewards.amount, Uint128::new(300));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("delegator_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(300),
                }],
            },
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let user1_rewards = USER_REWARDS
            .load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        let user2_rewards = USER_REWARDS
            .load(deps.as_mut().storage, &user2.clone())
            .unwrap();
        assert_eq!(user1_rewards.amount, Uint128::new(500));
        assert_eq!(user2_rewards.amount, Uint128::new(300));
    }

    #[test]
    fn test_withdraw_funds() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("manager", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("delegator_contract")),
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-manager", &[]),
            ExecuteMsg::WithdrawFunds {
                withdraw_address: Addr::unchecked("randomAddr"),
                amount: Default::default(),
                denom: "utest".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("manager", &[]),
            ExecuteMsg::WithdrawFunds {
                withdraw_address: Addr::unchecked("randomAddr"),
                amount: Default::default(),
                denom: "utest".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::AmountZero {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("manager", &[]),
            ExecuteMsg::WithdrawFunds {
                withdraw_address: Addr::unchecked("randomAddr"),
                amount: Uint128::new(800),
                denom: "utest".to_string(),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(BankMsg::Send {
                to_address: "randomAddr".to_string(),
                amount: vec![Coin::new(800, "utest")]
            })
        );
    }

    #[test]
    fn test_update_config() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_msg = ExecuteMsg::UpdateConfig {
            delegator_contract: None,
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
            delegator_contract: Addr::unchecked("delegator_contract")
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
            delegator_contract: Addr::unchecked("new_delegator_contract")
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                delegator_contract: Some(Addr::unchecked("new_delegator_contract")),
            }
                .clone(),
        )
            .unwrap();
        assert!(res.messages.is_empty());
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config, expected_config);
    }
}
