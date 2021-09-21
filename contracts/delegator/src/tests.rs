#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};
    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, GetConfigResponse, InstantiateMsg, QueryMsg};
    use crate::state::{
        Config, DepositInfo, PoolPointerInfo, UndelegationInfo, UserPoolInfo, CONFIG, USER_REGISTRY,
    };
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use cosmwasm_std::{
        attr, from_binary, Addr, BankMsg, Coin, Decimal, Env, MessageInfo, OwnedDeps, Response,
        SubMsg, Uint128,
    };
    use cw_storage_plus::U64Key;
    use stader_utils::coin_utils::{check_equal_coin_vector, check_equal_deccoin_vector, DecCoin};
    use terra_cosmwasm::TerraMsgWrapper;

    pub fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
        info: &MessageInfo,
        env: &Env,
        vault_denom: Option<String>,
    ) -> Response<TerraMsgWrapper> {
        let instantiate_msg = InstantiateMsg {
            vault_denom: vault_denom.unwrap_or_else(|| "utest".to_string()),
            pools_contract: Addr::unchecked("pools_addr"),
            scc_contract: Addr::unchecked("scc_addr"),
            protocol_fee: Decimal::from_ratio(1_u128, 1000_u128), // 0.1%
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            vault_denom: "utest".to_string(),
            pools_contract: Addr::unchecked("pools_address"),
            scc_contract: Addr::unchecked("scc_addr"),
            protocol_fee: Decimal::from_ratio(1_u128, 1000_u128), // 0.1%
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
        };
        let expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "utest".to_string(),
            pools_contract: Addr::unchecked("pools_address"),
            scc_contract: Addr::unchecked("scc_addr"),
            protocol_fee: Decimal::from_ratio(1_u128, 1000_u128),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
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
    fn test_deposit() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let pools_info = mock_info("pools_addr", &[]);
        let user1 = Addr::unchecked("user0001");

        instantiate_contract(&mut deps, &info, &env, None);
        let initial_airdrop_pointer = vec![
            DecCoin::new(Decimal::from_ratio(6_u128, 60_u128), "uair1"),
            DecCoin::new(Decimal::from_ratio(18_u128, 60_u128), "uair2"),
        ];
        let initial_rewards_pointer = Decimal::from_ratio(12_u128, 60_u128);
        let initial_msg = ExecuteMsg::Deposit {
            user_addr: user1.clone(),
            pool_id: 0,
            amount: Uint128::new(30),
            pool_rewards_pointer: Decimal::from_ratio(12_u128, 60_u128),
            pool_airdrops_pointer: initial_airdrop_pointer.clone(),
        };
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[Coin::new(12, "utest")]),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Deposit {
                user_addr: user1.clone(),
                pool_id: 0,
                amount: Uint128::new(0),
                pool_rewards_pointer: initial_rewards_pointer.clone(),
                pool_airdrops_pointer: initial_airdrop_pointer.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ZeroAmount {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone(),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert!(res.attributes.len() == 3);
        assert_eq!(res.attributes[0], attr("deposit_amount", "30"));
        assert_eq!(
            res.attributes[1],
            attr("user_addr", user1.clone().to_string())
        );
        assert_eq!(res.attributes[2], attr("deposit_pool", "0"));

        let user1_info = USER_REGISTRY
            .load(deps.as_mut().storage, (&user1, U64Key::new(0)))
            .unwrap();
        assert_eq!(
            user1_info.deposit,
            DepositInfo {
                staked: Uint128::new(30)
            }
        );
        assert!(check_equal_deccoin_vector(
            &user1_info.airdrops_pointer,
            &initial_airdrop_pointer
        ));
        assert!(user1_info
            .rewards_pointer
            .eq(&initial_rewards_pointer.clone()));
        assert!(user1_info.pending_airdrops.is_empty());
        assert!(user1_info.pending_rewards.is_zero());
        assert!(user1_info.redelegations.is_empty());
        assert!(user1_info.undelegations.is_empty());

        let next_airdrop_pointer = vec![
            DecCoin::new(Decimal::from_ratio(36_u128, 60_u128), "uair1"),
            DecCoin::new(Decimal::from_ratio(36_u128, 60_u128), "uair2"),
        ];
        let next_reward_pointer = Decimal::from_ratio(48_u128, 60_u128);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Deposit {
                user_addr: user1.clone(),
                pool_id: 0,
                amount: Uint128::new(50),
                pool_rewards_pointer: next_reward_pointer.clone(),
                pool_airdrops_pointer: next_airdrop_pointer.clone(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert!(res.attributes.len() == 3);
        assert_eq!(res.attributes[0], attr("deposit_amount", "50"));
        assert_eq!(
            res.attributes[1],
            attr("user_addr", user1.clone().to_string())
        );
        assert_eq!(res.attributes[2], attr("deposit_pool", "0"));

        let user1_info = USER_REGISTRY
            .load(deps.as_mut().storage, (&user1, U64Key::new(0)))
            .unwrap();
        assert_eq!(
            user1_info.deposit,
            DepositInfo {
                staked: Uint128::new(80)
            }
        );

        assert!(check_equal_deccoin_vector(
            &user1_info.airdrops_pointer,
            &next_airdrop_pointer.clone()
        ));
        assert!(user1_info.rewards_pointer.eq(&next_reward_pointer.clone()));
        assert!(check_equal_coin_vector(
            &user1_info.pending_airdrops,
            &vec![Coin::new(15, "uair1"), Coin::new(9, "uair2")]
        ));
        assert!(user1_info.pending_rewards.eq(&Uint128::new(18)));
        assert!(user1_info.redelegations.is_empty());
        assert!(user1_info.undelegations.is_empty());
    }

    #[test]
    fn test_undelegate() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let pools_info = mock_info("pools_addr", &[]);
        let user1 = Addr::unchecked("user0001");

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_airdrop_pointer = vec![
            DecCoin::new(Decimal::from_ratio(6_u128, 60_u128), "uair1"),
            DecCoin::new(Decimal::from_ratio(18_u128, 60_u128), "uair2"),
        ];
        let initial_rewards_pointer = Decimal::from_ratio(12_u128, 60_u128);
        let next_airdrop_pointer = vec![
            DecCoin::new(Decimal::from_ratio(36_u128, 60_u128), "uair1"),
            DecCoin::new(Decimal::from_ratio(36_u128, 60_u128), "uair2"),
        ];
        let next_reward_pointer = Decimal::from_ratio(48_u128, 60_u128);
        let initial_msg = ExecuteMsg::Undelegate {
            user_addr: user1.clone(),
            batch_id: 13,
            amount: Uint128::new(20),
            pool_rewards_pointer: next_reward_pointer.clone(),
            pool_airdrops_pointer: next_airdrop_pointer.clone(),
            from_pool: 22,
        };
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[Coin::new(12, "utest")]),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Undelegate {
                user_addr: user1.clone(),
                batch_id: 0,
                amount: Uint128::new(0),
                pool_rewards_pointer: Decimal::from_ratio(12_u128, 60_u128),
                pool_airdrops_pointer: initial_airdrop_pointer.clone(),
                from_pool: 0,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ZeroAmount {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserNotFound {}));

        let mut init_user_info = UserPoolInfo {
            pool_id: 22,
            deposit: DepositInfo {
                staked: Uint128::new(10),
            },
            airdrops_pointer: initial_airdrop_pointer.clone(),
            pending_airdrops: vec![],
            rewards_pointer: initial_rewards_pointer.clone(),
            pending_rewards: Uint128::zero(),
            redelegations: vec![],
            undelegations: vec![],
        };
        USER_REGISTRY
            .save(
                deps.as_mut().storage,
                (&user1, U64Key::new(22)),
                &init_user_info,
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InSufficientFunds {}));

        init_user_info.deposit.staked = Uint128::new(30);
        USER_REGISTRY
            .save(
                deps.as_mut().storage,
                (&user1, U64Key::new(22)),
                &init_user_info,
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone(),
        )
        .unwrap();

        assert_eq!(res.messages.len(), 0);
        assert!(res.attributes.len() == 3);
        assert_eq!(res.attributes[0], attr("undelegate_amount", "20"));
        assert_eq!(res.attributes[1], attr("from_pool", "22"));
        assert_eq!(
            res.attributes[2],
            attr("user_addr", user1.clone().to_string())
        );

        let user1_info = USER_REGISTRY
            .load(deps.as_mut().storage, (&user1, U64Key::new(22)))
            .unwrap();
        assert_eq!(
            user1_info.deposit,
            DepositInfo {
                staked: Uint128::new(10)
            }
        );

        assert!(check_equal_deccoin_vector(
            &user1_info.airdrops_pointer,
            &next_airdrop_pointer.clone()
        ));
        assert!(user1_info.rewards_pointer.eq(&next_reward_pointer.clone()));
        assert!(check_equal_coin_vector(
            &user1_info.pending_airdrops,
            &vec![Coin::new(15, "uair1"), Coin::new(9, "uair2")]
        ));
        assert!(user1_info.pending_rewards.eq(&Uint128::new(18)));
        assert!(user1_info.redelegations.is_empty());
        assert_eq!(user1_info.undelegations.len(), 1);
        assert_eq!(
            user1_info.undelegations[0],
            UndelegationInfo {
                batch_id: 13,
                id: 1,
                amount: Uint128::new(20),
                pool_id: 22
            }
        );
    }

    #[test]
    fn test_withdraw_funds() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let pools_info = mock_info("pools_addr", &[]);
        let user1 = Addr::unchecked("user0001");

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_msg = ExecuteMsg::WithdrawFunds {
            user_addr: user1.clone(),
            pool_id: 22,
            amount: Uint128::new(20),
            undelegate_id: 27,
        };
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[Coin::new(14, "utest")]),
            initial_msg.clone(),
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::FundsNotExpected {}));
        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserNotFound {}));

        let mut init_user_info = UserPoolInfo {
            pool_id: 22,
            deposit: DepositInfo {
                staked: Uint128::new(10),
            },
            airdrops_pointer: vec![],
            pending_airdrops: vec![],
            rewards_pointer: Decimal::zero(),
            pending_rewards: Uint128::zero(),
            redelegations: vec![],
            undelegations: vec![],
        };
        USER_REGISTRY
            .save(
                deps.as_mut().storage,
                (&user1, U64Key::new(22)),
                &init_user_info,
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::RecordNotFound {}));
        init_user_info.undelegations.push(UndelegationInfo {
            batch_id: 88,
            id: 27,
            amount: Uint128::new(10),
            pool_id: 22,
        });
        USER_REGISTRY
            .save(
                deps.as_mut().storage,
                (&user1, U64Key::new(22)),
                &init_user_info,
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone(),
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::NonMatchingAmount {}));
        init_user_info.undelegations = vec![]; // Remove incorrect entry and add the right one.
        init_user_info.undelegations.push(UndelegationInfo {
            batch_id: 88,
            id: 27,
            amount: Uint128::new(20),
            pool_id: 22,
        });
        init_user_info.undelegations.push(UndelegationInfo {
            batch_id: 88,
            id: 28,
            amount: Uint128::new(10),
            pool_id: 22,
        });
        init_user_info.undelegations.push(UndelegationInfo {
            batch_id: 88,
            id: 29,
            amount: Uint128::new(3000),
            pool_id: 22,
        });
        USER_REGISTRY
            .save(
                deps.as_mut().storage,
                (&user1, U64Key::new(22)),
                &init_user_info,
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone(),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(BankMsg::Send {
                to_address: user1.to_string(),
                amount: vec![Coin::new(20, "utest")]
            })
        );
        let user_info = USER_REGISTRY
            .load(deps.as_mut().storage, (&user1, U64Key::new(22)))
            .unwrap();
        assert_eq!(user_info.undelegations.len(), 2);
        assert_eq!(
            user_info.undelegations[0],
            UndelegationInfo {
                batch_id: 88,
                id: 28,
                amount: Uint128::new(10),
                pool_id: 22
            }
        );

        let undel2 = ExecuteMsg::WithdrawFunds {
            user_addr: user1.clone(),
            pool_id: 22,
            amount: Uint128::new(3000),
            undelegate_id: 29,
        };
        let res = execute(deps.as_mut(), env.clone(), pools_info.clone(), undel2).unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages[0],
            SubMsg::new(BankMsg::Send {
                to_address: "protocol_fee_addr".to_string(),
                amount: vec![Coin::new(3, "utest")]
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(BankMsg::Send {
                to_address: user1.to_string(),
                amount: vec![Coin::new(2997, "utest")]
            })
        );
    }

    #[test]
    fn test_allocate_rewards() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let user1 = Addr::unchecked("user0001");
        let user2 = Addr::unchecked("user0002");
        let user3 = Addr::unchecked("user0003");

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_msg = ExecuteMsg::AllocateRewards {
            user_addrs: vec![user1.clone(), user2.clone(), user3.clone()],
            pool_pointers: vec![],
        };
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(14, "utest")]),
            initial_msg.clone(),
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::FundsNotExpected {}));

        let airdrop1_p = DecCoin::new(Decimal::from_ratio(9_u128, 100_u128), "uair1");
        let airdrop2_p = DecCoin::new(Decimal::from_ratio(27_u128, 100_u128), "uair1");
        let airdrop3_p = DecCoin::new(Decimal::from_ratio(27_u128, 100_u128), "uair2");
        let airdrop4_p = DecCoin::new(Decimal::from_ratio(45_u128, 100_u128), "uair2");

        let airdrop1 = Coin::new(10, "uair1");
        let airdrop3 = Coin::new(30, "uair2");

        let reward1_p = Decimal::from_ratio(12_u128, 100_u128);
        let reward2_p = Decimal::from_ratio(24_u128, 100_u128);
        let reward3_p = Decimal::from_ratio(36_u128, 100_u128);

        let reward1 = Uint128::new(5);
        let reward2 = Uint128::new(15);

        let init_user_info1_1 = UserPoolInfo {
            pool_id: 1,
            deposit: DepositInfo {
                staked: Uint128::new(10),
            },
            airdrops_pointer: vec![airdrop1_p.clone(), airdrop3_p.clone()],
            pending_airdrops: vec![airdrop1.clone(), airdrop3.clone()],
            rewards_pointer: reward1_p,
            pending_rewards: reward1,
            redelegations: vec![],
            undelegations: vec![],
        };
        let init_user_info1_2 = UserPoolInfo {
            pool_id: 2,
            deposit: DepositInfo {
                staked: Uint128::new(20),
            },
            airdrops_pointer: vec![airdrop1_p.clone()],
            pending_airdrops: vec![airdrop1.clone()],
            rewards_pointer: reward1_p,
            pending_rewards: reward1,
            redelegations: vec![],
            undelegations: vec![],
        };
        let init_user_info2_1 = UserPoolInfo {
            pool_id: 1,
            deposit: DepositInfo {
                staked: Uint128::new(30),
            },
            airdrops_pointer: vec![airdrop1_p.clone()],
            pending_airdrops: vec![airdrop1.clone()],
            rewards_pointer: reward2_p,
            pending_rewards: reward2,
            redelegations: vec![],
            undelegations: vec![],
        };
        let init_user_info2_3 = UserPoolInfo {
            pool_id: 3,
            deposit: DepositInfo {
                staked: Uint128::new(40),
            },
            airdrops_pointer: vec![airdrop3_p.clone()],
            pending_airdrops: vec![airdrop3.clone()],
            rewards_pointer: reward1_p,
            pending_rewards: reward2,
            redelegations: vec![],
            undelegations: vec![],
        };
        let init_user_info3_3 = UserPoolInfo {
            pool_id: 3,
            deposit: DepositInfo {
                staked: Uint128::new(50),
            },
            airdrops_pointer: vec![airdrop3_p.clone()],
            pending_airdrops: vec![airdrop3.clone()],
            rewards_pointer: reward1_p,
            pending_rewards: reward2,
            redelegations: vec![],
            undelegations: vec![],
        };

        USER_REGISTRY
            .save(
                deps.as_mut().storage,
                (&user1, U64Key::new(1)),
                &init_user_info1_1,
            )
            .unwrap();
        USER_REGISTRY
            .save(
                deps.as_mut().storage,
                (&user1, U64Key::new(2)),
                &init_user_info1_2,
            )
            .unwrap();
        USER_REGISTRY
            .save(
                deps.as_mut().storage,
                (&user2, U64Key::new(1)),
                &init_user_info2_1,
            )
            .unwrap();
        USER_REGISTRY
            .save(
                deps.as_mut().storage,
                (&user2, U64Key::new(3)),
                &init_user_info2_3,
            )
            .unwrap();
        USER_REGISTRY
            .save(
                deps.as_mut().storage,
                (&user3, U64Key::new(3)),
                &init_user_info3_3,
            )
            .unwrap();

        let initial_msg = ExecuteMsg::AllocateRewards {
            user_addrs: vec![user1.clone(), user2.clone(), user3.clone()],
            pool_pointers: vec![
                PoolPointerInfo {
                    pool_id: 1,
                    airdrops_pointer: vec![airdrop2_p.clone(), airdrop4_p.clone()],
                    rewards_pointer: reward3_p.clone(),
                },
                PoolPointerInfo {
                    pool_id: 2,
                    airdrops_pointer: vec![airdrop2_p.clone(), airdrop4_p.clone()],
                    rewards_pointer: reward3_p.clone(),
                },
            ],
        };
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            initial_msg.clone(),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);

        let user_info1_1 = USER_REGISTRY
            .load(deps.as_mut().storage, (&user1, U64Key::new(1)))
            .unwrap();
        assert!(user_info1_1.pending_rewards.is_zero());
        assert!(user_info1_1.pending_airdrops.is_empty());

        let user_info1_2 = USER_REGISTRY
            .load(deps.as_mut().storage, (&user1, U64Key::new(2)))
            .unwrap();
        assert!(user_info1_2.pending_rewards.is_zero());
        assert!(user_info1_2.pending_airdrops.is_empty());

        let user_info2_1 = USER_REGISTRY
            .load(deps.as_mut().storage, (&user2, U64Key::new(1)))
            .unwrap();
        assert!(user_info2_1.pending_rewards.is_zero());
        assert!(user_info2_1.pending_airdrops.is_empty());

        let user_info2_3 = USER_REGISTRY
            .load(deps.as_mut().storage, (&user2, U64Key::new(3)))
            .unwrap();
        assert!(!user_info2_3.pending_rewards.is_zero());
        assert!(!user_info2_3.pending_airdrops.is_empty());

        let user_info3_3 = USER_REGISTRY
            .load(deps.as_mut().storage, (&user3, U64Key::new(3)))
            .unwrap();
        assert!(!user_info3_3.pending_rewards.is_zero());
        assert!(!user_info3_3.pending_airdrops.is_empty());

        /* Please verify by printing this out. Cannot check for binary equality */

        // assert_eq!(res.messages[1], SubMsg::new(WasmMsg::Execute {
        //     contract_addr: "scc_addr".to_string(),
        //     msg: to_binary(&SccMsg::UpdateUserAirdrops {
        //         update_user_airdrops_requests: vec![UpdateUserAirdropsRequest { // Pool 1
        //             user: user1.clone(),
        //             pool_airdrops: vec![Coin::new(11, "uair1"), Coin::new(31, "uair2")]
        //         }, UpdateUserAirdropsRequest { // Pool 2
        //             user: user1.clone(),
        //             pool_airdrops: vec![Coin::new(13, "uair1"), Coin::new(9, "uair2")]
        //         }, UpdateUserAirdropsRequest { // Pool 1
        //             user: user2.clone(),
        //             pool_airdrops: vec![Coin::new(15, "uair1"), Coin::new(13, "uair2")]
        //         }]
        //     }).unwrap(),
        //     funds: vec![]
        // }));
        //
        // assert_eq!(res.messages[0], SubMsg::new(WasmMsg::Execute {
        //     contract_addr: "scc_addr".to_string(),
        //     msg: to_binary(&SccMsg::UpdateUserRewards {
        //         update_user_rewards_requests: vec![UpdateUserRewardsRequest { // Pool 1
        //             user: user1.clone(),
        //             funds: Uint128::new(7),
        //             strategy_id: None
        //         }, UpdateUserRewardsRequest { // Pool 2
        //             user: user1.clone(),
        //             funds: Uint128::new(9),
        //             strategy_id: None
        //         }, UpdateUserRewardsRequest { // Pool 1
        //             user: user2.clone(),
        //             funds: Uint128::new(18),
        //             strategy_id: None
        //         }]
        //     }).unwrap(),
        //     funds: vec![]
        // }));
    }

    #[test]
    fn test_update_config() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_msg = ExecuteMsg::UpdateConfig {
            pools_contract: None,
            scc_contract: None,
            protocol_fee: None,
            protocol_fee_contract: None,
        };
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(14, "utest")]),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        let mut expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "utest".to_string(),
            pools_contract: Addr::unchecked("pools_addr"),
            scc_contract: Addr::unchecked("scc_addr"),
            protocol_fee: Decimal::from_ratio(1_u128, 1000_u128),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
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
            vault_denom: "utest".to_string(),
            pools_contract: Addr::unchecked("new_pools_addr"),
            scc_contract: Addr::unchecked("new_scc_addr"),
            protocol_fee: Decimal::from_ratio(2_u128, 1000_u128),
            protocol_fee_contract: Addr::unchecked("new_protocol_fee_addr"),
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                pools_contract: Some(Addr::unchecked("new_pools_addr")),
                scc_contract: Some(Addr::unchecked("new_scc_addr")),
                protocol_fee: Some(Decimal::from_ratio(2_u128, 1000_u128)),
                protocol_fee_contract: Some(Addr::unchecked("new_protocol_fee_addr")),
            }
            .clone(),
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config, expected_config);
    }
}
