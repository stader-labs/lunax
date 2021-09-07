#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{execute, instantiate, query};
    use crate::error::ContractError::{StrategyInfoDoesNotExist, UserNotInStrategy};
    use crate::helpers::get_sic_total_tokens;
    use crate::msg::{
        ExecuteMsg, GetStateResponse, InstantiateMsg, QueryMsg, UpdateUserAirdropsRequest,
        UpdateUserRewardsRequest,
    };
    use crate::state::{
        Cw20TokenContractsInfo, State, StrategyInfo, UserRewardInfo, UserStrategyInfo,
        UserUndelegationRecord, UserUnprocessedUndelegationInfo, CW20_TOKEN_CONTRACTS_REGISTRY,
        STATE, STRATEGY_MAP, STRATEGY_UNPROCESSED_UNDELEGATIONS, USER_REWARD_INFO_MAP,
        USER_UNPROCESSED_UNDELEGATIONS,
    };
    use crate::test_helpers::{check_equal_reward_info, check_equal_user_strategies};
    use crate::ContractError;
    use cosmwasm_std::{
        coins, from_binary, to_binary, Addr, Attribute, Binary, Coin, Decimal, Empty, Env,
        MessageInfo, OwnedDeps, QuerierWrapper, Response, StdResult, SubMsg, Timestamp, Uint128,
        WasmMsg,
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
                event_loop_size: 20,
                total_accumulated_rewards: Uint128::zero(),
                current_rewards_in_scc: Uint128::zero(),
                total_accumulated_airdrops: vec![],
                current_undelegated_strategies: vec![]
            }
        );
    }

    #[test]
    fn test__try_create_user_undelegation_records_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
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
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::CreateUserUndelegationRecords {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test__try_create_user_undelegation_records_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        let user1 = Addr::unchecked("user1");
        let sic1_address = Addr::unchecked("sic1_address");

        /*
           Test - 1. Empty undelegation queue
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::CreateUserUndelegationRecords {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(env.contract.address),
                msg: to_binary(&ExecuteMsg::UndelegateFromStrategies {}).unwrap(),
                funds: vec![]
            })]
        ));
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "undelegated_from_strategies".to_string(),
                value: "1".to_string()
            }]
        ));

        /*
           Test - 2. 1 user with some unprocessed undelegation
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![
                    DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "anc".to_string()),
                    DecCoin::new(Decimal::from_ratio(300_u128, 5000_u128), "mir".to_string()),
                ],
                total_airdrops_accumulated: vec![
                    Coin::new(200_u128, "anc".to_string()),
                    Coin::new(300_u128, "mir".to_string()),
                ],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1.clone(),
            &UserRewardInfo {
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(2000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![
                    Coin::new(100_u128, "anc".to_string()),
                    Coin::new(200_u128, "mir".to_string()),
                ],
                undelegation_records: vec![],
            },
        );
        USER_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            &user1,
            &vec![UserUnprocessedUndelegationInfo {
                undelegation_amount: Uint128::new(100_u128),
                strategy_name: "".to_string(),
            }],
        );
    }

    #[test]
    fn test__try_undelegate_from_strategies_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
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
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test__try_undelegate_from_strategies_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        let sic1_address = Addr::unchecked("sic1_address");
        let sic2_address = Addr::unchecked("sic2_address");
        let sic3_address = Addr::unchecked("sic3_address");

        /*
            Test - 1. No strategy undelegations
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {},
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "no_strategies_to_undelegate_from".to_string(),
                value: "0".to_string()
            }]
        ));

        /*
           Test - 2. With some strategy undelegations
        */
        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.current_undelegated_strategies =
                    vec!["sid1".to_string(), "sid2".to_string(), "sid3".to_string()];
                Ok(state)
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::new("sid1".to_string(), sic1_address.clone(), None, None),
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo::new("sid2".to_string(), sic2_address.clone(), None, None),
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid3",
            &StrategyInfo::new("sid3".to_string(), sic3_address.clone(), None, None),
        );
        STRATEGY_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            "sid1",
            &Uint128::new(100_u128),
        );
        STRATEGY_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            "sid2",
            &Uint128::new(200_u128),
        );
        STRATEGY_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            "sid3",
            &Uint128::new(300_u128),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {},
        )
        .unwrap();

        assert_eq!(res.messages.len(), 3);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic1_address.clone().to_string(),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(100_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic2_address.to_string(),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(200_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic3_address.to_string(),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(300_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                }),
            ]
        ));

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.current_undelegated_strategies.len(), 0);

        let sid1_unprocessed_undelegation_opt = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        let sid2_unprocessed_undelegation_opt = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap();
        let sid3_unprocessed_undelegation_opt = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.as_mut().storage, "sid3")
            .unwrap();
        assert_eq!(sid1_unprocessed_undelegation_opt, None);
        assert_eq!(sid2_unprocessed_undelegation_opt, None);
        assert_eq!(sid3_unprocessed_undelegation_opt, None);

        let sid1_strategy = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap()
            .unwrap();
        let sid2_strategy = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap()
            .unwrap();
        let sid3_strategy = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid3")
            .unwrap()
            .unwrap();

        /*
           Test - 3. One of the strategies is still cooling down
        */
        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.current_undelegated_strategies = vec!["sid1".to_string(), "sid2".to_string()];
                Ok(state)
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::new("sid1".to_string(), sic1_address.clone(), None, None),
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo::new("sid2".to_string(), sic2_address.clone(), None, None),
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid3",
            &StrategyInfo {
                name: "sid3".to_string(),
                sic_contract_address: sic3_address.clone(),
                unbonding_period: (3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: false,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        STRATEGY_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            "sid1",
            &Uint128::new(100_u128),
        );
        STRATEGY_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            "sid2",
            &Uint128::new(200_u128),
        );
        STRATEGY_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            "sid3",
            &Uint128::new(300_u128),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {},
        )
        .unwrap();

        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic1_address.clone().to_string(),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(100_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic2_address.to_string(),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(200_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                }),
            ]
        ));

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.current_undelegated_strategies.len(), 0);

        let sid1_unprocessed_undelegation_opt = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        let sid2_unprocessed_undelegation_opt = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap();
        let sid3_unprocessed_undelegation_opt = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.as_mut().storage, "sid3")
            .unwrap();
        assert_eq!(sid1_unprocessed_undelegation_opt, None);
        assert_eq!(sid2_unprocessed_undelegation_opt, None);
        assert_ne!(sid3_unprocessed_undelegation_opt, None);
        assert_eq!(
            sid3_unprocessed_undelegation_opt.unwrap(),
            Uint128::new(300_u128)
        );

        let sid1_strategy = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap()
            .unwrap();
        let sid2_strategy = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap()
            .unwrap();
        let sid3_strategy = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid3")
            .unwrap()
            .unwrap();

        /*
           Test - 4. All strategies are in cooldown period
        */
        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.current_undelegated_strategies = vec![];
                Ok(state)
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::new("sid1".to_string(), sic1_address.clone(), None, None),
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo::new("sid2".to_string(), sic2_address.clone(), None, None),
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid3",
            &StrategyInfo {
                name: "sid3".to_string(),
                sic_contract_address: sic3_address.clone(),
                unbonding_period: (3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: false,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        STRATEGY_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            "sid1",
            &Uint128::new(100_u128),
        );
        STRATEGY_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            "sid2",
            &Uint128::new(200_u128),
        );
        STRATEGY_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            "sid3",
            &Uint128::new(300_u128),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "no_strategies_to_undelegate_from".to_string(),
                value: "0".to_string()
            }]
        ));

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.current_undelegated_strategies.len(), 0);

        let sid1_unprocessed_undelegation_opt = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        let sid2_unprocessed_undelegation_opt = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap();
        let sid3_unprocessed_undelegation_opt = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.as_mut().storage, "sid3")
            .unwrap();
        assert_ne!(sid1_unprocessed_undelegation_opt, None);
        assert_ne!(sid2_unprocessed_undelegation_opt, None);
        assert_ne!(sid3_unprocessed_undelegation_opt, None);

        /*
            Test - 5. One of the strategies is not in the strategy map
        */
        STRATEGY_MAP.remove(deps.as_mut().storage, "sid3");
        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.current_undelegated_strategies =
                    vec!["sid1".to_string(), "sid2".to_string(), "sid3".to_string()];
                Ok(state)
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::new("sid1".to_string(), sic1_address.clone(), None, None),
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo::new("sid2".to_string(), sic2_address.clone(), None, None),
        );
        STRATEGY_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            "sid1",
            &Uint128::new(100_u128),
        );
        STRATEGY_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            "sid2",
            &Uint128::new(200_u128),
        );
        STRATEGY_UNPROCESSED_UNDELEGATIONS.save(
            deps.as_mut().storage,
            "sid3",
            &Uint128::new(300_u128),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {},
        )
        .unwrap();

        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic1_address.clone().to_string(),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(100_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic2_address.to_string(),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(200_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                }),
            ]
        ));
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "failed_strategies".to_string(),
                value: "sid3".to_string()
            }]
        ));

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.current_undelegated_strategies.len(), 0);

        let sid1_unprocessed_undelegation_opt = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        let sid2_unprocessed_undelegation_opt = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap();
        let sid3_unprocessed_undelegation_opt = STRATEGY_UNPROCESSED_UNDELEGATIONS
            .may_load(deps.as_mut().storage, "sid3")
            .unwrap();
        assert_eq!(sid1_unprocessed_undelegation_opt, None);
        assert_eq!(sid2_unprocessed_undelegation_opt, None);
        assert_ne!(sid3_unprocessed_undelegation_opt, None);
        assert_eq!(
            sid3_unprocessed_undelegation_opt.unwrap(),
            Uint128::new(300_u128)
        );

        let sid1_strategy = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap()
            .unwrap();
        let sid2_strategy = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap()
            .unwrap();
    }

    #[test]
    fn test_try_withdraw_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        let user1 = Addr::unchecked("user1");

        /*
            Test - 1. Strategy info does not exist
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: "123".to_string(),
                strategy_name: "sid1".to_string(),
                amount: Default::default(),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::StrategyInfoDoesNotExist(String { .. })
        ));

        /*
            Test - 2. User reward info does not exist
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::default("sid1".to_string()),
        );
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: "123".to_string(),
                strategy_name: "sid1".to_string(),
                amount: Default::default(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserRewardInfoDoesNotExist {}));

        /*
            Test - 3. User undelegation record does not exist
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::default("sid1".to_string()),
        );
        USER_REWARD_INFO_MAP.save(deps.as_mut().storage, &user1, &UserRewardInfo::default());
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: "123".to_string(),
                strategy_name: "sid1".to_string(),
                amount: Default::default(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UndelegationRecordNotFound {}));
    }

    #[test]
    fn test__try_withdraw_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        let user1 = Addr::unchecked("user1");
        let sic1_address = Addr::unchecked("sic1_address");

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::new("sid1".to_string(), sic1_address.clone(), None, None),
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(5000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
                undelegation_records: vec![UserUndelegationRecord {
                    id: Timestamp::from_seconds(123),
                    amount: Uint128::new(100_u128),
                    strategy_name: "sid1".to_string(),
                    est_release_time: Timestamp::from_seconds(125),
                    undelegation_batch_id: 5,
                }],
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: "123000000000".to_string(),
                strategy_name: "sid1".to_string(),
                amount: Default::default(),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: sic1_address.clone().to_string(),
                msg: to_binary(&sic_execute_msg::WithdrawRewards {
                    user: user1,
                    amount: Uint128::new(100_u128),
                    undelegation_batch_id: 5
                })
                .unwrap(),
                funds: vec![]
            })]
        ));
    }

    #[test]
    fn test__try_undelegate_user_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        let user1 = Addr::unchecked("user1");
        let sid1_sic_address = Addr::unchecked("sid1_sic_address");

        /*
           Test - 1. Zero funds undelegations
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::zero(),
                strategy_name: "sid1".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::CannotUndelegateZeroFunds {}));

        /*
           Test - 2. Strategy info does not exist
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(100_u128),
                strategy_name: "sid1".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::StrategyInfoDoesNotExist(String { .. })
        ));

        /*
           Test - 3. User reward info does not exist
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::default("sid1".to_string()),
        );
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(100_u128),
                strategy_name: "sid1".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserRewardInfoDoesNotExist {}));

        /*
           Test - 4. User did not deposit to strategy
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::default("sid1".to_string()),
        );
        USER_REWARD_INFO_MAP.save(deps.as_mut().storage, &user1, &UserRewardInfo::default());
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(100_u128),
                strategy_name: "sid1".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserNotInStrategy {}));
        
        /*
            Test - 5. User did not have enough shares to undelegate
         */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sid1_sic_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(contracts_to_token);

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "".to_string(),
                sic_contract_address: sid1_sic_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128)
            },
        );
        USER_REWARD_INFO_MAP.save(deps.as_mut().storage, &user1, &UserRewardInfo {
            strategies: vec![UserStrategyInfo {
                strategy_name: "sid1".to_string(),
                shares: Decimal::from_ratio(3000_u128, 1_u128),
                airdrop_pointer: vec![]
            }],
            pending_airdrops: vec![],
            undelegation_records: vec![]
        });

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(400_u128),
                strategy_name: "sid1".to_string(),
            },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::UserDoesNotHaveEnoughRewards {}));
    }

    #[test]
    fn test__try_undelegate_user_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("pools_contract")),
        );

        let user1 = Addr::unchecked("user1");
        let sid1_sic_address = Addr::unchecked("sid1_sic_address");
        let sid2_sic_address = Addr::unchecked("sid2_sic_address");

        /*
            Test - 1. User undelegates for the first time from a strategy
         */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sid1_sic_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(contracts_to_token);

        STATE.update(deps.as_mut().storage, |mut state| -> Result<_, ContractError> {
            state.total_accumulated_rewards = Uint128::new(500_u128);
            Ok(state)
        });
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "".to_string(),
                sic_contract_address: sid1_sic_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![
                    DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "anc".to_string()),
                    DecCoin::new(Decimal::from_ratio(400_u128, 5000_u128), "mir".to_string()),
                ],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128)
            },
        );
        USER_REWARD_INFO_MAP.save(deps.as_mut().storage, &user1, &UserRewardInfo {
            strategies: vec![UserStrategyInfo {
                strategy_name: "sid1".to_string(),
                shares: Decimal::from_ratio(3000_u128, 1_u128),
                airdrop_pointer: vec![]
            }],
            pending_airdrops: vec![],
            undelegation_records: vec![]
        });

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(200_u128),
                strategy_name: "sid1".to_string(),
            },
        ).unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_accumulated_rewards, Uint128::new(300_u128));

        let sid1_strategy_info_opt = STRATEGY_MAP.may_load(deps.as_mut().storage, "sid1").unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.total_shares, Decimal::from_ratio(3000_u128, 1_u128));

        let user1_reward_info_opt = USER_REWARD_INFO_MAP.may_load(deps.as_mut().storage, &user1).unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert!(check_equal_vec(user1_reward_info.strategies, vec![UserStrategyInfo {
            strategy_name: "sid1".to_string(),
            shares: Decimal::from_ratio(1000_u128, 1_u128),
            airdrop_pointer: vec![
                DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "anc".to_string()),
                DecCoin::new(Decimal::from_ratio(400_u128, 5000_u128), "mir".to_string()),
            ]
        }]));
        assert!(check_equal_vec(user1_reward_info.pending_airdrops, vec![
            Coin::new(120_u128, "anc".to_string()),
            Coin::new(240_u128, "mir".to_string())
        ]));
        assert!(check_equal_vec(user1_reward_info.undelegation_records, vec![
            UserUndelegationRecord {
                id: env.block.time,
                amount: Uint128::new(200_u128),
                strategy_name: "sid1".to_string(),
                est_release_time: env.block.time.plus_seconds(7200),
                undelegation_batch_id: 0
            }
        ]));

        /*
            Test - 2. User undelegates again in the same blocktime
         */
        STATE.update(deps.as_mut().storage, |mut state| -> Result<_, ContractError> {
            state.total_accumulated_rewards = Uint128::new(300_u128);
            Ok(state)
        });
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "".to_string(),
                sic_contract_address: sid1_sic_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(3000_u128, 1_u128),
                global_airdrop_pointer: vec![
                    DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "anc".to_string()),
                    DecCoin::new(Decimal::from_ratio(400_u128, 5000_u128), "mir".to_string()),
                ],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128)
            },
        );
        USER_REWARD_INFO_MAP.save(deps.as_mut().storage, &user1, &UserRewardInfo {
            strategies: vec![UserStrategyInfo {
                strategy_name: "sid1".to_string(),
                shares: Decimal::from_ratio(1000_u128, 1_u128),
                airdrop_pointer: vec![
                    DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "anc".to_string()),
                    DecCoin::new(Decimal::from_ratio(400_u128, 5000_u128), "mir".to_string()),
                ]
            }],
            pending_airdrops: vec![
                Coin::new(120_u128, "anc".to_string()),
                Coin::new(240_u128, "mir".to_string())
            ],
            undelegation_records: vec![
                UserUndelegationRecord {
                    id: env.block.time,
                    amount: Uint128::new(200_u128),
                    strategy_name: "sid1".to_string(),
                    est_release_time: env.block.time.plus_seconds(7200),
                    undelegation_batch_id: 0
                }
            ]
        });

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(50_u128),
                strategy_name: "sid1".to_string(),
            },
        ).unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_accumulated_rewards, Uint128::new(250_u128));

        let sid1_strategy_info_opt = STRATEGY_MAP.may_load(deps.as_mut().storage, "sid1").unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.total_shares, Decimal::from_ratio(2500_u128, 1_u128));

        let user1_reward_info_opt = USER_REWARD_INFO_MAP.may_load(deps.as_mut().storage, &user1).unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert!(check_equal_vec(user1_reward_info.strategies, vec![UserStrategyInfo {
            strategy_name: "sid1".to_string(),
            shares: Decimal::from_ratio(500_u128, 1_u128),
            airdrop_pointer: vec![
                DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "anc".to_string()),
                DecCoin::new(Decimal::from_ratio(400_u128, 5000_u128), "mir".to_string()),
            ]
        }]));
        assert!(check_equal_vec(user1_reward_info.pending_airdrops, vec![
            Coin::new(120_u128, "anc".to_string()),
            Coin::new(240_u128, "mir".to_string())
        ]));
        assert!(check_equal_vec(user1_reward_info.undelegation_records, vec![
            UserUndelegationRecord {
                id: env.block.time,
                amount: Uint128::new(200_u128),
                strategy_name: "sid1".to_string(),
                est_release_time: env.block.time.plus_seconds(7200),
                undelegation_batch_id: 0
            },
            UserUndelegationRecord {
                id: env.block.time,
                amount: Uint128::new(50_u128),
                strategy_name: "sid1".to_string(),
                est_release_time: env.block.time.plus_seconds(7200),
                undelegation_batch_id: 0
            },
        ]));
    }

    #[test]
    fn test__try_claim_airdrops_fail() {
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
        assert!(matches!(
            err,
            ContractError::StrategyInfoDoesNotExist(String { .. })
        ));
    }

    #[test]
    fn test__try_claim_airdrops_success() {
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

        let anc_cw20_contract: Addr = Addr::unchecked("anc-cw20-contract");
        let mir_cw20_contract: Addr = Addr::unchecked("mir-cw20-contract");
        let anc_airdrop_contract: Addr = Addr::unchecked("anc-airdrop-contract");
        let mir_airdrop_contract: Addr = Addr::unchecked("mir-airdrop-contract");

        let sic_contract: Addr = Addr::unchecked("sic-contract");

        fn get_airdrop_claim_msg() -> Binary {
            Binary::from(vec![1, 2, 3, 4, 5, 6, 7, 8, 9])
        }

        CW20_TOKEN_CONTRACTS_REGISTRY.save(
            deps.as_mut().storage,
            "anc".to_string(),
            &Cw20TokenContractsInfo {
                airdrop_contract: anc_airdrop_contract.clone(),
                cw20_token_contract: anc_cw20_contract.clone(),
            },
        );
        CW20_TOKEN_CONTRACTS_REGISTRY.save(
            deps.as_mut().storage,
            "mir".to_string(),
            &Cw20TokenContractsInfo {
                airdrop_contract: mir_airdrop_contract.clone(),
                cw20_token_contract: mir_cw20_contract.clone(),
            },
        );

        /*
           Test - 1. Claiming airdrops from the sic for the first time
        */
        let mut strategy_info =
            StrategyInfo::new("sid".to_string(), sic_contract.clone(), None, None);
        strategy_info.total_shares = Decimal::from_ratio(100_000_000_u128, 1_u128);
        STRATEGY_MAP.save(deps.as_mut().storage, "sid", &strategy_info);

        let res = execute(
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
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(sic_contract.clone()),
                msg: to_binary(&sic_execute_msg::ClaimAirdrops {
                    airdrop_token_contract: anc_airdrop_contract.clone(),
                    cw20_token_contract: anc_cw20_contract.clone(),
                    airdrop_token: "anc".to_string(),
                    amount: Uint128::new(100_u128),
                    claim_msg: get_airdrop_claim_msg(),
                })
                .unwrap(),
                funds: vec![]
            })]
        );

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(
            state.total_accumulated_airdrops,
            vec![Coin::new(100_u128, "anc".to_string())]
        );

        let strategy_info_opt = STRATEGY_MAP.may_load(deps.as_mut().storage, "sid").unwrap();
        assert_ne!(strategy_info_opt, None);
        let strategy_info = strategy_info_opt.unwrap();
        assert_eq!(
            strategy_info.total_airdrops_accumulated,
            vec![Coin::new(100_u128, "anc".to_string())]
        );
        assert_eq!(
            strategy_info.global_airdrop_pointer,
            vec![DecCoin::new(
                Decimal::from_ratio(100_u128, 100_000_000_u128),
                "anc".to_string()
            )]
        );

        /*
            Test - 2. Claiming airdrops a mir airdrop with anc airdrop
        */
        let mut strategy_info =
            StrategyInfo::new("sid".to_string(), sic_contract.clone(), None, None);
        strategy_info.total_shares = Decimal::from_ratio(100_000_000_u128, 1_u128);
        strategy_info.global_airdrop_pointer = vec![DecCoin::new(
            Decimal::from_ratio(100_u128, 100_000_000_u128),
            "anc".to_string(),
        )];
        strategy_info.total_airdrops_accumulated = vec![Coin::new(100_u128, "anc".to_string())];

        STRATEGY_MAP.save(deps.as_mut().storage, "sid", &strategy_info);

        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.total_accumulated_airdrops = vec![
                    Coin::new(200_u128, "anc".to_string()),
                    Coin::new(500_u128, "mir".to_string()),
                ];
                Ok(state)
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: "sid".to_string(),
                amount: Uint128::new(100_u128),
                denom: "mir".to_string(),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(sic_contract.clone()),
                msg: to_binary(&sic_execute_msg::ClaimAirdrops {
                    airdrop_token_contract: mir_airdrop_contract.clone(),
                    cw20_token_contract: mir_cw20_contract,
                    airdrop_token: "mir".to_string(),
                    amount: Uint128::new(100_u128),
                    claim_msg: get_airdrop_claim_msg(),
                })
                .unwrap(),
                funds: vec![]
            })]
        );

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert!(check_equal_vec(
            state.total_accumulated_airdrops,
            vec![
                Coin::new(200_u128, "anc".to_string()),
                Coin::new(600_u128, "mir".to_string())
            ]
        ));

        let strategy_info_opt = STRATEGY_MAP.may_load(deps.as_mut().storage, "sid").unwrap();
        assert_ne!(strategy_info_opt, None);
        let strategy_info = strategy_info_opt.unwrap();
        assert!(check_equal_vec(
            strategy_info.total_airdrops_accumulated,
            vec![
                Coin::new(100_u128, "anc".to_string()),
                Coin::new(100_u128, "mir".to_string())
            ]
        ));
        assert!(check_equal_vec(
            strategy_info.global_airdrop_pointer,
            vec![
                DecCoin::new(
                    Decimal::from_ratio(100_u128, 100_000_000_u128),
                    "anc".to_string()
                ),
                DecCoin::new(
                    Decimal::from_ratio(100_u128, 100_000_000_u128),
                    "mir".to_string()
                )
            ]
        ));
    }

    #[test]
    fn test__try_withdraw_airdrops_fail() {
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
           Test - 1. User reward info does not exist
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::WithdrawAirdrops {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserRewardInfoDoesNotExist {}));
    }

    #[test]
    fn test__try_withdraw_airdrops_success() {
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
        let anc_airdrop_contract = Addr::unchecked("anc_airdrop_contract");
        let mir_airdrop_contract = Addr::unchecked("mir_airdrop_contract");
        let anc_token_contract = Addr::unchecked("anc_token_contract");
        let mir_token_contract = Addr::unchecked("mir_token_contract");

        let sic1_address = Addr::unchecked("sic1_address");
        /*
           Test - 1. User has some pending airdrops
        */
        CW20_TOKEN_CONTRACTS_REGISTRY.save(
            deps.as_mut().storage,
            "anc".to_string(),
            &Cw20TokenContractsInfo {
                airdrop_contract: anc_airdrop_contract.clone(),
                cw20_token_contract: anc_token_contract.clone(),
            },
        );
        CW20_TOKEN_CONTRACTS_REGISTRY.save(
            deps.as_mut().storage,
            "mir".to_string(),
            &Cw20TokenContractsInfo {
                airdrop_contract: mir_airdrop_contract.clone(),
                cw20_token_contract: mir_token_contract.clone(),
            },
        );

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address,
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
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
            },
        );

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(5000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
                undelegation_records: vec![],
            },
        );
        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.total_accumulated_airdrops = vec![
                Coin::new(500_u128, "anc".to_string()),
                Coin::new(700_u128, "mir".to_string()),
            ];
            Ok(state)
        });

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
                    contract_addr: String::from(anc_token_contract.clone()),
                    msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                        recipient: String::from(user1.clone()),
                        amount: Uint128::new(500),
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(mir_token_contract.clone()),
                    msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                        recipient: String::from(user1.clone()),
                        amount: Uint128::new(200),
                    })
                    .unwrap(),
                    funds: vec![]
                }),
            ]
        ));

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert!(check_equal_vec(
            state.total_accumulated_airdrops,
            vec![
                Coin::new(0_u128, "anc".to_string()),
                Coin::new(500_u128, "mir".to_string())
            ]
        ));

        let user_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        assert_ne!(user_reward_info_opt, None);
        let user_reward_info = user_reward_info_opt.unwrap();
        assert_eq!(user_reward_info.pending_airdrops, vec![]);
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
                undelegation_records: vec![],
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::new("sid1".to_string(), sid1_sic_address.clone(), None, None),
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
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
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
                undelegation_records: vec![],
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
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
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
                undelegation_records: vec![],
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
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
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
                undelegation_records: vec![],
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
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
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
                undelegation_records: vec![],
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

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(sid1_sic_address.clone()),
                msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                funds: vec![Coin::new(1000_u128, state.scc_denom.clone())]
            })]
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
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo {
                name: "sid2".to_string(),
                sic_contract_address: sid2_sic_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1.clone(),
            &UserRewardInfo {
                strategies: vec![],
                pending_airdrops: vec![],
                undelegation_records: vec![],
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
                undelegation_records: vec![],
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sid1_sic_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
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
                undelegation_records: vec![],
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sid1_sic_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
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
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo {
                name: "sid2".to_string(),
                sic_contract_address: sid2_sic_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
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
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid3",
            &StrategyInfo {
                name: "sid3".to_string(),
                sic_contract_address: sid3_sic_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                global_airdrop_pointer: vec![DecCoin::new(
                    Decimal::from_ratio(200_u128, 5000_u128),
                    "mir".to_string(),
                )],
                total_airdrops_accumulated: vec![Coin::new(200_u128, "mir".to_string())],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
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

        let strategy_name: String = String::from("sid");

        /*
           Test - 1. Unauthorized
        */
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::DeactivateStrategy {
                strategy_id: strategy_name.clone(),
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
                strategy_id: strategy_name.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::StrategyInfoDoesNotExist(String { .. })
        ));
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
        assert!(matches!(
            err,
            ContractError::StrategyInfoDoesNotExist(String { .. })
        ));
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
                unbonding_buffer: None,
                unbonding_period: None,
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
                unbonding_buffer: None,
                unbonding_period: None,
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
                unbonding_buffer: Some(100u64),
                unbonding_period: Some(100u64),
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
                unbonding_period: 100u64,
                unbonding_buffer: 3600,
                current_undelegation_batch_id: 0,
                is_active: false,
                total_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
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
            mock_info("pools_contract", &[]),
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
            mock_info("pools_contract", &[]),
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
                undelegation_records: vec![],
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user2,
            &UserRewardInfo {
                strategies: vec![],
                pending_airdrops: vec![Coin::new(20_u128, "abc"), Coin::new(100_u128, "def")],
                undelegation_records: vec![],
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user3,
            &UserRewardInfo {
                strategies: vec![],
                pending_airdrops: vec![Coin::new(30_u128, "abc"), Coin::new(50_u128, "def")],
                undelegation_records: vec![],
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user4,
            &UserRewardInfo {
                strategies: vec![],
                pending_airdrops: vec![Coin::new(40_u128, "abc"), Coin::new(80_u128, "def")],
                undelegation_records: vec![],
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_contract", &[]),
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
