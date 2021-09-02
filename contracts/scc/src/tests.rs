#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{execute, instantiate, query};
    use crate::helpers::get_sic_total_tokens;
    use crate::msg::{
        ExecuteMsg, GetStateResponse, InstantiateMsg, QueryMsg, UpdateUserAirdropsRequest,
        UpdateUserRewardsRequest,
    };
    use crate::state::{
        State, StrategyInfo, UserRewardInfo, UserStrategyInfo, STATE, STRATEGY_MAP,
        USER_REWARD_INFO_MAP,
    };
    use crate::test_helpers::{check_equal_reward_info, check_equal_user_strategies};
    use crate::ContractError;
    use cosmwasm_std::{
        coins, from_binary, to_binary, Addr, Coin, Decimal, Empty, Env, MessageInfo, OwnedDeps,
        QuerierWrapper, Response, StdResult, SubMsg, Uint128, WasmMsg,
    };
    use sic_base::msg::{ExecuteMsg as sic_execute_msg, QueryMsg as sic_query_msg};
    use stader_utils::coin_utils::DecCoin;
    use stader_utils::mock::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use stader_utils::test_helpers::check_equal_vec;
    use std::collections::HashMap;

    fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
        info: &MessageInfo,
        env: &Env,
        strategy_denom: Option<String>,
        pools_contract: Option<String>,
    ) -> Response<Empty> {
        let msg = InstantiateMsg {
            strategy_denom: strategy_denom.unwrap_or("uluna".to_string()),
            pools_contract: Addr::unchecked(pools_contract.unwrap_or("pools_contract".to_string())),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

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
                pool_contract: Addr::unchecked("pools_contract"),
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
    fn test__try_update_user_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        /*
           Test - 1. Unauthorized
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
            Test - 2. 0 user requests
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![],
            },
        )
        .unwrap();
        assert_eq!(res, Response::default());
    }

    #[test]
    fn test__try_update_user_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        let user1 = Addr::unchecked("user1");
        let user2 = Addr::unchecked("user2");
        let user3 = Addr::unchecked("user3");
        let user4 = Addr::unchecked("user4");
        let sid1_sic_address = Addr::unchecked("sid1_sic_address");
        let sid2_sic_address = Addr::unchecked("sid2_sic_address");
        let sid3_sic_address = Addr::unchecked("sid3_sic_address");

        /*
           Test - 1. New user rewards
        */

        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sid1_sic_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(contracts_to_token);

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1.clone(),
            &UserRewardInfo {
                strategies: vec![],
                pending_airdrops: vec![],
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::new("sid1".to_string(), sid1_sic_address.clone(), None, vec![]),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    rewards: Uint128::new(500_u128),
                    strategy_id: "sid1".to_string(),
                }],
            },
        )
        .unwrap();

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_accumulated_rewards, Uint128::new(500_u128));

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(sid1_sic_address.clone()),
                msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                funds: vec![Coin::new(500_u128, state.scc_denom.clone())]
            }),]
        ));

        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.strategies.len(), 1);
        assert!(check_equal_vec(
            user1_reward_info.strategies,
            vec![UserStrategyInfo {
                strategy_name: "sid1".to_string(),
                shares: Decimal::from_ratio(5000_u128, 1_u128),
                airdrop_pointer: vec![]
            }]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(
            sid1_strategy_info.total_shares,
            Decimal::from_ratio(5000_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy_info.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        /*
           Test - 2. user rewards are deposited into a strategy again
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sid1_sic_address.clone(), Uint128::new(500_u128));
        deps.querier.update_wasm(contracts_to_token);

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.total_accumulated_rewards = Uint128::new(500_u128);
            Ok(state)
        });

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sid1_sic_address.clone(),
                unbonding_period: None,
                supported_airdrops: vec![],
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                current_unprocessed_undelegations: Default::default(),
            },
        );

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1.clone(),
            &UserRewardInfo {
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(5000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    rewards: Uint128::new(500_u128),
                    strategy_id: "sid1".to_string(),
                }],
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(sid1_sic_address.clone()),
                msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                funds: vec![Coin::new(500_u128, state.scc_denom.clone())]
            })]
        ));

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_accumulated_rewards, Uint128::new(1000_u128));

        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.strategies.len(), 1);
        assert!(check_equal_vec(
            user1_reward_info.strategies,
            vec![UserStrategyInfo {
                strategy_name: "sid1".to_string(),
                shares: Decimal::from_ratio(10000_u128, 1_u128),
                airdrop_pointer: vec![]
            }]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(
            sid1_strategy_info.total_shares,
            Decimal::from_ratio(10000_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy_info.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );

        /*
           Test - 3. user deposits to same strategy but the strategy tokens have increased
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sid1_sic_address.clone(), Uint128::new(1000_u128));
        deps.querier.update_wasm(contracts_to_token);

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.total_accumulated_rewards = Uint128::new(500_u128);
            Ok(state)
        });

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sid1_sic_address.clone(),
                unbonding_period: None,
                supported_airdrops: vec![],
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                current_unprocessed_undelegations: Default::default(),
            },
        );

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1.clone(),
            &UserRewardInfo {
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(5000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    rewards: Uint128::new(500_u128),
                    strategy_id: "sid1".to_string(),
                }],
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(sid1_sic_address.clone()),
                msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                funds: vec![Coin::new(500_u128, state.scc_denom.clone())]
            })]
        ));

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_accumulated_rewards, Uint128::new(1000_u128));

        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.strategies.len(), 1);
        assert!(check_equal_vec(
            user1_reward_info.strategies,
            vec![UserStrategyInfo {
                strategy_name: "sid1".to_string(),
                shares: Decimal::from_ratio(7500_u128, 1_u128),
                airdrop_pointer: vec![]
            }]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(
            sid1_strategy_info.total_shares,
            Decimal::from_ratio(7500_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy_info.shares_per_token_ratio,
            Decimal::from_ratio(5_u128, 1_u128)
        );

        /*
           Test - 4. user deposits to same strategy but the strategy tokens have decreased
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sid1_sic_address.clone(), Uint128::new(100_u128));
        deps.querier.update_wasm(contracts_to_token);

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.total_accumulated_rewards = Uint128::new(500_u128);
            Ok(state)
        });

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sid1_sic_address.clone(),
                unbonding_period: None,
                supported_airdrops: vec![],
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                current_unprocessed_undelegations: Default::default(),
            },
        );

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1.clone(),
            &UserRewardInfo {
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(5000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    rewards: Uint128::new(500_u128),
                    strategy_id: "sid1".to_string(),
                }],
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(sid1_sic_address.clone()),
                msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                funds: vec![Coin::new(500_u128, state.scc_denom.clone())]
            })]
        ));

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_accumulated_rewards, Uint128::new(1000_u128));

        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.strategies.len(), 1);
        assert!(check_equal_vec(
            user1_reward_info.strategies,
            vec![UserStrategyInfo {
                strategy_name: "sid1".to_string(),
                shares: Decimal::from_ratio(30000_u128, 1_u128),
                airdrop_pointer: vec![]
            }]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(
            sid1_strategy_info.total_shares,
            Decimal::from_ratio(30000_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy_info.shares_per_token_ratio,
            Decimal::from_ratio(50_u128, 1_u128)
        );

        /*
            Test - 5. 2 users deposit to the same strategy
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sid1_sic_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(contracts_to_token);

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.total_accumulated_rewards = Uint128::new(500_u128);
            Ok(state)
        });

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sid1_sic_address.clone(),
                unbonding_period: None,
                supported_airdrops: vec![],
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                current_unprocessed_undelegations: Default::default(),
            },
        );

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1.clone(),
            &UserRewardInfo {
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(5000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![
                    UpdateUserRewardsRequest {
                        user: user1.clone(),
                        rewards: Uint128::new(500_u128),
                        strategy_id: "sid1".to_string(),
                    },
                    UpdateUserRewardsRequest {
                        user: user2.clone(),
                        rewards: Uint128::new(500_u128),
                        strategy_id: "sid1".to_string(),
                    },
                ],
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sid1_sic_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(500_u128, state.scc_denom.clone())]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sid1_sic_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(500_u128, state.scc_denom.clone())]
                })
            ]
        ));

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_accumulated_rewards, Uint128::new(1500_u128));

        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.strategies.len(), 1);
        assert!(check_equal_vec(
            user1_reward_info.strategies,
            vec![UserStrategyInfo {
                strategy_name: "sid1".to_string(),
                shares: Decimal::from_ratio(10000_u128, 1_u128),
                airdrop_pointer: vec![]
            }]
        ));
        let user2_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user2.clone())
            .unwrap();
        assert_ne!(user2_reward_info_opt, None);
        let user2_reward_info = user2_reward_info_opt.unwrap();
        assert_eq!(user2_reward_info.strategies.len(), 1);
        assert!(check_equal_vec(
            user2_reward_info.strategies,
            vec![UserStrategyInfo {
                strategy_name: "sid1".to_string(),
                shares: Decimal::from_ratio(5000_u128, 1_u128),
                airdrop_pointer: vec![]
            }]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(
            sid1_strategy_info.total_shares,
            Decimal::from_ratio(15000_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy_info.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );

        /*
            Test - 5. Same user deposits to different strategies
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sid1_sic_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sid2_sic_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(contracts_to_token);

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.total_accumulated_rewards = Uint128::new(1000_u128);
            Ok(state)
        });

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sid1_sic_address.clone(),
                unbonding_period: None,
                supported_airdrops: vec![],
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                current_unprocessed_undelegations: Default::default(),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo {
                name: "sid2".to_string(),
                sic_contract_address: sid2_sic_address.clone(),
                unbonding_period: None,
                supported_airdrops: vec![],
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                current_unprocessed_undelegations: Default::default(),
            },
        );

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1.clone(),
            &UserRewardInfo {
                strategies: vec![],
                pending_airdrops: vec![],
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![
                    UpdateUserRewardsRequest {
                        user: user1.clone(),
                        rewards: Uint128::new(500_u128),
                        strategy_id: "sid1".to_string(),
                    },
                    UpdateUserRewardsRequest {
                        user: user1.clone(),
                        rewards: Uint128::new(500_u128),
                        strategy_id: "sid2".to_string(),
                    },
                ],
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sid1_sic_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(500_u128, state.scc_denom.clone())]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sid2_sic_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(500_u128, state.scc_denom.clone())]
                })
            ]
        ));

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_accumulated_rewards, Uint128::new(2000_u128));

        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.strategies.len(), 2);
        assert!(check_equal_vec(
            user1_reward_info.strategies,
            vec![
                UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(5000_u128, 1_u128),
                    airdrop_pointer: vec![]
                },
                UserStrategyInfo {
                    strategy_name: "sid2".to_string(),
                    shares: Decimal::from_ratio(5000_u128, 1_u128),
                    airdrop_pointer: vec![]
                }
            ]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(
            sid1_strategy_info.total_shares,
            Decimal::from_ratio(10000_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy_info.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap();
        assert_ne!(sid2_strategy_info_opt, None);
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(
            sid2_strategy_info.total_shares,
            Decimal::from_ratio(10000_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy_info.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        /*
           Test - 6. Update the user airdrop pointer
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sid1_sic_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(contracts_to_token);

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.total_accumulated_rewards = Uint128::new(500_u128);
            Ok(state)
        });

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1.clone(),
            &UserRewardInfo {
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(5000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sid1_sic_address.clone(),
                unbonding_period: None,
                supported_airdrops: vec![],
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![
                    DecCoin::new(Decimal::from_ratio(500_u128, 5000_u128), "anc".to_string()),
                    DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "mir".to_string()),
                ],
                total_airdrops_accumulated: vec![
                    Coin::new(500_u128, "anc".to_string()),
                    Coin::new(200_u128, "mir".to_string()),
                ],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                current_unprocessed_undelegations: Default::default(),
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    rewards: Uint128::new(500_u128),
                    strategy_id: "sid1".to_string(),
                }],
            },
        )
        .unwrap();

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_accumulated_rewards, Uint128::new(1000_u128));

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(sid1_sic_address.clone()),
                msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                funds: vec![Coin::new(500_u128, state.scc_denom.clone())]
            }),]
        ));

        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.strategies.len(), 1);
        assert!(check_equal_user_strategies(
            user1_reward_info.clone().strategies,
            vec![UserStrategyInfo {
                strategy_name: "sid1".to_string(),
                shares: Decimal::from_ratio(10000_u128, 1_u128),
                airdrop_pointer: vec![
                    DecCoin::new(Decimal::from_ratio(500_u128, 5000_u128), "anc".to_string()),
                    DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "mir".to_string()),
                ]
            }]
        ));
        assert!(check_equal_vec(
            user1_reward_info.clone().pending_airdrops,
            vec![
                Coin::new(500_u128, "anc".to_string()),
                Coin::new(200_u128, "mir".to_string())
            ]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(
            sid1_strategy_info.total_shares,
            Decimal::from_ratio(10000_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy_info.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );

        /*
            Test - 7. User has deposited to multiple strategies already
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sid1_sic_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sid2_sic_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sid3_sic_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(contracts_to_token);

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.total_accumulated_rewards = Uint128::new(500_u128);
            Ok(state)
        });

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1.clone(),
            &UserRewardInfo {
                strategies: vec![
                    UserStrategyInfo {
                        strategy_name: "sid1".to_string(),
                        shares: Decimal::from_ratio(5000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                    UserStrategyInfo {
                        strategy_name: "sid2".to_string(),
                        shares: Decimal::from_ratio(5000_u128, 1_u128),
                        airdrop_pointer: vec![DecCoin::new(
                            Decimal::from_ratio(500_u128, 5000_u128),
                            "anc".to_string(),
                        )],
                    },
                    UserStrategyInfo {
                        strategy_name: "sid3".to_string(),
                        shares: Decimal::from_ratio(5000_u128, 1_u128),
                        airdrop_pointer: vec![DecCoin::new(
                            Decimal::from_ratio(200_u128, 5000_u128),
                            "mir".to_string(),
                        )],
                    },
                ],
                pending_airdrops: vec![Coin::new(500_u128, "anc".to_string())],
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sid1_sic_address.clone(),
                unbonding_period: None,
                supported_airdrops: vec![],
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![
                    DecCoin::new(Decimal::from_ratio(500_u128, 5000_u128), "anc".to_string()),
                    DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "mir".to_string()),
                ],
                total_airdrops_accumulated: vec![
                    Coin::new(500_u128, "anc".to_string()),
                    Coin::new(200_u128, "mir".to_string()),
                ],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                current_unprocessed_undelegations: Default::default(),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo {
                name: "sid2".to_string(),
                sic_contract_address: sid2_sic_address.clone(),
                unbonding_period: None,
                supported_airdrops: vec![],
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![
                    DecCoin::new(Decimal::from_ratio(500_u128, 5000_u128), "anc".to_string()),
                    DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "mir".to_string()),
                ],
                total_airdrops_accumulated: vec![
                    Coin::new(500_u128, "anc".to_string()),
                    Coin::new(200_u128, "mir".to_string()),
                ],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                current_unprocessed_undelegations: Default::default(),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid3",
            &StrategyInfo {
                name: "sid3".to_string(),
                sic_contract_address: sid3_sic_address.clone(),
                unbonding_period: None,
                supported_airdrops: vec![],
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![DecCoin::new(
                    Decimal::from_ratio(200_u128, 5000_u128),
                    "mir".to_string(),
                )],
                total_airdrops_accumulated: vec![Coin::new(200_u128, "mir".to_string())],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                current_unprocessed_undelegations: Default::default(),
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![
                    UpdateUserRewardsRequest {
                        user: user1.clone(),
                        rewards: Uint128::new(500_u128),
                        strategy_id: "sid1".to_string(),
                    },
                    UpdateUserRewardsRequest {
                        user: user1.clone(),
                        rewards: Uint128::new(500_u128),
                        strategy_id: "sid2".to_string(),
                    },
                    UpdateUserRewardsRequest {
                        user: user1.clone(),
                        rewards: Uint128::new(500_u128),
                        strategy_id: "sid3".to_string(),
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
        assert_eq!(state.total_accumulated_rewards, Uint128::new(2000_u128));

        assert_eq!(res.messages.len(), 3);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sid1_sic_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(500_u128, state.scc_denom.clone())]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sid2_sic_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(500_u128, state.scc_denom.clone())]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sid3_sic_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(500_u128, state.scc_denom.clone())]
                })
            ]
        ));

        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.strategies.len(), 3);
        assert!(check_equal_user_strategies(
            user1_reward_info.clone().strategies,
            vec![
                UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(10000_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(500_u128, 5000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_name: "sid2".to_string(),
                    shares: Decimal::from_ratio(10000_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(500_u128, 5000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_name: "sid3".to_string(),
                    shares: Decimal::from_ratio(10000_u128, 1_u128),
                    airdrop_pointer: vec![DecCoin::new(
                        Decimal::from_ratio(200_u128, 5000_u128),
                        "mir".to_string()
                    ),]
                }
            ]
        ));
        assert!(check_equal_vec(
            user1_reward_info.clone().pending_airdrops,
            vec![
                Coin::new(1000_u128, "anc".to_string()),
                Coin::new(400_u128, "mir".to_string())
            ]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(
            sid1_strategy_info.total_shares,
            Decimal::from_ratio(10000_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy_info.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap();
        assert_ne!(sid2_strategy_info_opt, None);
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(
            sid2_strategy_info.total_shares,
            Decimal::from_ratio(10000_u128, 1_u128)
        );
        assert_eq!(
            sid2_strategy_info.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let sid3_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid3")
            .unwrap();
        assert_ne!(sid3_strategy_info_opt, None);
        let sid3_strategy_info = sid3_strategy_info_opt.unwrap();
        assert_eq!(
            sid3_strategy_info.total_shares,
            Decimal::from_ratio(10000_u128, 1_u128)
        );
        assert_eq!(
            sid3_strategy_info.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
    }

    #[test]
    fn test__try_remove_strategy_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        /*
            Test - 1. Unauthorized
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::RemoveStrategy {
                strategy_id: "sid".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test__try_remove_strategy_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid",
            &StrategyInfo::default("sid".to_string()),
        );
        let strategy_info_op = STRATEGY_MAP.may_load(deps.as_mut().storage, "sid").unwrap();
        assert_ne!(strategy_info_op, None);

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveStrategy {
                strategy_id: "sid".to_string(),
            },
        )
        .unwrap();
        let strategy_info_op = STRATEGY_MAP.may_load(deps.as_mut().storage, "sid").unwrap();
        assert_eq!(strategy_info_op, None);
    }

    #[test]
    fn test__try_deactivate_strategy_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        /*
           Test - 1. Unauthorized
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::DeactivateStrategy {
                strategy_id: "sid".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Strategy info does not exist
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::DeactivateStrategy {
                strategy_id: "sid".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::StrategyInfoDoesNotExist {}));
    }

    #[test]
    fn test__try_deactivate_strategy_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        let mut sid_strategy_info = StrategyInfo::default("sid".to_string());
        sid_strategy_info.is_active = true;
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid",
            &StrategyInfo::default("sid".to_string()),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::DeactivateStrategy {
                strategy_id: "sid".to_string(),
            },
        )
        .unwrap();

        let strategy_info_opt = STRATEGY_MAP.may_load(deps.as_mut().storage, "sid").unwrap();
        assert_ne!(strategy_info_opt, None);
        let strategy_info = strategy_info_opt.unwrap();
        assert_eq!(strategy_info.is_active, false);
    }

    #[test]
    fn test__try_activate_strategy_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        /*
           Test - 1. Unauthorized
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::ActivateStrategy {
                strategy_id: "sid".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Strategy info does not exist
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ActivateStrategy {
                strategy_id: "sid".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::StrategyInfoDoesNotExist {}));
    }

    #[test]
    fn test__try_activate_strategy_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        let mut sid_strategy_info = StrategyInfo::default("sid".to_string());
        sid_strategy_info.is_active = false;
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid",
            &StrategyInfo::default("sid".to_string()),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ActivateStrategy {
                strategy_id: "sid".to_string(),
            },
        )
        .unwrap();

        let strategy_info_opt = STRATEGY_MAP.may_load(deps.as_mut().storage, "sid").unwrap();
        assert_ne!(strategy_info_opt, None);
        let strategy_info = strategy_info_opt.unwrap();
        assert_eq!(strategy_info.is_active, true);
    }

    #[test]
    fn test__try_register_strategy_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        /*
           Test - 1. Unauthorized
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::RegisterStrategy {
                strategy_id: "sid".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: None,
                supported_airdrops: vec!["anc".to_string(), "mir".to_string()],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
            Test - 2. Strategy already exists
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid",
            &StrategyInfo::default("sid".to_string()),
        );

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RegisterStrategy {
                strategy_id: "sid".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: None,
                supported_airdrops: vec!["anc".to_string(), "mir".to_string()],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::StrategyInfoAlreadyExists {}));
    }

    #[test]
    fn test__try_register_strategy_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RegisterStrategy {
                strategy_id: "sid".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: Some(100u64),
                supported_airdrops: vec!["anc".to_string(), "mir".to_string()],
            },
        )
        .unwrap();

        let strategy_info_opt = STRATEGY_MAP.may_load(deps.as_mut().storage, "sid").unwrap();
        assert_ne!(strategy_info_opt, None);
        let strategy_info = strategy_info_opt.unwrap();
        assert_eq!(
            strategy_info,
            StrategyInfo {
                name: "sid".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: Some(100u64),
                supported_airdrops: vec!["anc".to_string(), "mir".to_string()],
                is_active: false,
                total_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(100_000_000_u128, 1_u128),
                current_unprocessed_undelegations: Default::default()
            }
        );
    }

    #[test]
    fn test__try_update_user_airdrops_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

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
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

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
