#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{execute, instantiate, query};
    use crate::msg::{
        ExecuteMsg, GetStateResponse, InstantiateMsg, QueryMsg, UpdateUserAirdropsRequest,
    };
    use crate::state::{Cw20TokenContractsInfo, State, UserRewardInfo, CW20_TOKEN_CONTRACTS_REGISTRY, STATE, USER_REWARD_INFO_MAP, STRATEGY_INFO_MAP, StrategyInfo, STRATEGY_METADATA_MAP};
    use crate::test_helpers::check_equal_vec;
    use crate::ContractError;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary, Addr, Binary, Coin, Response, Uint128};

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            strategy_denom: "uluna".to_string(),
        };
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(
            state,
            State {
                manager: info.sender,
                scc_denom: "uluna".to_string(),
                contract_genesis_block_height: env.block.height,
                contract_genesis_timestamp: env.block.time,
                total_accumulated_rewards: Uint128::zero(),
                current_rewards_in_scc: Uint128::zero(),
                total_accumulated_airdrops: vec![]
            }
        );
    }

    #[test]
    fn test__try_update_cw20_contracts_registry_fail() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            strategy_denom: "uluna".to_string(),
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        /*
            Test - 1. Unauthorized
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::RegsiterCW20Contract {
                denom: "anc".to_string(),
                cw20_contract: Addr::unchecked("abc"),
                airdrop_contract: Addr::unchecked("abc"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test__try_update_cw20_contract_registry_success() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            strategy_denom: "uluna".to_string(),
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RegsiterCW20Contract {
                denom: "anc".to_string(),
                cw20_contract: Addr::unchecked("abc"),
                airdrop_contract: Addr::unchecked("def"),
            },
        )
        .unwrap();

        let cw20_contract_info_opt = CW20_TOKEN_CONTRACTS_REGISTRY
            .may_load(deps.as_mut().storage, "anc".to_string())
            .unwrap();
        assert_ne!(cw20_contract_info_opt, None);
        let cw20_contract_info = cw20_contract_info_opt.unwrap();
        assert_eq!(
            cw20_contract_info,
            Cw20TokenContractsInfo {
                airdrop_contract: Addr::unchecked("def"),
                cw20_token_contract: Addr::unchecked("abc")
            }
        );
    }

    #[test]
    fn test__try_claim_airdrops_fail() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            strategy_denom: "uluna".to_string(),
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        fn get_airdrop_claim_msg() -> Binary {
            Binary::from(vec![1, 2, 3, 4, 5, 6, 7, 8, 9])
        }

        /*
           Test - 1. Unauthorized
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: "sid".to_string(),
                amount: Uint128::new(100_u128),
                denom: "anc".to_string(),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Unregistered airdrop
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: "sid".to_string(),
                amount: Uint128::new(100_u128),
                denom: "anc".to_string(),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::AirdropNotRegistered {}));

        /*
           Test - 3. Non-existent strategy
        */
        CW20_TOKEN_CONTRACTS_REGISTRY.save(
            deps.as_mut().storage,
            "anc".to_string(),
            &Cw20TokenContractsInfo {
                airdrop_contract: Addr::unchecked("abc"),
                cw20_token_contract: Addr::unchecked("def"),
            },
        );

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: "sid".to_string(),
                amount: Uint128::new(100_u128),
                denom: "anc".to_string(),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::StrategyInfoDoesNotExist {}));

        /*
           Test - 4. Strategy does not support airdrops
        */
        CW20_TOKEN_CONTRACTS_REGISTRY.save(
            deps.as_mut().storage,
            "anc".to_string(),
            &Cw20TokenContractsInfo {
                airdrop_contract: Addr::unchecked("abc"),
                cw20_token_contract: Addr::unchecked("def"),
            },
        );

        STRATEGY_INFO_MAP.save(deps.as_mut().storage, "anc".to_string(), &StrategyInfo {
            name: "sid".to_string(),
            sic_contract_address: Addr::unchecked("abc"),
            unbonding_period: None,
            supported_airdrops: vec!["mir".to_string()],
            is_active: false
        });
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: "sid".to_string(),
                amount: Uint128::new(100_u128),
                denom: "anc".to_string(),
                claim_msg: get_airdrop_claim_msg(),
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::StrategyDoesNotSupportAirdrop {}));

        /*
            Test - 5. strategy metadata does not exist
         */
        CW20_TOKEN_CONTRACTS_REGISTRY.save(
            deps.as_mut().storage,
            "anc".to_string(),
            &Cw20TokenContractsInfo {
                airdrop_contract: Addr::unchecked("abc"),
                cw20_token_contract: Addr::unchecked("def"),
            },
        );
        STRATEGY_INFO_MAP.save(deps.as_mut().storage, "anc".to_string(), &StrategyInfo {
            name: "sid".to_string(),
            sic_contract_address: Addr::unchecked("abc"),
            unbonding_period: None,
            supported_airdrops: vec!["anc".to_string()],
            is_active: false
        });
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: "sid".to_string(),
                amount: Uint128::new(100_u128),
                denom: "anc".to_string(),
                claim_msg: get_airdrop_claim_msg(),
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::StrategyMetadataDoesNotExist {}));
    }

    #[test]
    fn test__try_update_user_airdrops_fail() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            strategy_denom: "uluna".to_string(),
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        /*
           Test - 1. Unauthorized
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::UpdateUserAirdrops {
                update_user_airdrops_requests: vec![],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Empty request object
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateUserAirdrops {
                update_user_airdrops_requests: vec![],
            },
        )
        .unwrap();
        assert_eq!(res, Response::default());
    }

    #[test]
    fn test__try_update_user_airdrops_success() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            strategy_denom: "uluna".to_string(),
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let user1 = Addr::unchecked("user-1");
        let user2 = Addr::unchecked("user-2");
        let user3 = Addr::unchecked("user-3");
        let user4 = Addr::unchecked("user-4");

        /*
           Test - 1. First airdrops
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateUserAirdrops {
                update_user_airdrops_requests: vec![
                    UpdateUserAirdropsRequest {
                        user: user1.clone(),
                        pool_airdrops: vec![Coin::new(100_u128, "abc"), Coin::new(50_u128, "def")],
                    },
                    UpdateUserAirdropsRequest {
                        user: user2.clone(),
                        pool_airdrops: vec![Coin::new(50_u128, "abc"), Coin::new(50_u128, "def")],
                    },
                    UpdateUserAirdropsRequest {
                        user: user3.clone(),
                        pool_airdrops: vec![Coin::new(200_u128, "abc"), Coin::new(100_u128, "def")],
                    },
                    UpdateUserAirdropsRequest {
                        user: user4.clone(),
                        pool_airdrops: vec![],
                    },
                ],
            },
        )
        .unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert!(check_equal_vec(
            state.total_accumulated_airdrops,
            vec![Coin::new(350_u128, "abc"), Coin::new(200_u128, "def")]
        ));
        let user_reward_info_1_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user_reward_info_1_opt, None);
        let user_reward_info_1 = user_reward_info_1_opt.unwrap();
        assert!(check_equal_vec(
            user_reward_info_1.pending_airdrops,
            vec![Coin::new(100_u128, "abc"), Coin::new(50_u128, "def")]
        ));
        let user_reward_info_2_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user2)
            .unwrap();
        assert_ne!(user_reward_info_2_opt, None);
        let user_reward_info_2 = user_reward_info_2_opt.unwrap();
        assert!(check_equal_vec(
            user_reward_info_2.pending_airdrops,
            vec![Coin::new(50_u128, "abc"), Coin::new(50_u128, "def")]
        ));
        let user_reward_info_3_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user3)
            .unwrap();
        assert_ne!(user_reward_info_3_opt, None);
        let user_reward_info_3 = user_reward_info_3_opt.unwrap();
        assert!(check_equal_vec(
            user_reward_info_3.pending_airdrops,
            vec![Coin::new(200_u128, "abc"), Coin::new(100_u128, "def")]
        ));
        let user_reward_info_4_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user4)
            .unwrap();
        assert_ne!(user_reward_info_4_opt, None);
        let user_reward_info_4 = user_reward_info_4_opt.unwrap();
        assert!(check_equal_vec(user_reward_info_4.pending_airdrops, vec![]));

        /*
           Test - 2. updating the user airdrops with existing user_airdrops
        */
        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.total_accumulated_airdrops =
                    vec![Coin::new(100_u128, "abc"), Coin::new(200_u128, "def")];
                Ok(state)
            },
        );

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                strategies: vec![],
                pending_airdrops: vec![Coin::new(10_u128, "abc"), Coin::new(200_u128, "def")],
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user2,
            &UserRewardInfo {
                strategies: vec![],
                pending_airdrops: vec![Coin::new(20_u128, "abc"), Coin::new(100_u128, "def")],
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user3,
            &UserRewardInfo {
                strategies: vec![],
                pending_airdrops: vec![Coin::new(30_u128, "abc"), Coin::new(50_u128, "def")],
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user4,
            &UserRewardInfo {
                strategies: vec![],
                pending_airdrops: vec![Coin::new(40_u128, "abc"), Coin::new(80_u128, "def")],
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateUserAirdrops {
                update_user_airdrops_requests: vec![
                    UpdateUserAirdropsRequest {
                        user: user1.clone(),
                        pool_airdrops: vec![Coin::new(100_u128, "abc"), Coin::new(50_u128, "def")],
                    },
                    UpdateUserAirdropsRequest {
                        user: user2.clone(),
                        pool_airdrops: vec![Coin::new(50_u128, "abc"), Coin::new(50_u128, "def")],
                    },
                    UpdateUserAirdropsRequest {
                        user: user3.clone(),
                        pool_airdrops: vec![Coin::new(200_u128, "abc"), Coin::new(100_u128, "def")],
                    },
                    UpdateUserAirdropsRequest {
                        user: user4.clone(),
                        pool_airdrops: vec![],
                    },
                ],
            },
        )
        .unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert!(check_equal_vec(
            state.total_accumulated_airdrops,
            vec![Coin::new(450_u128, "abc"), Coin::new(400_u128, "def")]
        ));
        let user_reward_info_1_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user_reward_info_1_opt, None);
        let user_reward_info_1 = user_reward_info_1_opt.unwrap();
        assert!(check_equal_vec(
            user_reward_info_1.pending_airdrops,
            vec![Coin::new(110_u128, "abc"), Coin::new(250_u128, "def")]
        ));
        let user_reward_info_2_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user2)
            .unwrap();
        assert_ne!(user_reward_info_2_opt, None);
        let user_reward_info_2 = user_reward_info_2_opt.unwrap();
        assert!(check_equal_vec(
            user_reward_info_2.pending_airdrops,
            vec![Coin::new(70_u128, "abc"), Coin::new(150_u128, "def")]
        ));
        let user_reward_info_3_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user3)
            .unwrap();
        assert_ne!(user_reward_info_3_opt, None);
        let user_reward_info_3 = user_reward_info_3_opt.unwrap();
        assert!(check_equal_vec(
            user_reward_info_3.pending_airdrops,
            vec![Coin::new(230_u128, "abc"), Coin::new(150_u128, "def")]
        ));
        let user_reward_info_4_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user4)
            .unwrap();
        assert_ne!(user_reward_info_4_opt, None);
        let user_reward_info_4 = user_reward_info_4_opt.unwrap();
        assert!(check_equal_vec(
            user_reward_info_4.pending_airdrops,
            vec![Coin::new(40_u128, "abc"), Coin::new(80_u128, "def")]
        ));
    }
}
