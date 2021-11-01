#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};
    use crate::error::ContractError;
    use crate::helpers::{get_pool_stake_info, get_validator_for_deposit};
    use crate::msg::{ExecuteMsg, GetStateResponse, InstantiateMsg, QueryMsg};
    use crate::state::{State, STATE};
    use crate::testing::mock_querier::{
        mock_dependencies, SwapRates, WasmMockQuerier, MOCK_CONTRACT_ADDR,
    };
    use crate::testing::test_helpers::check_equal_vec;
    use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage};
    use cosmwasm_std::{
        from_binary, to_binary, Addr, Attribute, BankMsg, Binary, Coin, Decimal, DistributionMsg,
        Empty, Env, FullDelegation, MessageInfo, OwnedDeps, Response, StakingMsg, StdResult,
        SubMsg, Uint128, Validator, WasmMsg,
    };
    use reward::msg::ExecuteMsg as reward_execute;
    use terra_cosmwasm::create_swap_msg;

    fn get_validators() -> Vec<Validator> {
        vec![
            Validator {
                address: "valid0001".to_string(),
                commission: Decimal::zero(),
                max_commission: Decimal::zero(),
                max_change_rate: Decimal::zero(),
            },
            Validator {
                address: "valid0002".to_string(),
                commission: Decimal::zero(),
                max_commission: Decimal::zero(),
                max_change_rate: Decimal::zero(),
            },
        ]
    }

    fn get_delegations() -> Vec<FullDelegation> {
        vec![
            FullDelegation {
                delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                validator: "valid0001".to_string(),
                amount: Coin::new(2000, "uluna".to_string()),
                can_redelegate: Coin::new(1000, "uluna".to_string()),
                accumulated_rewards: vec![
                    Coin::new(20, "uluna".to_string()),
                    Coin::new(30, "urew1"),
                ],
            },
            FullDelegation {
                delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                validator: "valid0002".to_string(),
                amount: Coin::new(2000, "uluna".to_string()),
                can_redelegate: Coin::new(0, "uluna".to_string()),
                accumulated_rewards: vec![
                    Coin::new(40, "uluna".to_string()),
                    Coin::new(60, "urew1"),
                ],
            },
        ]
    }

    macro_rules! hashmap {
        ($( $key: expr => $val: expr ),*) => {{
             let mut map = ::std::collections::HashMap::new();
             $( map.insert($key, $val); )*
             map
        }}
    }

    pub fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
        info: &MessageInfo,
        env: &Env,
        validators: Option<Vec<Addr>>,
        strategy_denom: Option<String>,
    ) -> Response<Empty> {
        let default_validator1: Addr = Addr::unchecked("valid0001");
        let default_validator2: Addr = Addr::unchecked("valid0002");
        let scc_address: Addr = Addr::unchecked("scc-address");
        let reward_contract_address: Addr = Addr::unchecked("reward_contract_address");

        let instantiate_msg = InstantiateMsg {
            scc_address: scc_address.to_string(),
            reward_contract_address: reward_contract_address.to_string(),
            strategy_denom: strategy_denom.unwrap_or("uluna".to_string()),
            initial_validators: validators
                .unwrap_or_else(|| vec![default_validator1, default_validator2]),
            min_validator_pool_size: Some(2),
            manager_seed_funds: Uint128::new(1000_u128),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    fn get_scc_contract_address() -> String {
        String::from("scc-address")
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let default_validator1: Addr = Addr::unchecked("valid0001");
        let default_validator2: Addr = Addr::unchecked("valid0002");
        let scc_address: Addr = Addr::unchecked("scc-address");
        let reward_contract_address: Addr = Addr::unchecked("reward_contract_address");

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None, None);
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(DistributionMsg::SetWithdrawAddress {
                address: reward_contract_address.to_string()
            })]
        ));

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        assert_eq!(
            state_response.state.unwrap(),
            State {
                manager: info.sender,
                scc_address,
                reward_contract_address,
                manager_seed_funds: Uint128::new(1000_u128),
                min_validator_pool_size: 2,
                strategy_denom: "uluna".to_string(),
                contract_genesis_block_height: env.block.height,
                contract_genesis_timestamp: env.block.time,
                validator_pool: vec![default_validator1, default_validator2],
            }
        );
    }

    #[test]
    fn test_set_reward_withdraw_address() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::SetRewardWithdrawAddress {
                reward_contract: "".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Set new addr
        */
        let new_reward_contract_addr = Addr::unchecked("new_reward_contract_addr");
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::SetRewardWithdrawAddress {
                reward_contract: new_reward_contract_addr.to_string(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(DistributionMsg::SetWithdrawAddress {
                address: new_reward_contract_addr.to_string()
            })]
        ));
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.reward_contract_address, new_reward_contract_addr);
    }

    #[test]
    fn test_get_validator_for_deposit_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. Empty validator array
        */
        let err = get_validator_for_deposit(
            deps.as_mut().querier,
            env.contract.address.to_string(),
            vec![],
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoValidatorsInPool {}));

        /*
           Test - 2. All validators jailed
        */
        deps.querier.update_staking("uluna", &[], &[]);
        let err = get_validator_for_deposit(
            deps.as_mut().querier,
            env.contract.address.to_string(),
            vec![Addr::unchecked("valid0001"), Addr::unchecked("valid0002")],
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::AllValidatorsJailed {}));
    }

    #[test]
    fn test_get_validator_for_deposit_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. Validator with no stake
        */
        fn get_some_validators_test_1() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0003".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_1() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(20, "uluna".to_string()),
                        Coin::new(30, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(3000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_1(),
            &*get_some_delegations_test_1(),
        );
        let val_addr = get_validator_for_deposit(
            deps.as_mut().querier,
            env.contract.address.to_string(),
            vec![
                Addr::unchecked("valid0001"),
                Addr::unchecked("valid0002"),
                Addr::unchecked("valid0003"),
            ],
        )
        .unwrap();
        assert_eq!(val_addr, Addr::unchecked("valid0003"));

        /*
           Test - 2. Validators with stake
        */
        fn get_some_validators_test_2() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0003".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_2() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(7000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(20, "uluna".to_string()),
                        Coin::new(30, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0003".to_string(),
                    amount: Coin::new(5000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_2(),
            &*get_some_delegations_test_2(),
        );
        let val_addr = get_validator_for_deposit(
            deps.as_mut().querier,
            env.contract.address.to_string(),
            vec![
                Addr::unchecked("valid0001"),
                Addr::unchecked("valid0002"),
                Addr::unchecked("valid0003"),
            ],
        )
        .unwrap();
        assert_eq!(val_addr, Addr::unchecked("valid0002"));
    }

    #[test]
    fn test_get_pool_stake_info() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        fn get_some_validators_test() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0003".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(20, "uluna".to_string()),
                        Coin::new(30, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(3000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test(),
            &*get_some_delegations_test(),
        );
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validator_pool = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    Ok(state)
                },
            )
            .unwrap();

        let state = STATE.load(deps.as_mut().storage).unwrap();
        let pool_stake_info = get_pool_stake_info(
            deps.as_mut().querier,
            env.contract.address.to_string(),
            state.validator_pool,
        )
        .unwrap();
        let expected_validator_map =
            hashmap!(valid1 => Uint128::new(2000_u128), valid2 => Uint128::new(3000_u128));
        assert_eq!(pool_stake_info.0, expected_validator_map);
        assert_eq!(pool_stake_info.1, Uint128::new(5000_u128));
    }

    #[test]
    fn test_swap_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::Swap {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test_swap_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Swap {},
        )
        .unwrap();

        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: state.reward_contract_address.to_string(),
                msg: to_binary(&reward_execute::Swap {}).unwrap(),
                funds: vec![]
            })]
        ))
    }

    #[test]
    fn test_update_config_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::UpdateConfig {
                min_validator_pool_size: None,
                scc_address: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test_update_config_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                min_validator_pool_size: Some(5),
                scc_address: Some("new_scc_address".to_string()),
            },
        )
        .unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.min_validator_pool_size, 5);
        assert_eq!(state.scc_address, Addr::unchecked("new_scc_address"));
    }

    #[test]
    fn test_remove_validator_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. Not authorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::RemoveValidator {
                removed_val: "".to_string(),
                redelegate_val: "".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Validator not in pool
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validator_pool = vec![Addr::unchecked("abc"), Addr::unchecked("def")];
                    Ok(state)
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                removed_val: "fsdf".to_string(),
                redelegate_val: "asd".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotInPool {}));

        /*
            Test - 3. Cannot remove more validators
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.min_validator_pool_size = 2;
                    state.validator_pool = vec![Addr::unchecked("abc"), Addr::unchecked("def")];
                    Ok(state)
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                removed_val: "abc".to_string(),
                redelegate_val: "def".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::CannotRemoveMoreValidators {}));

        /*
            Test - 4. Redelegation validator does not exist
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.min_validator_pool_size = 1;
                    state.validator_pool = vec![Addr::unchecked("abc"), Addr::unchecked("def")];
                    Ok(state)
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                removed_val: "abc".to_string(),
                redelegate_val: "def".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorDoesNotExist {}));
    }

    #[test]
    fn test_remove_validator_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. Validator being removed has no delegation
        */
        fn get_some_validators_test_1() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0003".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_1() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(20, "uluna".to_string()),
                        Coin::new(30, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
            ]
        }

        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_1(),
            &*get_some_delegations_test_1(),
        );
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validator_pool = vec![
                        Addr::unchecked("valid0001"),
                        Addr::unchecked("valid0002"),
                        Addr::unchecked("valid0003"),
                    ];
                    Ok(state)
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                removed_val: "valid0003".to_string(),
                redelegate_val: "valid0001".to_string(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert!(check_equal_vec(
            state.validator_pool,
            vec![Addr::unchecked("valid0001"), Addr::unchecked("valid0002")]
        ));
        /*
           Test - 2. Validator has delegation
        */
        fn get_some_validators_test_2() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0003".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_2() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(20, "uluna".to_string()),
                        Coin::new(30, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0003".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_2(),
            &*get_some_delegations_test_2(),
        );
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validator_pool = vec![
                        Addr::unchecked("valid0001"),
                        Addr::unchecked("valid0002"),
                        Addr::unchecked("valid0003"),
                    ];
                    Ok(state)
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                removed_val: "valid0003".to_string(),
                redelegate_val: "valid0002".to_string(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(StakingMsg::Redelegate {
                src_validator: "valid0003".to_string(),
                dst_validator: "valid0002".to_string(),
                amount: Coin::new(2000_u128, "uluna".to_string())
            })]
        ));
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert!(check_equal_vec(
            state.validator_pool,
            vec![Addr::unchecked("valid0001"), Addr::unchecked("valid0002")]
        ));

        /*
           Test - 3. Validator is being removed from the list when no staking has been done
        */
        fn get_some_validators_test_3() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0003".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_3() -> Vec<FullDelegation> {
            vec![]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_3(),
            &*get_some_delegations_test_3(),
        );
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validator_pool = vec![
                        Addr::unchecked("valid0001"),
                        Addr::unchecked("valid0002"),
                        Addr::unchecked("valid0003"),
                        Addr::unchecked("valid0004"),
                    ];
                    Ok(state)
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                removed_val: "valid0004".to_string(),
                redelegate_val: "valid0003".to_string(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert!(check_equal_vec(
            state.validator_pool,
            vec![
                Addr::unchecked("valid0001"),
                Addr::unchecked("valid0002"),
                Addr::unchecked("valid0003")
            ]
        ));

        /*
           Test - 4. Redelegating to new validator in pool
        */
        fn get_some_validators_test_4() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0003".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0004".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_4() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(20, "uluna".to_string()),
                        Coin::new(30, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0003".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_4(),
            &*get_some_delegations_test_4(),
        );
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validator_pool = vec![
                        Addr::unchecked("valid0001"),
                        Addr::unchecked("valid0002"),
                        Addr::unchecked("valid0003"),
                    ];
                    Ok(state)
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                removed_val: "valid0003".to_string(),
                redelegate_val: "valid0004".to_string(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(StakingMsg::Redelegate {
                src_validator: "valid0003".to_string(),
                dst_validator: "valid0004".to_string(),
                amount: Coin::new(2000_u128, "uluna".to_string())
            })]
        ));
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert!(check_equal_vec(
            state.validator_pool,
            vec![
                Addr::unchecked("valid0001"),
                Addr::unchecked("valid0002"),
                Addr::unchecked("valid0004")
            ]
        ));
    }

    #[test]
    fn test_replace_validator_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::ReplaceValidator {
                src_validator: "abc".to_string(),
                dst_validator: "def".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Src validator is the same as dest validator
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ReplaceValidator {
                src_validator: "abc".to_string(),
                dst_validator: "abc".to_string(),
            },
        )
        .unwrap();
        assert_eq!(res, Response::default());

        /*
           Test - 3. Validator not in pool
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ReplaceValidator {
                src_validator: "abc".to_string(),
                dst_validator: "def".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotInPool {}));

        /*
           Test - 4. Validator already exists in pool
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ReplaceValidator {
                src_validator: "valid0002".to_string(),
                dst_validator: "valid0001".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::ValidatorAlreadyExistsInPool {}
        ));
    }

    #[test]
    fn test_replace_validator_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. src validator has stake
        */
        fn get_some_validators_test_1() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0003".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_1() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(20, "uluna".to_string()),
                        Coin::new(30, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0003".to_string(),
                    amount: Coin::new(0, "uluna".to_string()),
                    can_redelegate: Coin::new(0, "uluna".to_string()),
                    accumulated_rewards: vec![],
                },
            ]
        }

        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_1(),
            &*get_some_delegations_test_1(),
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ReplaceValidator {
                src_validator: "valid0001".to_string(),
                dst_validator: "valid0003".to_string(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(StakingMsg::Redelegate {
                src_validator: "valid0001".to_string(),
                dst_validator: "valid0003".to_string(),
                amount: Coin::new(2000_u128, "uluna".to_string())
            })]
        ));

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert!(check_equal_vec(
            state.validator_pool,
            vec![Addr::unchecked("valid0002"), Addr::unchecked("valid0003")]
        ));
        /*
           Test - 2. Src validator has no stake
        */
        fn get_some_validators_test_2() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0003".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_2() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0003".to_string(),
                    amount: Coin::new(0, "uluna".to_string()),
                    can_redelegate: Coin::new(0, "uluna".to_string()),
                    accumulated_rewards: vec![],
                },
            ]
        }

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validator_pool =
                        vec![Addr::unchecked("valid0001"), Addr::unchecked("valid0002")];
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_2(),
            &*get_some_delegations_test_2(),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ReplaceValidator {
                src_validator: "valid0001".to_string(),
                dst_validator: "valid0003".to_string(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert!(check_equal_vec(
            state.validator_pool,
            vec![Addr::unchecked("valid0002"), Addr::unchecked("valid0003")]
        ));

        /*
           Test - 3. Replacing validator when there has been no stake from the contract
        */
        fn get_some_validators_test_3() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0003".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0004".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_3() -> Vec<FullDelegation> {
            vec![]
        }

        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_3(),
            &*get_some_delegations_test_3(),
        );
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validator_pool = vec![
                        Addr::unchecked("valid0001"),
                        Addr::unchecked("valid0002"),
                        Addr::unchecked("valid0003"),
                    ];
                    Ok(state)
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ReplaceValidator {
                src_validator: "valid0001".to_string(),
                dst_validator: "valid0004".to_string(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert!(check_equal_vec(
            state.validator_pool,
            vec![
                Addr::unchecked("valid0002"),
                Addr::unchecked("valid0003"),
                Addr::unchecked("valid0004")
            ]
        ));
    }

    #[test]
    fn test_add_validator_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::AddValidator {
                validator: "abc".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Validator already in pool
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                validator: "valid0001".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::ValidatorAlreadyExistsInPool {}
        ));

        /*
           Test - 3. Validator does not exist in blockchain
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                validator: "valid0003".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorDoesNotExist {}));
    }

    #[test]
    fn test_add_validator_success() {
        fn get_some_validators() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0003".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .filter(|x| x.address.ne("valid0003"))
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        deps.querier
            .update_staking("uluna", &*get_some_validators(), &[]);

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                validator: "valid0003".to_string(),
            },
        )
        .unwrap();

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert!(check_equal_vec(
            state.validator_pool,
            vec![
                Addr::unchecked("valid0001"),
                Addr::unchecked("valid0002"),
                Addr::unchecked("valid0003"),
            ]
        ))
    }

    #[test]
    fn test_transfer_undelegated_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-scc", &[]),
            ExecuteMsg::TransferUndelegatedRewards {
                amount: Uint128::new(100_u128),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test_transfer_undelegated_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. amount is equal to the unaccounted funds
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.manager_seed_funds = Uint128::new(1000_u128);
                    Ok(state)
                },
            )
            .unwrap();

        deps.querier.update_balance(
            env.contract.address.clone(),
            vec![
                Coin::new(2800_u128, "uluna".to_string()),
                Coin::new(200_u128, "uusd".to_string()),
            ],
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferUndelegatedRewards {
                amount: Uint128::new(1000_u128),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: get_scc_contract_address(),
                amount: vec![Coin::new(1000_u128, "uluna".to_string())]
            })]
        ));

        /*
            Test - 2. unaccounted_funds is less than the amount
        */
        deps.querier.update_balance(
            env.contract.address.clone(),
            vec![
                Coin::new(2400_u128, "uluna".to_string()),
                Coin::new(200_u128, "uusd".to_string()),
            ],
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferUndelegatedRewards {
                amount: Uint128::new(1500_u128),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: get_scc_contract_address(),
                amount: vec![Coin::new(1400_u128, "uluna".to_string())]
            })]
        ));

        /*
            Test - 2. unaccounted_funds is more than the amount
        */
        deps.querier.update_balance(
            env.contract.address.clone(),
            vec![
                Coin::new(2500_u128, "uluna".to_string()),
                Coin::new(200_u128, "uusd".to_string()),
            ],
        );

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.manager_seed_funds = Uint128::zero();
                    Ok(state)
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferUndelegatedRewards {
                amount: Uint128::new(700_u128),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: get_scc_contract_address(),
                amount: vec![Coin::new(700_u128, "uluna".to_string())]
            })]
        ));
    }

    #[test]
    fn test_claim_airdrops_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let airdrop_token_contract = Addr::unchecked("airdrop_token_contract");
        let cw20_token_contract = Addr::unchecked("cw20_token_contract");
        fn get_airdrop_claim_msg() -> Binary {
            Binary::from(vec![01, 02, 03, 04, 05, 06, 07, 08])
        }

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-scc", &[]),
            ExecuteMsg::ClaimAirdrops {
                airdrop_token_contract: airdrop_token_contract.to_string(),
                cw20_token_contract: cw20_token_contract.to_string(),
                airdrop_token: "abc".to_string(),
                amount: Uint128::new(1000_u128),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test_claim_airdrops_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let airdrop_token_contract = Addr::unchecked("airdrop_token_contract");
        let cw20_token_contract = Addr::unchecked("cw20_token_contract");
        let scc_address: Addr = Addr::unchecked("scc-address");

        fn get_airdrop_claim_msg() -> Binary {
            Binary::from(vec![01, 02, 03, 04, 05, 06, 07, 08])
        }

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::ClaimAirdrops {
                airdrop_token_contract: airdrop_token_contract.to_string(),
                cw20_token_contract: cw20_token_contract.to_string(),
                airdrop_token: "abc".to_string(),
                amount: Uint128::new(1000_u128),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: airdrop_token_contract.to_string(),
                    msg: get_airdrop_claim_msg(),
                    funds: vec![],
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: cw20_token_contract.to_string(),
                    msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                        recipient: scc_address.to_string(),
                        amount: Uint128::new(1000_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                })
            ]
        ));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::ClaimAirdrops {
                airdrop_token_contract: airdrop_token_contract.to_string(),
                cw20_token_contract: cw20_token_contract.to_string(),
                airdrop_token: "abc".to_string(),
                amount: Uint128::new(1000_u128),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: airdrop_token_contract.to_string(),
                    msg: get_airdrop_claim_msg(),
                    funds: vec![],
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: cw20_token_contract.to_string(),
                    msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                        recipient: scc_address.to_string(),
                        amount: Uint128::new(1000_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                })
            ]
        ));
    }

    #[test]
    fn test_undelegate_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-scc", &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(1000_u128),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::zero(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ZeroUndelegation {}));

        /*
           requested amount is greater than total_staked_tokens
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(2000_u128),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NotEnoughFundsToUndelegate {}))
    }

    #[test]
    fn test_undelegate_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let valid1: Addr = Addr::unchecked("valid0001");
        let valid2: Addr = Addr::unchecked("valid0002");

        /*
           Test - 1. Normal undelegation
        */

        fn get_some_validators_test_1() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_1() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(500, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(500, "uluna".to_string()),
                    can_redelegate: Coin::new(0, "uluna".to_string()),
                    accumulated_rewards: vec![],
                },
            ]
        }

        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_1(),
            &*get_some_delegations_test_1(),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(500_u128),
            },
        )
        .unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(StakingMsg::Undelegate {
                validator: valid1.to_string(),
                amount: Coin::new(500_u128, "uluna")
            }),]
        ));

        /*
           Test - 2. 100% undelegation
        */
        fn get_some_validators_test_2() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_2() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(500, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(500, "uluna".to_string()),
                    can_redelegate: Coin::new(0, "uluna".to_string()),
                    accumulated_rewards: vec![],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_2(),
            &*get_some_delegations_test_2(),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(1000_u128),
            },
        )
        .unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(StakingMsg::Undelegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                }),
                SubMsg::new(StakingMsg::Undelegate {
                    validator: valid2.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                })
            ]
        ));

        /*
            Test - 3. Partial undelegation
        */
        fn get_some_validators_test_3() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_3() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(500, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(500, "uluna".to_string()),
                    can_redelegate: Coin::new(0, "uluna".to_string()),
                    accumulated_rewards: vec![],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_3(),
            &*get_some_delegations_test_3(),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(600_u128),
            },
        )
        .unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(StakingMsg::Undelegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                }),
                SubMsg::new(StakingMsg::Undelegate {
                    validator: valid2.to_string(),
                    amount: Coin::new(100_u128, "uluna")
                })
            ]
        ));

        /*
            Test - 4. Partial undelegation
        */
        fn get_some_validators_test_4() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_some_delegations_test_4() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(500, "uluna".to_string()),
                    can_redelegate: Coin::new(2000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(500, "uluna".to_string()),
                    can_redelegate: Coin::new(0, "uluna".to_string()),
                    accumulated_rewards: vec![],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_some_validators_test_4(),
            &*get_some_delegations_test_4(),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(300_u128),
            },
        )
        .unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(StakingMsg::Undelegate {
                validator: valid1.to_string(),
                amount: Coin::new(300_u128, "uluna")
            }),]
        ));
    }

    #[test]
    fn test_reinvest_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::Reinvest {
                invest_transferred_rewards: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Reinvest {
                invest_transferred_rewards: Some(true),
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "no_rewards_to_reinvest".to_string(),
                value: "1".to_string()
            }]
        ));
    }

    #[test]
    fn test_reinvest_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let _deleg1 = Addr::unchecked("deleg0001".to_string());
        let _deleg2 = Addr::unchecked("deleg0002".to_string());
        let _deleg3 = Addr::unchecked("deleg0003".to_string());
        let valid1 = Addr::unchecked("valid0001".to_string());
        let valid2 = Addr::unchecked("valid0002".to_string());
        let valid3 = Addr::unchecked("valid0003".to_string());

        /*
           Test - 1. First reinvest. Only transferred rewards
        */
        deps.querier.update_staking("test", &*get_validators(), &[]);

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(1000_u128, "uluna")]),
            ExecuteMsg::Reinvest {
                invest_transferred_rewards: Some(true),
            },
        )
        .unwrap();
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(StakingMsg::Delegate {
                validator: valid1.to_string(),
                amount: Coin::new(1000_u128, "uluna")
            }),]
        ));

        /*
           Test - 2. Reinvesting after a few reinvests. Only transferred rewards
        */
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(1000_u128, "uluna")]),
            ExecuteMsg::Reinvest {
                invest_transferred_rewards: Some(true),
            },
        )
        .unwrap();
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(StakingMsg::Delegate {
                validator: valid1.to_string(),
                amount: Coin::new(1000_u128, "uluna")
            }),]
        ));
        /*
           Test - 3. Slashing. Only transferred rewards
        */
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(1000_u128, "uluna")]),
            ExecuteMsg::Reinvest {
                invest_transferred_rewards: Some(true),
            },
        )
        .unwrap();
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(StakingMsg::Delegate {
                validator: valid1.to_string(),
                amount: Coin::new(1000_u128, "uluna")
            })]
        ));

        /*
            Test - 4. transferred_rewards + reward contract rewards
        */
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(1000_u128)));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(1000_u128, "uluna")]),
            ExecuteMsg::Reinvest {
                invest_transferred_rewards: None,
            },
        )
        .unwrap();
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: state.reward_contract_address.to_string(),
                    msg: to_binary(&reward_execute::Transfer {
                        reward_amount: Uint128::new(1000_u128),
                        reward_withdraw_contract: env.contract.address.clone(),
                        protocol_fee_amount: Uint128::zero(),
                        protocol_fee_contract: env.contract.address.clone()
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(2000_u128, "uluna")
                }),
            ]
        ));

        /*
           Test - 5. Reinvest with a jailed validator
        */
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(1000_u128)));

        STATE
            .update(deps.as_mut().storage, |mut state| -> StdResult<_> {
                state.validator_pool = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                Ok(state)
            })
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(1000_u128, "uluna")]),
            ExecuteMsg::Reinvest {
                invest_transferred_rewards: None,
            },
        )
        .unwrap();
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: state.reward_contract_address.to_string(),
                    msg: to_binary(&reward_execute::Transfer {
                        reward_amount: Uint128::new(1000_u128),
                        reward_withdraw_contract: env.contract.address.clone(),
                        protocol_fee_amount: Uint128::zero(),
                        protocol_fee_contract: env.contract.address.clone()
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(2000_u128, "uluna")
                }),
            ]
        ));

        /*
            Test - 6. Just another test
        */
        fn get_validators_test() -> Vec<Validator> {
            vec![
                Validator {
                    address: "valid0001".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
                Validator {
                    address: "valid0002".to_string(),
                    commission: Decimal::zero(),
                    max_commission: Decimal::zero(),
                    max_change_rate: Decimal::zero(),
                },
            ]
        }

        fn get_delegations_test() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(2000, "uluna".to_string()),
                    can_redelegate: Coin::new(1000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(20, "uluna".to_string()),
                        Coin::new(30, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(1000, "uluna".to_string()),
                    can_redelegate: Coin::new(0, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(40, "uluna".to_string()),
                        Coin::new(60, "urew1"),
                    ],
                },
            ]
        }

        deps.querier
            .update_staking("uluna", &*get_validators_test(), &*get_delegations_test());
        deps.querier
            .update_stader_balances(Some(Uint128::new(1000_u128)));

        STATE
            .update(deps.as_mut().storage, |mut state| -> StdResult<_> {
                state.validator_pool = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                Ok(state)
            })
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(1000_u128, "uluna")]),
            ExecuteMsg::Reinvest {
                invest_transferred_rewards: None,
            },
        )
        .unwrap();
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: state.reward_contract_address.to_string(),
                    msg: to_binary(&reward_execute::Transfer {
                        reward_amount: Uint128::new(1000_u128),
                        reward_withdraw_contract: env.contract.address.clone(),
                        protocol_fee_amount: Uint128::zero(),
                        protocol_fee_contract: env.contract.address.clone()
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid2.to_string(),
                    amount: Coin::new(2000_u128, "uluna")
                }),
            ]
        ));
    }

    #[test]
    fn test_transfer_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-scc-contract", &[]),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "no_funds_sent".to_string(),
                value: "1".to_string()
            }]
        ));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[Coin::new(10_u128, "abc"), Coin::new(10_u128, "abc")],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "multiple_coins_passed".to_string(),
                value: "1".to_string()
            }]
        ));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[Coin::new(10_u128, "abc")]),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "transferred_denom_is_wrong".to_string(),
                value: "1".to_string()
            }]
        ));
    }

    #[test]
    fn test_transfer_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. First reinvest
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[Coin::new(100_u128, "uluna")],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(env.contract.address.clone()),
                msg: to_binary(&ExecuteMsg::Reinvest {
                    invest_transferred_rewards: Some(true)
                })
                .unwrap(),
                funds: vec![Coin::new(100_u128, "uluna")]
            })]
        ));

        /*
           Test - 2. Reinvest with existing uninvested_rewards
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[Coin::new(100_u128, "uluna")],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(env.contract.address),
                msg: to_binary(&ExecuteMsg::Reinvest {
                    invest_transferred_rewards: Some(true)
                })
                .unwrap(),
                funds: vec![Coin::new(100_u128, "uluna")]
            })]
        ));
    }

    #[test]
    fn test_redeem_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. Address apart from manager or current contract calling redeem_rewards
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::RedeemRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}))
    }

    #[test]
    fn test_redeem_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemRewards {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0001".to_string(),
                }),
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0002".to_string(),
                })
            ]
        ));
    }
}
