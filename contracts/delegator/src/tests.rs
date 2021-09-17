#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{execute, instantiate, query, reply, reply_remove_validator};
    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, GetConfigResponse, InstantiateMsg, QueryMsg};
    use crate::state::{Config, State, CONFIG, STATE, USER_REGISTRY, DepositInfo, UserPoolInfo, UndelegationInfo};
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
        MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{coins, from_binary, to_binary, Addr, Attribute, BankMsg, Binary, Coin, ContractResult, Decimal, DistributionMsg, Empty, Env, Event, FullDelegation, MessageInfo, OwnedDeps, Reply, Response, StakingMsg, SubMsg, SubMsgExecutionResponse, Uint128, Validator, WasmMsg, attr};
    use cw20::Cw20ExecuteMsg;
    use stader_utils::coin_utils::{check_equal_coin_vector, DecCoin, check_equal_deccoin_vector};
    use terra_cosmwasm::{TerraMsg, TerraMsgWrapper};
    use cw_storage_plus::U64Key;
    use crate::msg::ExecuteMsg::Deposit;

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
            Validator {
                address: "valid0003".to_string(),
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
                amount: Coin::new(1000, "utest"),
                can_redelegate: Coin::new(1000, "utest"),
                accumulated_rewards: vec![Coin::new(20, "utest"), Coin::new(30, "urew1")],
            },
            FullDelegation {
                delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                validator: "valid0002".to_string(),
                amount: Coin::new(1000, "utest"),
                can_redelegate: Coin::new(0, "utest"),
                accumulated_rewards: vec![Coin::new(40, "utest"), Coin::new(60, "urew1")],
            },
            FullDelegation {
                delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                validator: "valid0003".to_string(),
                amount: Coin::new(0, "utest"),
                can_redelegate: Coin::new(0, "utest"),
                accumulated_rewards: vec![],
            },
        ]
    }

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
        };
        let expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "utest".to_string(),
            pools_contract: Addr::unchecked("pools_address"),
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
    fn test_deposit() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let pools_info = mock_info("pools_addr", &[]);
        let user1 = Addr::unchecked("user0001");
        let user2 = Addr::unchecked("user0002");

        instantiate_contract(&mut deps, &info, &env, None);
        let initial_airdrop_pointer = vec![
            DecCoin::new(Decimal::from_ratio(6_u128, 60_u128), "uair1"),
            DecCoin::new(Decimal::from_ratio(18_u128, 60_u128), "uair2")
        ];
        let initial_rewards_pointer = Decimal::from_ratio(12_u128, 60_u128);
        let initial_msg = ExecuteMsg::Deposit {
            user_addr: user1.clone(),
            pool_id: 0,
            amount: Uint128::new(30),
            pool_rewards_pointer: Decimal::from_ratio(12_u128, 60_u128),
            pool_airdrops_pointer: initial_airdrop_pointer.clone()
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
                pool_airdrops_pointer: initial_airdrop_pointer.clone()
            }
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::ZeroAmount {}));

        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone(),
        )
            .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert!(res.attributes.len() == 3);
        assert_eq!(res.attributes[0], attr("deposit_amount", "30"));
        assert_eq!(res.attributes[1], attr("user_addr", user1.clone().to_string()));
        assert_eq!(res.attributes[2], attr("deposit_pool", "0"));

        let user1_info = USER_REGISTRY
            .load(deps.as_mut().storage, (&user1, U64Key::new(0)))
            .unwrap();
        assert_eq!(user1_info.deposit, DepositInfo {
            staked: Uint128::new(30)
        });
        assert!(check_equal_deccoin_vector(&user1_info.airdrops_pointer, &initial_airdrop_pointer));
        assert!(user1_info.rewards_pointer.eq(&initial_rewards_pointer.clone()));
        assert!(user1_info.pending_airdrops.is_empty());
        assert!(user1_info.pending_rewards.is_zero());
        assert!(user1_info.redelegations.is_empty());
        assert!(user1_info.undelegations.is_empty());

        let next_airdrop_pointer = vec![
            DecCoin::new(Decimal::from_ratio(36_u128, 60_u128), "uair1"),
            DecCoin::new(Decimal::from_ratio(36_u128, 60_u128), "uair2")
        ];
        let next_reward_pointer = Decimal::from_ratio(48_u128, 60_u128);
        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Deposit {
                user_addr: user1.clone(),
                pool_id: 0,
                amount: Uint128::new(50),
                pool_rewards_pointer: next_reward_pointer.clone(),
                pool_airdrops_pointer: next_airdrop_pointer.clone()
            }
        ).unwrap();
        assert_eq!(res.messages.len(), 0);
        assert!(res.attributes.len() == 3);
        assert_eq!(res.attributes[0], attr("deposit_amount", "50"));
        assert_eq!(res.attributes[1], attr("user_addr", user1.clone().to_string()));
        assert_eq!(res.attributes[2], attr("deposit_pool", "0"));

        let user1_info = USER_REGISTRY
            .load(deps.as_mut().storage, (&user1, U64Key::new(0)))
            .unwrap();
        assert_eq!(user1_info.deposit, DepositInfo {
            staked: Uint128::new(80)
        });

        assert!(check_equal_deccoin_vector(&user1_info.airdrops_pointer, &next_airdrop_pointer.clone()));
        assert!(user1_info.rewards_pointer.eq(&next_reward_pointer.clone()));
        assert!(check_equal_coin_vector(&user1_info.pending_airdrops, &vec![
            Coin::new(15, "uair1"), Coin::new(9, "uair2")
        ]));
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
        let user2 = Addr::unchecked("user0002");

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_airdrop_pointer = vec![
            DecCoin::new(Decimal::from_ratio(6_u128, 60_u128), "uair1"),
            DecCoin::new(Decimal::from_ratio(18_u128, 60_u128), "uair2")
        ];
        let initial_rewards_pointer = Decimal::from_ratio(12_u128, 60_u128);
        let next_airdrop_pointer = vec![
            DecCoin::new(Decimal::from_ratio(36_u128, 60_u128), "uair1"),
            DecCoin::new(Decimal::from_ratio(36_u128, 60_u128), "uair2")
        ];
        let next_reward_pointer = Decimal::from_ratio(48_u128, 60_u128);
        let initial_msg = ExecuteMsg::Undelegate {
            user_addr: user1.clone(),
            batch_id: 13,
            amount: Uint128::new(20),
            pool_rewards_pointer: next_reward_pointer.clone(),
            pool_airdrops_pointer: next_airdrop_pointer.clone(),
            from_pool: 22
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
                from_pool: 0
            }
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::ZeroAmount {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone()
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::UserNotFound {}));


        let mut init_user_info = UserPoolInfo {
            deposit: DepositInfo { staked: Uint128::new(10) },
            airdrops_pointer: initial_airdrop_pointer.clone(),
            pending_airdrops: vec![],
            rewards_pointer: initial_rewards_pointer.clone(),
            pending_rewards: Uint128::zero(),
            redelegations: vec![],
            undelegations: vec![]
        };
        USER_REGISTRY.save(deps.as_mut().storage, (&user1, U64Key::new(22)), &init_user_info);

        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone()
        ).unwrap_err();
        assert!(matches!(err, ContractError::InSufficientFunds {}));

        init_user_info.deposit.staked = Uint128::new(30);
        USER_REGISTRY.save(deps.as_mut().storage, (&user1, U64Key::new(22)), &init_user_info);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone()
        ).unwrap();

        assert_eq!(res.messages.len(), 0);
        assert!(res.attributes.len() == 3);
        assert_eq!(res.attributes[0], attr("undelegate_amount", "20"));
        assert_eq!(res.attributes[1], attr("from_pool", "22"));
        assert_eq!(res.attributes[2], attr("user_addr", user1.clone().to_string()));

        let user1_info = USER_REGISTRY
            .load(deps.as_mut().storage, (&user1, U64Key::new(22)))
            .unwrap();
        assert_eq!(user1_info.deposit, DepositInfo {
            staked: Uint128::new(10)
        });

        assert!(check_equal_deccoin_vector(&user1_info.airdrops_pointer, &next_airdrop_pointer.clone()));
        assert!(user1_info.rewards_pointer.eq(&next_reward_pointer.clone()));
        assert!(check_equal_coin_vector(&user1_info.pending_airdrops, &vec![
            Coin::new(15, "uair1"), Coin::new(9, "uair2")
        ]));
        assert!(user1_info.pending_rewards.eq(&Uint128::new(18)));
        assert!(user1_info.redelegations.is_empty());
        assert_eq!(user1_info.undelegations.len(), 1);
        assert_eq!(user1_info.undelegations[0], UndelegationInfo {
            batch_id: 13,
            id: 1,
            amount: Uint128::new(20),
            pool_id: 22
        });
    }

    #[test]
    fn test_withdraw_funds() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let pools_info = mock_info("pools_addr", &[]);
        let user1 = Addr::unchecked("user0001");
        let user2 = Addr::unchecked("user0002");

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_msg = ExecuteMsg::WithdrawFunds {
            user_addr: user1.clone(),
            pool_id: 22,
            amount: Uint128::new(20),
            undelegate_id: 27
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
            deposit: DepositInfo { staked: Uint128::new(10) },
            airdrops_pointer: vec![],
            pending_airdrops: vec![],
            rewards_pointer: Decimal::zero(),
            pending_rewards: Uint128::zero(),
            redelegations: vec![],
            undelegations: vec![]
        };
        USER_REGISTRY.save(deps.as_mut().storage, (&user1, U64Key::new(22)), &init_user_info);

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
            pool_id: 22
        });
        USER_REGISTRY.save(deps.as_mut().storage, (&user1, U64Key::new(22)), &init_user_info);

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
            pool_id: 22
        });
        init_user_info.undelegations.push(UndelegationInfo {
            batch_id: 88,
            id: 28,
            amount: Uint128::new(10),
            pool_id: 22
        });
        USER_REGISTRY.save(deps.as_mut().storage, (&user1, U64Key::new(22)), &init_user_info);


        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            initial_msg.clone(),
        )
            .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0], SubMsg::new(BankMsg::Send { to_address: user1.to_string(), amount: vec![Coin::new(20, "utest")] }));
        let user_info = USER_REGISTRY.load(deps.as_mut().storage, (&user1, U64Key::new(22))).unwrap();
        assert_eq!(user_info.undelegations.len(), 1);
        assert_eq!(user_info.undelegations[0], UndelegationInfo {
            batch_id: 88,
            id: 28,
            amount: Uint128::new(10),
            pool_id: 22
        });
    }
}
