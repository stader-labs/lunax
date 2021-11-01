#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};
    use crate::helpers::{
        get_expected_strategy_or_default, get_strategy_apr, get_strategy_shares_per_token_ratio,
        get_strategy_split, validate_user_portfolio,
    };
    use crate::msg::{
        ExecuteMsg, GetAllStrategiesResponse, GetConfigResponse, GetStateResponse,
        GetStrategiesListResponse, GetUserResponse, InstantiateMsg, QueryMsg, StrategyInfoQuery,
        UpdateUserAirdropsRequest, UpdateUserRewardsRequest, UserRewardInfoQuery,
        UserStrategyQueryInfo,
    };
    use crate::state::{
        BatchUndelegationRecord, Config, Cw20TokenContractsInfo, State, StrategyInfo,
        UndelegationBatchStatus, UserRewardInfo, UserStrategyInfo, UserStrategyPortfolio,
        UserUndelegationRecord, CONFIG, CW20_TOKEN_CONTRACTS_REGISTRY, STATE, STRATEGY_MAP,
        UNDELEGATION_BATCH_MAP, USER_REWARD_INFO_MAP,
    };
    use crate::testing::mock_querier::{mock_dependencies, WasmMockQuerier};
    use crate::testing::test_helpers::{
        check_equal_reward_info, check_equal_user_strategies, check_equal_user_strategy_query_info,
        check_equal_vec,
    };
    use crate::ContractError;
    use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage};
    use cosmwasm_std::{
        coins, from_binary, to_binary, Addr, Attribute, BankMsg, Binary, Coin, Decimal, Empty, Env,
        MessageInfo, OwnedDeps, Response, StdResult, SubMsg, Timestamp, Uint128, WasmMsg,
    };
    use cw_storage_plus::U64Key;
    use sic_base::msg::ExecuteMsg as sic_execute_msg;
    use stader_utils::coin_utils::DecCoin;
    use std::collections::HashMap;

    fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
        info: &MessageInfo,
        env: &Env,
        strategy_denom: Option<String>,
        delegator_contract: Option<String>,
        default_user_portfolio: Option<Vec<UserStrategyPortfolio>>,
    ) -> Response<Empty> {
        let msg = InstantiateMsg {
            strategy_denom: strategy_denom.unwrap_or("uluna".to_string()),
            delegator_contract: delegator_contract.unwrap_or("delegator_contract".to_string()),
            default_user_portfolio,
            default_fallback_strategy: None,
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    }

    fn get_delegator_contract_address() -> Addr {
        Addr::unchecked("delegator_contract")
    }

    macro_rules! hashmap {
        ($( $key: expr => $val: expr ),*) => {{
             let mut map = ::std::collections::HashMap::new();
             $( map.insert($key, $val); )*
             map
        }}
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
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
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
                delegator_contract: Addr::unchecked("delegator_contract"),
                scc_denom: "uluna".to_string(),
                contract_genesis_block_height: env.block.height,
                contract_genesis_timestamp: env.block.time,
                next_undelegation_id: 0,
                next_strategy_id: 1,
                rewards_in_scc: Uint128::zero(),
                total_accumulated_airdrops: vec![],
            }
        );
        // query the config
        let config_response: GetConfigResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetConfig {}).unwrap())
                .unwrap();
        assert_ne!(config_response.config, None);
        let config = config_response.config.unwrap();
        assert_eq!(
            config,
            Config {
                default_user_portfolio: vec![],
                fallback_strategy: 0
            }
        );

        // check whether RETAIN_REWARDS strategy has been created
        let retain_rewards_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(0))
            .unwrap();
        assert_ne!(retain_rewards_strategy_opt, None);
        let retain_rewards_strategy = retain_rewards_strategy_opt.unwrap();
        assert_eq!(
            retain_rewards_strategy,
            StrategyInfo::default("RETAIN_REWARDS".to_string())
        );
    }

    #[test]
    fn test_query_get_user() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let user1: Addr = Addr::unchecked("user1");

        let sic1_address: Addr = Addr::unchecked("sic1_address");
        let sic2_address: Addr = Addr::unchecked("sic2_address");
        let sic3_address: Addr = Addr::unchecked("sic3_address");

        /*
           Test - 1. User reward info is not present
        */
        CONFIG
            .update(
                deps.as_mut().storage,
                |mut config| -> Result<_, ContractError> {
                    config.default_user_portfolio = vec![UserStrategyPortfolio {
                        strategy_id: 0,
                        deposit_fraction: Uint128::new(100_u128),
                    }];
                    Ok(config)
                },
            )
            .unwrap();
        let user_response: GetUserResponse = from_binary(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::GetUser {
                    user: user1.to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(
            user_response.user,
            Some(UserRewardInfoQuery {
                total_airdrops: vec![],
                retained_rewards: Uint128::zero(),
                undelegation_records: vec![],
                user_strategy_info: vec![],
                user_portfolio: config.default_user_portfolio
            })
        );

        /*
           Test - 2. User has delegated to multiple strategies
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic3_address.clone(), Uint128::new(0_u128));

        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(250_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(20_u128, 250_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(30_u128, 250_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(20_u128, "anc".to_string()),
                        Coin::new(30_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 7200,
                    unbonding_buffer: 7200,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(250_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(100_u128, 1_u128),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(20_u128, 250_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(30_u128, 250_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(20_u128, "anc".to_string()),
                        Coin::new(30_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: sic3_address.clone(),
                    unbonding_period: 14400,
                    unbonding_buffer: 14400,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(250_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(20_u128, 250_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(30_u128, 250_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(20_u128, "anc".to_string()),
                        Coin::new(30_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();

        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![
                        UserStrategyPortfolio {
                            strategy_id: 1,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 2,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 3,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                    ],
                    strategies: vec![
                        UserStrategyInfo {
                            strategy_id: 1,
                            shares: Decimal::from_ratio(250_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                        UserStrategyInfo {
                            strategy_id: 2,
                            shares: Decimal::from_ratio(250_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                        UserStrategyInfo {
                            strategy_id: 3,
                            shares: Decimal::from_ratio(250_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                    ],
                    pending_airdrops: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(200_u128, "mir".to_string()),
                    ],
                    undelegation_records: vec![UserUndelegationRecord {
                        id: 0,
                        amount: Uint128::new(100_u128),
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        strategy_id: 1,
                        undelegation_batch_id: 0,
                    }],
                    pending_rewards: Uint128::new(25_u128),
                },
            )
            .unwrap();

        let user_response: GetUserResponse = from_binary(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::GetUser {
                    user: user1.to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_ne!(user_response.user, None);
        let user = user_response.user.unwrap();
        assert_eq!(user.retained_rewards, Uint128::new(25_u128));
        assert!(check_equal_vec(
            user.total_airdrops,
            vec![
                Coin::new(160_u128, "anc".to_string()),
                Coin::new(290_u128, "mir".to_string())
            ]
        ));
        assert!(check_equal_vec(
            user.undelegation_records,
            vec![UserUndelegationRecord {
                id: 0,
                amount: Uint128::new(100_u128),
                shares: Decimal::from_ratio(1000_u128, 1_u128),
                strategy_id: 1,
                undelegation_batch_id: 0
            }]
        ));
        assert!(check_equal_vec(
            user.user_portfolio,
            vec![
                UserStrategyPortfolio {
                    strategy_id: 1,
                    deposit_fraction: Uint128::new(25_u128)
                },
                UserStrategyPortfolio {
                    strategy_id: 2,
                    deposit_fraction: Uint128::new(25_u128)
                },
                UserStrategyPortfolio {
                    strategy_id: 3,
                    deposit_fraction: Uint128::new(25_u128)
                }
            ]
        ));

        assert!(check_equal_user_strategy_query_info(
            user.user_strategy_info,
            vec![
                UserStrategyQueryInfo {
                    strategy_id: 1,
                    strategy_name: "sid1".to_string(),
                    total_rewards: Uint128::new(25_u128),
                    total_airdrops: vec![
                        Coin::new(20_u128, "anc".to_string()),
                        Coin::new(30_u128, "mir".to_string())
                    ]
                },
                UserStrategyQueryInfo {
                    strategy_id: 2,
                    strategy_name: "sid2".to_string(),
                    total_rewards: Uint128::new(25_u128),
                    total_airdrops: vec![
                        Coin::new(20_u128, "anc".to_string()),
                        Coin::new(30_u128, "mir".to_string())
                    ]
                },
                UserStrategyQueryInfo {
                    strategy_id: 3,
                    strategy_name: "sid3".to_string(),
                    total_rewards: Uint128::new(25_u128),
                    total_airdrops: vec![
                        Coin::new(20_u128, "anc".to_string()),
                        Coin::new(30_u128, "mir".to_string())
                    ]
                }
            ]
        ));
    }

    #[test]
    fn test_query_get_all_strategies() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let sic1_address: Addr = Addr::unchecked("sic1_address");
        let sic2_address: Addr = Addr::unchecked("sic2_address");
        let sic3_address: Addr = Addr::unchecked("sic3_address");

        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(500_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(2000_u128));
        contracts_to_token.insert(sic3_address.clone(), Uint128::new(0_u128));

        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.rewards_in_scc = Uint128::new(1000_u128);
                    state.next_strategy_id = 4;
                    Ok(state)
                },
            )
            .unwrap();

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(10000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(
                            Decimal::from_ratio(1000_u128, 10000_u128),
                            "anc".to_string(),
                        ),
                        DecCoin::new(Decimal::from_ratio(500_u128, 10000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(1000_u128, "anc".to_string()),
                        Coin::new(500_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 7200,
                    unbonding_buffer: 7200,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(10000_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(500_u128, 10000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(700_u128, 10000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(500_u128, "anc".to_string()),
                        Coin::new(700_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: sic3_address.clone(),
                    unbonding_period: 14400,
                    unbonding_buffer: 14400,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(7000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 7000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(50_u128, 7000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(50_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();

        let all_strategies_response: GetAllStrategiesResponse = from_binary(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::GetAllStrategies {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_ne!(all_strategies_response.all_strategies, None);
        let all_strategies = all_strategies_response.all_strategies.unwrap();
        assert!(check_equal_vec(
            all_strategies,
            vec![
                StrategyInfoQuery {
                    strategy_id: 0,
                    strategy_name: "RETAIN_REWARDS".to_string(),
                    total_rewards: Uint128::new(1000_u128),
                    rewards_in_undelegation: Uint128::zero(),
                    is_active: true,
                    total_airdrops_accumulated: vec![],
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    sic_contract_address: Addr::unchecked("")
                },
                StrategyInfoQuery {
                    strategy_id: 1,
                    strategy_name: "sid1".to_string(),
                    total_rewards: Uint128::new(500_u128),
                    rewards_in_undelegation: Uint128::zero(),
                    is_active: true,
                    total_airdrops_accumulated: vec![
                        Coin::new(1000_u128, "anc".to_string()),
                        Coin::new(500_u128, "mir".to_string())
                    ],
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    sic_contract_address: sic1_address.clone()
                },
                StrategyInfoQuery {
                    strategy_id: 2,
                    strategy_name: "sid2".to_string(),
                    total_rewards: Uint128::new(2000_u128),
                    rewards_in_undelegation: Uint128::new(1000_u128),
                    is_active: true,
                    total_airdrops_accumulated: vec![
                        Coin::new(500_u128, "anc".to_string()),
                        Coin::new(700_u128, "mir".to_string())
                    ],
                    unbonding_period: 7200,
                    unbonding_buffer: 7200,
                    sic_contract_address: sic2_address.clone()
                },
                StrategyInfoQuery {
                    strategy_id: 3,
                    strategy_name: "sid3".to_string(),
                    total_rewards: Uint128::new(700_u128),
                    rewards_in_undelegation: Uint128::zero(),
                    is_active: true,
                    total_airdrops_accumulated: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(50_u128, "mir".to_string())
                    ],
                    unbonding_period: 14400,
                    unbonding_buffer: 14400,
                    sic_contract_address: sic3_address.clone()
                }
            ]
        ));

        /*
           Test pagination
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.rewards_in_scc = Uint128::new(1000_u128);
                    state.next_strategy_id = 4;
                    Ok(state)
                },
            )
            .unwrap();

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(10000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(
                            Decimal::from_ratio(1000_u128, 10000_u128),
                            "anc".to_string(),
                        ),
                        DecCoin::new(Decimal::from_ratio(500_u128, 10000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(1000_u128, "anc".to_string()),
                        Coin::new(500_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 7200,
                    unbonding_buffer: 7200,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(10000_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(500_u128, 10000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(700_u128, 10000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(500_u128, "anc".to_string()),
                        Coin::new(700_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: sic3_address.clone(),
                    unbonding_period: 14400,
                    unbonding_buffer: 14400,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(7000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 7000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(50_u128, 7000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(50_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();

        let all_strategies_response: GetAllStrategiesResponse = from_binary(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::GetAllStrategies {
                    start_after: Some(1),
                    limit: Some(1),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_ne!(all_strategies_response.all_strategies, None);
        let all_strategies = all_strategies_response.all_strategies.unwrap();
        assert!(check_equal_vec(
            all_strategies,
            vec![StrategyInfoQuery {
                strategy_id: 2,
                strategy_name: "sid2".to_string(),
                total_rewards: Uint128::new(2000_u128),
                rewards_in_undelegation: Uint128::new(1000_u128),
                is_active: true,
                total_airdrops_accumulated: vec![
                    Coin::new(500_u128, "anc".to_string()),
                    Coin::new(700_u128, "mir".to_string())
                ],
                unbonding_period: 7200,
                unbonding_buffer: 7200,
                sic_contract_address: sic2_address.clone()
            },]
        ));

        /*
            Test pagination
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.rewards_in_scc = Uint128::new(1000_u128);
                    state.next_strategy_id = 4;
                    Ok(state)
                },
            )
            .unwrap();

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(10000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(
                            Decimal::from_ratio(1000_u128, 10000_u128),
                            "anc".to_string(),
                        ),
                        DecCoin::new(Decimal::from_ratio(500_u128, 10000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(1000_u128, "anc".to_string()),
                        Coin::new(500_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 7200,
                    unbonding_buffer: 7200,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(10000_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(500_u128, 10000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(700_u128, 10000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(500_u128, "anc".to_string()),
                        Coin::new(700_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: sic3_address.clone(),
                    unbonding_period: 14400,
                    unbonding_buffer: 14400,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(7000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 7000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(50_u128, 7000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(50_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();

        let all_strategies_response: GetAllStrategiesResponse = from_binary(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::GetAllStrategies {
                    start_after: Some(2),
                    limit: Some(3),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_ne!(all_strategies_response.all_strategies, None);
        let all_strategies = all_strategies_response.all_strategies.unwrap();
        assert!(check_equal_vec(
            all_strategies,
            vec![StrategyInfoQuery {
                strategy_id: 3,
                strategy_name: "sid3".to_string(),
                total_rewards: Uint128::new(700_u128),
                rewards_in_undelegation: Uint128::zero(),
                is_active: true,
                total_airdrops_accumulated: vec![
                    Coin::new(100_u128, "anc".to_string()),
                    Coin::new(50_u128, "mir".to_string())
                ],
                unbonding_period: 14400,
                unbonding_buffer: 14400,
                sic_contract_address: sic3_address.clone()
            }]
        ));
    }

    #[test]
    fn test_query_strategies_list() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        /*
            Test - 1. No strategies
        */
        let strategies_list_response: GetStrategiesListResponse = from_binary(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::GetStrategiesList {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_ne!(strategies_list_response.strategies_list, None);
        let strategies_list = strategies_list_response.strategies_list.unwrap();
        assert_eq!(strategies_list.len(), 1);
        assert!(check_equal_vec(
            strategies_list,
            vec!["RETAIN_REWARDS".to_string()]
        ));

        /*
           Test - 2. 5 strategies
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.next_strategy_id = 6;
                    Ok(state)
                },
            )
            .unwrap();

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo::default("sid2".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo::default("sid3".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(4),
                &StrategyInfo::default("sid4".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(5),
                &StrategyInfo::default("sid5".to_string()),
            )
            .unwrap();

        let strategies_list_response: GetStrategiesListResponse = from_binary(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::GetStrategiesList {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_ne!(strategies_list_response.strategies_list, None);
        let strategies_list = strategies_list_response.strategies_list.unwrap();
        assert_eq!(
            strategies_list,
            vec!["RETAIN_REWARDS", "sid1", "sid2", "sid3", "sid4", "sid5"]
        );

        /*
            Test - 3. Test pagination
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.next_strategy_id = 6;
                    Ok(state)
                },
            )
            .unwrap();

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo::default("sid2".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo::default("sid3".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(4),
                &StrategyInfo::default("sid4".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(5),
                &StrategyInfo::default("sid5".to_string()),
            )
            .unwrap();

        let strategies_list_response: GetStrategiesListResponse = from_binary(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::GetStrategiesList {
                    start_after: Some(2),
                    limit: Some(2),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_ne!(strategies_list_response.strategies_list, None);
        let strategies_list = strategies_list_response.strategies_list.unwrap();
        assert_eq!(strategies_list, vec!["sid3", "sid4"]);

        /*
            Test - 4. Test pagination
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.next_strategy_id = 6;
                    Ok(state)
                },
            )
            .unwrap();

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo::default("sid2".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo::default("sid3".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(4),
                &StrategyInfo::default("sid4".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(5),
                &StrategyInfo::default("sid5".to_string()),
            )
            .unwrap();

        let strategies_list_response: GetStrategiesListResponse = from_binary(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::GetStrategiesList {
                    start_after: Some(4),
                    limit: Some(3),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_ne!(strategies_list_response.strategies_list, None);
        let strategies_list = strategies_list_response.strategies_list.unwrap();
        assert_eq!(strategies_list, vec!["sid5"]);
    }

    #[test]
    fn test_update_config_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::UpdateConfig {
                delegator_contract: None,
                default_user_portfolio: None,
                fallback_strategy: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Invalid fallback strategy
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                delegator_contract: None,
                default_user_portfolio: None,
                fallback_strategy: Some(1),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::StrategyInfoDoesNotExist {}));

        /*
            Test - 3. Invalid user portfolio - non existent strategy
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                delegator_contract: None,
                default_user_portfolio: Some(vec![
                    UserStrategyPortfolio {
                        strategy_id: 1,
                        deposit_fraction: Uint128::new(50_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_id: 2,
                        deposit_fraction: Uint128::new(25_u128),
                    },
                ]),
                fallback_strategy: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InvalidUserPortfolio {}));

        /*
           Test - 4. Invalid user portfolio - invalid deposit fraction
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo::default("sid2".to_string()),
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                delegator_contract: None,
                default_user_portfolio: Some(vec![
                    UserStrategyPortfolio {
                        strategy_id: 1,
                        deposit_fraction: Uint128::new(50_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_id: 2,
                        deposit_fraction: Uint128::new(75_u128),
                    },
                ]),
                fallback_strategy: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InvalidUserPortfolio {}));
    }

    #[test]
    fn test_update_config_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let old_delegator_contract = Addr::unchecked("old_delegator_contract");
        let new_delegator_contract = Addr::unchecked("new_delegator_contract");

        /*
           Test - 1. successful
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo::default("sid2".to_string()),
            )
            .unwrap();
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.delegator_contract = old_delegator_contract;
                    Ok(state)
                },
            )
            .unwrap();
        CONFIG
            .update(
                deps.as_mut().storage,
                |mut config| -> Result<_, ContractError> {
                    config.fallback_strategy = 0;
                    config.default_user_portfolio = vec![];
                    Ok(config)
                },
            )
            .unwrap();

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                delegator_contract: Some(new_delegator_contract.to_string()),
                default_user_portfolio: Some(vec![
                    UserStrategyPortfolio {
                        strategy_id: 1,
                        deposit_fraction: Uint128::new(25_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_id: 2,
                        deposit_fraction: Uint128::new(25_u128),
                    },
                ]),
                fallback_strategy: Some(1),
            },
        )
        .unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.delegator_contract, new_delegator_contract);

        let config_response: GetConfigResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetConfig {}).unwrap())
                .unwrap();
        assert_ne!(config_response.config, None);
        let config = config_response.config.unwrap();
        assert_eq!(config.fallback_strategy, 1);
        assert!(check_equal_vec(
            config.default_user_portfolio,
            vec![
                UserStrategyPortfolio {
                    strategy_id: 1,
                    deposit_fraction: Uint128::new(25_u128)
                },
                UserStrategyPortfolio {
                    strategy_id: 2,
                    deposit_fraction: Uint128::new(25_u128)
                },
            ]
        ));
    }

    #[test]
    fn test_deposit_funds_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let user1: Addr = Addr::unchecked("user1");

        /*
           Test - 1. No funds sent
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::DepositFunds {
                strategy_override: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoFundsSent {}));

        /*
           Test - 2. Multiple coins sent
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                user1.as_str(),
                &[
                    Coin::new(10_u128, "abc".to_string()),
                    Coin::new(10_u128, "def".to_string()),
                ],
            ),
            ExecuteMsg::DepositFunds {
                strategy_override: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::MultipleCoinsSent {}));

        /*
           Test - 3. Wrong denom sent
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[Coin::new(10_u128, "abc".to_string())]),
            ExecuteMsg::DepositFunds {
                strategy_override: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::WrongDenomSent {}));

        /*
           Test - 4. 0 funds sent
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[Coin::new(0_u128, "uluna".to_string())]),
            ExecuteMsg::DepositFunds {
                strategy_override: None,
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "0 funds sent".to_string(),
                value: "1".to_string()
            }]
        ));
    }

    #[test]
    fn test_deposit_funds_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            Some(vec![UserStrategyPortfolio {
                strategy_id: 1,
                deposit_fraction: Uint128::new(100_u128),
            }]),
        );

        let user1 = Addr::unchecked("user1");
        let sic1_address = Addr::unchecked("sic1_address");
        let sic2_address = Addr::unchecked("sic2_address");
        let sic3_address = Addr::unchecked("sic3_address");

        /*
           Test - 1. User deposits for first time
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 1000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(200_u128, "anc".to_string()),
                        Coin::new(300_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::zero(),
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[Coin::new(100_u128, "uluna".to_string())]),
            ExecuteMsg::DepositFunds {
                strategy_override: None,
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: sic1_address.to_string(),
                msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                funds: vec![Coin::new(100_u128, "uluna".to_string())]
            })]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(
            sid1_strategy_info.total_shares,
            Decimal::from_ratio(2000_u128, 1_u128)
        );

        let user_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user_reward_info_opt, None);
        let user_reward_info = user_reward_info_opt.unwrap();
        assert!(check_equal_reward_info(
            user_reward_info,
            UserRewardInfo {
                user_portfolio: vec![UserStrategyPortfolio {
                    strategy_id: 1,
                    deposit_fraction: Uint128::new(100_u128)
                }],
                strategies: vec![UserStrategyInfo {
                    strategy_id: 1,
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 1000_u128), "mir".to_string()),
                    ]
                }],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Uint128::zero()
            }
        ));

        /*
           Test - 2. User has an existing portfolio
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic3_address.clone(), Uint128::new(0_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 1000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(200_u128, "anc".to_string()),
                        Coin::new(300_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(2000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 2000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 2000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(200_u128, "anc".to_string()),
                        Coin::new(300_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: sic3_address.clone(),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(4000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 4000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 4000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(200_u128, "anc".to_string()),
                        Coin::new(300_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![
                        UserStrategyPortfolio {
                            strategy_id: 1,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 2,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 3,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                    ],
                    strategies: vec![
                        UserStrategyInfo {
                            strategy_id: 1,
                            shares: Decimal::from_ratio(500_u128, 1_u128),
                            airdrop_pointer: vec![
                                DecCoin::new(
                                    Decimal::from_ratio(100_u128, 1000_u128),
                                    "anc".to_string(),
                                ),
                                DecCoin::new(
                                    Decimal::from_ratio(200_u128, 1000_u128),
                                    "mir".to_string(),
                                ),
                            ],
                        },
                        UserStrategyInfo {
                            strategy_id: 2,
                            shares: Decimal::from_ratio(1000_u128, 1_u128),
                            airdrop_pointer: vec![
                                DecCoin::new(
                                    Decimal::from_ratio(200_u128, 2000_u128),
                                    "anc".to_string(),
                                ),
                                DecCoin::new(
                                    Decimal::from_ratio(300_u128, 2000_u128),
                                    "mir".to_string(),
                                ),
                            ],
                        },
                        UserStrategyInfo {
                            strategy_id: 3,
                            shares: Decimal::from_ratio(500_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                    ],
                    pending_airdrops: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(200_u128, "mir".to_string()),
                    ],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::new(100_u128),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[Coin::new(100_u128, "uluna".to_string())]),
            ExecuteMsg::DepositFunds {
                strategy_override: None,
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 3);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic1_address.to_string(),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(25_u128, "uluna".to_string())]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic2_address.to_string(),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(25_u128, "uluna".to_string())]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic3_address.to_string(),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(25_u128, "uluna".to_string())]
                })
            ]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(
            sid1_strategy_info.total_shares,
            Decimal::from_ratio(1250_u128, 1_u128)
        );
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(sid2_strategy_info_opt, None);
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(
            sid2_strategy_info.total_shares,
            Decimal::from_ratio(2250_u128, 1_u128)
        );
        let sid3_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(3))
            .unwrap();
        assert_ne!(sid3_strategy_info_opt, None);
        let sid3_strategy_info = sid3_strategy_info_opt.unwrap();
        assert_eq!(
            sid3_strategy_info.total_shares,
            Decimal::from_ratio(4250_u128, 1_u128)
        );

        let user_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user_reward_info_opt, None);
        let user_reward_info = user_reward_info_opt.unwrap();
        assert!(check_equal_reward_info(
            user_reward_info,
            UserRewardInfo {
                user_portfolio: vec![
                    UserStrategyPortfolio {
                        strategy_id: 1,
                        deposit_fraction: Uint128::new(25_u128)
                    },
                    UserStrategyPortfolio {
                        strategy_id: 2,
                        deposit_fraction: Uint128::new(25_u128)
                    },
                    UserStrategyPortfolio {
                        strategy_id: 3,
                        deposit_fraction: Uint128::new(25_u128)
                    },
                ],
                strategies: vec![
                    UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(750_u128, 1_u128),
                        airdrop_pointer: vec![
                            DecCoin::new(
                                Decimal::from_ratio(200_u128, 1000_u128),
                                "anc".to_string()
                            ),
                            DecCoin::new(
                                Decimal::from_ratio(300_u128, 1000_u128),
                                "mir".to_string()
                            ),
                        ]
                    },
                    UserStrategyInfo {
                        strategy_id: 2,
                        shares: Decimal::from_ratio(1250_u128, 1_u128),
                        airdrop_pointer: vec![
                            DecCoin::new(
                                Decimal::from_ratio(200_u128, 2000_u128),
                                "anc".to_string()
                            ),
                            DecCoin::new(
                                Decimal::from_ratio(300_u128, 2000_u128),
                                "mir".to_string()
                            ),
                        ]
                    },
                    UserStrategyInfo {
                        strategy_id: 3,
                        shares: Decimal::from_ratio(750_u128, 1_u128),
                        airdrop_pointer: vec![
                            DecCoin::new(
                                Decimal::from_ratio(200_u128, 4000_u128),
                                "anc".to_string()
                            ),
                            DecCoin::new(
                                Decimal::from_ratio(300_u128, 4000_u128),
                                "mir".to_string()
                            ),
                        ]
                    }
                ],
                pending_airdrops: vec![
                    Coin::new(175_u128, "anc".to_string()),
                    Coin::new(287_u128, "mir".to_string())
                ],
                undelegation_records: vec![],
                pending_rewards: Uint128::new(125_u128)
            }
        ));
    }

    #[test]
    fn test_update_strategy_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::UpdateStrategy {
                strategy_id: 2,
                unbonding_period: Some(0),
                unbonding_buffer: Some(0),
                sic_contract_address: None,
                is_active: Some(false),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Strategy does not exist
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateStrategy {
                strategy_id: 1,
                unbonding_period: None,
                unbonding_buffer: None,
                sic_contract_address: None,
                is_active: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::StrategyInfoDoesNotExist {}));
    }

    #[test]
    fn test_update_strategy_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: false,
                    total_shares: Default::default(),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Default::default(),
                },
            )
            .unwrap();

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateStrategy {
                strategy_id: 1,
                unbonding_period: Some(10000),
                unbonding_buffer: Some(15000),
                sic_contract_address: Some("test".to_string()),
                is_active: Some(true),
            },
        )
        .unwrap();

        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.unbonding_period, 10000);
        assert_eq!(sid1_strategy_info.unbonding_buffer, 15000);
        assert_eq!(
            sid1_strategy_info.sic_contract_address,
            Addr::unchecked("test")
        );
        assert!(sid1_strategy_info.is_active);
    }

    #[test]
    fn test_update_cw20_contracts_registry_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::RegisterCw20Contracts {
                denom: "anc".to_string(),
                cw20_contract: "abc".to_string(),
                airdrop_contract: "def".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test_update_cw20_contracts_registry_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RegisterCw20Contracts {
                denom: "anc".to_string(),
                cw20_contract: "abc".to_string(),
                airdrop_contract: "def".to_string(),
            },
        )
        .unwrap();

        let anc_contracts_opt = CW20_TOKEN_CONTRACTS_REGISTRY
            .may_load(deps.as_mut().storage, "anc".to_string())
            .unwrap();
        assert_ne!(anc_contracts_opt, None);
        let anc_contracts = anc_contracts_opt.unwrap();
        assert_eq!(anc_contracts.cw20_token_contract, Addr::unchecked("abc"));
        assert_eq!(anc_contracts.airdrop_contract, Addr::unchecked("def"));
    }

    #[test]
    fn test_fetch_undelegated_rewards_from_strategies_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        /*
            Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies { strategies: vec![] },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
            Test - 2. Empty strategies
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies { strategies: vec![] },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "no_strategies".to_string(),
                value: "1".to_string()
            }]
        ));

        /*
           Test - 3. Failed strategies
        */

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec![1, 2],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 4);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "1,2".to_string()
                },
                Attribute {
                    key: "failed_undelegation_batches".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_in_unbonding_period".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_released".to_string(),
                    value: "".to_string()
                }
            ]
        ));

        /*
           Test - 4. Undelegation batches not found
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 2,
                    next_reconciliation_batch_id: 1,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: Addr::unchecked("def"),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 3,
                    next_reconciliation_batch_id: 2,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec![1, 2],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 4);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "failed_undelegation_batches".to_string(),
                    value: "1:1,2:2".to_string()
                },
                Attribute {
                    key: "undelegation_batches_in_unbonding_period".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_released".to_string(),
                    value: "".to_string()
                }
            ]
        ));
        let sid1_strategy_info = STRATEGY_MAP
            .load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        let sid2_strategy_info = STRATEGY_MAP
            .load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_eq!(sid1_strategy_info.next_reconciliation_batch_id, 2);
        assert_eq!(sid1_strategy_info.next_undelegation_batch_id, 2);
        assert_eq!(sid2_strategy_info.next_reconciliation_batch_id, 3);
        assert_eq!(sid2_strategy_info.next_undelegation_batch_id, 3);

        /*
            Test - 5. Undelegation batches in unbonding period
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 2,
                    next_reconciliation_batch_id: 1,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: Addr::unchecked("def"),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 3,
                    next_reconciliation_batch_id: 2,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(1), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(100_u128),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(Timestamp::from_seconds(1631094920)),
                    est_release_time: Option::from(Timestamp::from_seconds(1631094990)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(2), U64Key::new(2)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(Timestamp::from_seconds(1631094920)),
                    est_release_time: Option::from(Timestamp::from_seconds(1631095990)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec![1, 2],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 4);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "failed_undelegation_batches".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_in_unbonding_period".to_string(),
                    value: "1:1,2:2".to_string()
                },
                Attribute {
                    key: "undelegation_batches_released".to_string(),
                    value: "".to_string()
                }
            ]
        ));
        let sid1_strategy_info = STRATEGY_MAP
            .load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        let sid2_strategy_info = STRATEGY_MAP
            .load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_eq!(sid1_strategy_info.next_reconciliation_batch_id, 1);
        assert_eq!(sid1_strategy_info.next_undelegation_batch_id, 2);
        assert_eq!(sid2_strategy_info.next_reconciliation_batch_id, 2);
        assert_eq!(sid2_strategy_info.next_undelegation_batch_id, 3);

        /*
            Test - 6. Undelegation batches have already been accounted for slashing
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 2,
                    next_reconciliation_batch_id: 1,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: Addr::unchecked("def"),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 3,
                    next_reconciliation_batch_id: 2,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(1), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(100_u128),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(Timestamp::from_seconds(123)),
                    est_release_time: Option::from(Timestamp::from_seconds(125)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::Done,
                    released: true,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(2), U64Key::new(2)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Decimal::from_ratio(4000_u128, 1_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(Timestamp::from_seconds(123)),
                    est_release_time: Option::from(Timestamp::from_seconds(125)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::Done,
                    released: true,
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec![1, 2],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 4);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "failed_undelegation_batches".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_in_unbonding_period".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_released".to_string(),
                    value: "1:1,2:2".to_string()
                }
            ]
        ));
        let sid1_strategy_info = STRATEGY_MAP
            .load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        let sid2_strategy_info = STRATEGY_MAP
            .load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_eq!(sid1_strategy_info.next_reconciliation_batch_id, 2);
        assert_eq!(sid1_strategy_info.next_undelegation_batch_id, 2);
        assert_eq!(sid2_strategy_info.next_reconciliation_batch_id, 3);
        assert_eq!(sid2_strategy_info.next_undelegation_batch_id, 3);
    }

    #[test]
    fn test_fetch_undelegated_rewards_from_strategies_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let sic1_address = Addr::unchecked("sic1_address");
        let sic2_address = Addr::unchecked("sic2_address");

        /*
           Test - 1. Strategies have no undelegation slashing
        */
        let mut contracts_to_fulfillable_undelegation: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_fulfillable_undelegation.insert(sic1_address.clone(), Uint128::new(100_u128));
        contracts_to_fulfillable_undelegation.insert(sic2_address.clone(), Uint128::new(400_u128));
        deps.querier
            .update_stader_balances(None, Some(contracts_to_fulfillable_undelegation));

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 2,
                    next_reconciliation_batch_id: 1,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 3,
                    next_reconciliation_batch_id: 2,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(1), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(100_u128),
                    shares: Default::default(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Default::default(),
                    create_time: Option::from(Timestamp::from_seconds(123)),
                    est_release_time: Option::from(Timestamp::from_seconds(125)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(2), U64Key::new(2)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Default::default(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Default::default(),
                    create_time: Option::from(Timestamp::from_seconds(123)),
                    est_release_time: Option::from(Timestamp::from_seconds(125)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec![1, 2],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 4);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "failed_undelegation_batches".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_in_unbonding_period".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_released".to_string(),
                    value: "".to_string()
                }
            ]
        ));
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic1_address.clone().to_string(),
                    msg: to_binary(&sic_execute_msg::TransferUndelegatedRewards {
                        amount: Uint128::new(100_u128),
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic2_address.clone().to_string(),
                    msg: to_binary(&sic_execute_msg::TransferUndelegatedRewards {
                        amount: Uint128::new(400_u128),
                    })
                    .unwrap(),
                    funds: vec![]
                }),
            ]
        ));

        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        assert_ne!(sid2_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.next_undelegation_batch_id, 2);
        assert_eq!(sid1_strategy_info.next_reconciliation_batch_id, 2);
        assert_eq!(sid2_strategy_info.next_undelegation_batch_id, 3);
        assert_eq!(sid2_strategy_info.next_reconciliation_batch_id, 3);
        let sid1_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(1), U64Key::new(1)))
            .unwrap();
        let sid2_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(2), U64Key::new(2)))
            .unwrap();
        assert_ne!(sid1_undelegation_batch_opt, None);
        assert_ne!(sid2_undelegation_batch_opt, None);
        let sid1_undelegation_batch = sid1_undelegation_batch_opt.unwrap();
        let sid2_undelegation_batch = sid2_undelegation_batch_opt.unwrap();
        assert!(sid1_undelegation_batch.released);
        assert!(sid2_undelegation_batch.released);
        assert_eq!(
            sid1_undelegation_batch.unbonding_slashing_ratio,
            Decimal::one()
        );
        assert_eq!(
            sid2_undelegation_batch.unbonding_slashing_ratio,
            Decimal::one()
        );
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);

        /*
            Test - 2. Strategies have undelegation slashing
        */
        let mut contracts_to_fulfillable_undelegation: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_fulfillable_undelegation.insert(sic1_address.clone(), Uint128::new(75_u128));
        contracts_to_fulfillable_undelegation.insert(sic2_address.clone(), Uint128::new(200_u128));
        deps.querier
            .update_stader_balances(None, Some(contracts_to_fulfillable_undelegation));

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 2,
                    next_reconciliation_batch_id: 1,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 3,
                    next_reconciliation_batch_id: 2,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(1), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(100_u128),
                    shares: Default::default(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Default::default(),
                    create_time: Option::from(Timestamp::from_seconds(123)),
                    est_release_time: Option::from(Timestamp::from_seconds(125)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(2), U64Key::new(2)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Default::default(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Default::default(),
                    create_time: Option::from(Timestamp::from_seconds(123)),
                    est_release_time: Option::from(Timestamp::from_seconds(125)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec![1, 2],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 4);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "failed_undelegation_batches".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_in_unbonding_period".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_released".to_string(),
                    value: "".to_string()
                }
            ]
        ));
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic1_address.clone().to_string(),
                    msg: to_binary(&sic_execute_msg::TransferUndelegatedRewards {
                        amount: Uint128::new(100_u128),
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic2_address.clone().to_string(),
                    msg: to_binary(&sic_execute_msg::TransferUndelegatedRewards {
                        amount: Uint128::new(400_u128),
                    })
                    .unwrap(),
                    funds: vec![]
                }),
            ]
        ));

        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        assert_ne!(sid2_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.next_undelegation_batch_id, 2);
        assert_eq!(sid1_strategy_info.next_reconciliation_batch_id, 2);
        assert_eq!(sid2_strategy_info.next_undelegation_batch_id, 3);
        assert_eq!(sid2_strategy_info.next_reconciliation_batch_id, 3);
        let sid1_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(1), U64Key::new(1)))
            .unwrap();
        let sid2_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(2), U64Key::new(2)))
            .unwrap();
        assert_ne!(sid1_undelegation_batch_opt, None);
        assert_ne!(sid2_undelegation_batch_opt, None);
        let sid1_undelegation_batch = sid1_undelegation_batch_opt.unwrap();
        let sid2_undelegation_batch = sid2_undelegation_batch_opt.unwrap();
        assert!(sid1_undelegation_batch.released);
        assert!(sid2_undelegation_batch.released);
        assert_eq!(
            sid1_undelegation_batch.unbonding_slashing_ratio,
            Decimal::from_ratio(3_u128, 4_u128)
        );
        assert_eq!(
            sid2_undelegation_batch.unbonding_slashing_ratio,
            Decimal::from_ratio(1_u128, 2_u128)
        );

        /*
            Test - 3. Strategies have surplus of the requested amount
        */
        let mut contracts_to_fulfillable_undelegation: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_fulfillable_undelegation.insert(sic1_address.clone(), Uint128::new(120_u128));
        contracts_to_fulfillable_undelegation.insert(sic2_address.clone(), Uint128::new(500_u128));
        deps.querier
            .update_stader_balances(None, Some(contracts_to_fulfillable_undelegation));

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 2,
                    next_reconciliation_batch_id: 1,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 3,
                    next_reconciliation_batch_id: 2,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(1), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(100_u128),
                    shares: Default::default(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Default::default(),
                    create_time: Option::from(Timestamp::from_seconds(123)),
                    est_release_time: Option::from(Timestamp::from_seconds(125)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(2), U64Key::new(2)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Default::default(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Default::default(),
                    create_time: Option::from(Timestamp::from_seconds(123)),
                    est_release_time: Option::from(Timestamp::from_seconds(125)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec![1, 2],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 4);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "failed_undelegation_batches".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_in_unbonding_period".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_released".to_string(),
                    value: "".to_string()
                }
            ]
        ));
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic1_address.clone().to_string(),
                    msg: to_binary(&sic_execute_msg::TransferUndelegatedRewards {
                        amount: Uint128::new(100_u128),
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic2_address.clone().to_string(),
                    msg: to_binary(&sic_execute_msg::TransferUndelegatedRewards {
                        amount: Uint128::new(400_u128),
                    })
                    .unwrap(),
                    funds: vec![]
                }),
            ]
        ));

        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        assert_ne!(sid2_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.next_undelegation_batch_id, 2);
        assert_eq!(sid1_strategy_info.next_reconciliation_batch_id, 2);
        assert_eq!(sid2_strategy_info.next_undelegation_batch_id, 3);
        assert_eq!(sid2_strategy_info.next_reconciliation_batch_id, 3);
        let sid1_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(1), U64Key::new(1)))
            .unwrap();
        let sid2_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(2), U64Key::new(2)))
            .unwrap();
        assert_ne!(sid1_undelegation_batch_opt, None);
        assert_ne!(sid2_undelegation_batch_opt, None);
        let sid1_undelegation_batch = sid1_undelegation_batch_opt.unwrap();
        let sid2_undelegation_batch = sid2_undelegation_batch_opt.unwrap();
        assert!(sid1_undelegation_batch.released);
        assert!(sid2_undelegation_batch.released);
        assert_eq!(
            sid1_undelegation_batch.unbonding_slashing_ratio,
            Decimal::one()
        );
        assert_eq!(
            sid2_undelegation_batch.unbonding_slashing_ratio,
            Decimal::one()
        );

        /*
            Test - 4. Multiple undelegation batches in unbonding period
        */
        let mut contracts_to_fulfillable_undelegation: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_fulfillable_undelegation.insert(sic1_address.clone(), Uint128::new(120_u128));
        contracts_to_fulfillable_undelegation.insert(sic2_address.clone(), Uint128::new(500_u128));
        deps.querier
            .update_stader_balances(None, Some(contracts_to_fulfillable_undelegation));
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 4,
                    next_reconciliation_batch_id: 1,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 6,
                    next_reconciliation_batch_id: 2,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(2), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(100_u128),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(Timestamp::from_seconds(1631094920)),
                    est_release_time: Option::from(Timestamp::from_seconds(1631094990)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(1), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(100_u128),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(Timestamp::from_seconds(150)),
                    est_release_time: Option::from(Timestamp::from_seconds(155)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(2), U64Key::new(2)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(Timestamp::from_seconds(123)),
                    est_release_time: Option::from(Timestamp::from_seconds(126)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: true,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(3), U64Key::new(2)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(Timestamp::from_seconds(140)),
                    est_release_time: Option::from(Timestamp::from_seconds(145)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(4), U64Key::new(2)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(Timestamp::from_seconds(1631094920)),
                    est_release_time: Option::from(Timestamp::from_seconds(1631095990)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec![1, 2],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 4);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "failed_undelegation_batches".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_in_unbonding_period".to_string(),
                    value: "1:2,2:4".to_string()
                },
                Attribute {
                    key: "undelegation_batches_released".to_string(),
                    value: "2:2".to_string()
                }
            ]
        ));
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic1_address.clone().to_string(),
                    msg: to_binary(&sic_execute_msg::TransferUndelegatedRewards {
                        amount: Uint128::new(100_u128),
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: sic2_address.clone().to_string(),
                    msg: to_binary(&sic_execute_msg::TransferUndelegatedRewards {
                        amount: Uint128::new(400_u128),
                    })
                    .unwrap(),
                    funds: vec![]
                }),
            ]
        ));
        let sid1_strategy_info = STRATEGY_MAP
            .load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        let sid2_strategy_info = STRATEGY_MAP
            .load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_eq!(sid1_strategy_info.next_reconciliation_batch_id, 2);
        assert_eq!(sid1_strategy_info.next_undelegation_batch_id, 4);
        assert_eq!(sid2_strategy_info.next_reconciliation_batch_id, 4);
        assert_eq!(sid2_strategy_info.next_undelegation_batch_id, 6);
        let sid1_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(1), U64Key::new(1)))
            .unwrap();
        assert_ne!(sid1_undelegation_batch_opt, None);
        let sid1_undelegation_batch = sid1_undelegation_batch_opt.unwrap();
        assert!(sid1_undelegation_batch.released);
        assert_eq!(
            sid1_undelegation_batch.unbonding_slashing_ratio,
            Decimal::one()
        );
        let sid2_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(3), U64Key::new(2)))
            .unwrap();
        assert_ne!(sid2_undelegation_batch_opt, None);
        let sid2_undelegation_batch = sid2_undelegation_batch_opt.unwrap();
        assert!(sid2_undelegation_batch.released);
        assert_eq!(
            sid2_undelegation_batch.unbonding_slashing_ratio,
            Decimal::one()
        );
    }

    #[test]
    fn test_undelegate_from_strategies_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("non-creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {
                strategies: vec![1, 2],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}))
    }

    #[test]
    fn test_undelegate_from_strategies_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let sic1_address = Addr::unchecked("sic1_address");
        let sic2_address = Addr::unchecked("sic2_address");
        let sic3_address = Addr::unchecked("sic3_address");

        /*
           Test - 1. Empty strategies
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UndelegateFromStrategies { strategies: vec![] },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "no_strategies".to_string(),
                value: "1".to_string()
            }]
        ));

        /*
           Test - 2. Successful run
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic3_address.clone(), Uint128::new(0_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 4,
                    next_reconciliation_batch_id: 1,
                    is_active: false,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 4,
                    next_reconciliation_batch_id: 1,
                    is_active: false,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(2000_u128, 1_u128),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: sic3_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 4,
                    next_reconciliation_batch_id: 1,
                    is_active: false,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(3000_u128, 1_u128),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(4), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::zero(),
                    shares: Decimal::zero(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(env.block.time),
                    est_release_time: Option::from(env.block.time),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::Pending,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(4), U64Key::new(2)),
                &BatchUndelegationRecord {
                    amount: Uint128::zero(),
                    shares: Decimal::zero(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(env.block.time),
                    est_release_time: Option::from(env.block.time),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::Pending,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(4), U64Key::new(3)),
                &BatchUndelegationRecord {
                    amount: Uint128::zero(),
                    shares: Decimal::zero(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(env.block.time),
                    est_release_time: Option::from(env.block.time),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::Pending,
                    released: false,
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {
                strategies: vec![1, 2, 3],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 2);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "strategies_with_no_undelegations".to_string(),
                    value: "".to_string()
                }
            ]
        ));
        assert_eq!(res.messages.len(), 3);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic1_address.clone()),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(100_u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic2_address.clone()),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(200_u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic3_address.clone()),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(300_u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
            ]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.next_undelegation_batch_id, 5);
        assert_eq!(
            sid1_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        assert_eq!(
            sid1_strategy_info.total_shares,
            Decimal::from_ratio(4000_u128, 1_u128)
        );
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(sid2_strategy_info_opt, None);
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(sid2_strategy_info.next_undelegation_batch_id, 5);
        assert_eq!(
            sid2_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        assert_eq!(
            sid2_strategy_info.total_shares,
            Decimal::from_ratio(3000_u128, 1_u128)
        );
        let sid3_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(3))
            .unwrap();
        assert_ne!(sid3_strategy_info_opt, None);
        let sid3_strategy_info = sid3_strategy_info_opt.unwrap();
        assert_eq!(
            sid3_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        assert_eq!(sid3_strategy_info.next_undelegation_batch_id, 5);
        assert_eq!(
            sid3_strategy_info.total_shares,
            Decimal::from_ratio(2000_u128, 1_u128)
        );

        let undelegation_batch_sid1_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), U64Key::new(1)))
            .unwrap();
        let undelegation_batch_sid2_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), U64Key::new(2)))
            .unwrap();
        let undelegation_batch_sid3_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), U64Key::new(3)))
            .unwrap();
        assert_ne!(undelegation_batch_sid1_opt, None);
        assert_ne!(undelegation_batch_sid2_opt, None);
        assert_ne!(undelegation_batch_sid3_opt, None);
        let undelegation_batch_sid1 = undelegation_batch_sid1_opt.unwrap();
        let undelegation_batch_sid2 = undelegation_batch_sid2_opt.unwrap();
        let undelegation_batch_sid3 = undelegation_batch_sid3_opt.unwrap();
        assert_eq!(
            undelegation_batch_sid1,
            BatchUndelegationRecord {
                amount: Uint128::new(100_u128),
                shares: Decimal::from_ratio(1000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Option::from(env.block.time),
                est_release_time: Option::from(
                    env.block
                        .time
                        .plus_seconds(sid1_strategy_info.unbonding_period)
                ),
                withdrawal_time: Option::from(env.block.time.plus_seconds(
                    sid1_strategy_info.unbonding_period + sid1_strategy_info.unbonding_buffer
                )),
                undelegation_batch_status: UndelegationBatchStatus::InProgress,
                released: false
            }
        );
        assert_eq!(
            undelegation_batch_sid2,
            BatchUndelegationRecord {
                amount: Uint128::new(200_u128),
                shares: Decimal::from_ratio(2000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Option::from(env.block.time),
                est_release_time: Option::from(
                    env.block
                        .time
                        .plus_seconds(sid2_strategy_info.unbonding_period)
                ),
                withdrawal_time: Option::from(env.block.time.plus_seconds(
                    sid2_strategy_info.unbonding_period + sid2_strategy_info.unbonding_buffer
                )),
                undelegation_batch_status: UndelegationBatchStatus::InProgress,
                released: false
            }
        );
        assert_eq!(
            undelegation_batch_sid3,
            BatchUndelegationRecord {
                amount: Uint128::new(300_u128),
                shares: Decimal::from_ratio(3000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Option::from(env.block.time),
                est_release_time: Option::from(
                    env.block
                        .time
                        .plus_seconds(sid3_strategy_info.unbonding_period)
                ),
                withdrawal_time: Option::from(env.block.time.plus_seconds(
                    sid3_strategy_info.unbonding_period + sid3_strategy_info.unbonding_buffer
                )),
                undelegation_batch_status: UndelegationBatchStatus::InProgress,
                released: false
            }
        );

        /*
           Test - 3. Failed strategies
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 4,
                    next_reconciliation_batch_id: 1,
                    is_active: false,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 4,
                    next_reconciliation_batch_id: 1,
                    is_active: false,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(2000_u128, 1_u128),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: sic3_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 4,
                    next_reconciliation_batch_id: 1,
                    is_active: false,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(3000_u128, 1_u128),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(4), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::zero(),
                    shares: Decimal::zero(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(env.block.time),
                    est_release_time: Option::from(env.block.time),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::Pending,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(4), U64Key::new(2)),
                &BatchUndelegationRecord {
                    amount: Uint128::zero(),
                    shares: Decimal::zero(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(env.block.time),
                    est_release_time: Option::from(env.block.time),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::Pending,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(4), U64Key::new(3)),
                &BatchUndelegationRecord {
                    amount: Uint128::zero(),
                    shares: Decimal::zero(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(env.block.time),
                    est_release_time: Option::from(env.block.time),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::Pending,
                    released: false,
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {
                strategies: vec![1, 2, 3, 4],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 2);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "4".to_string()
                },
                Attribute {
                    key: "strategies_with_no_undelegations".to_string(),
                    value: "".to_string()
                }
            ]
        ));
        assert_eq!(res.messages.len(), 3);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic1_address.clone()),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(100_u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic2_address.clone()),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(200_u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic3_address.clone()),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(300_u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
            ]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.next_undelegation_batch_id, 5);
        assert_eq!(
            sid1_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(sid2_strategy_info_opt, None);
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(sid2_strategy_info.next_undelegation_batch_id, 5);
        assert_eq!(
            sid2_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        let sid3_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(3))
            .unwrap();
        assert_ne!(sid3_strategy_info_opt, None);
        let sid3_strategy_info = sid3_strategy_info_opt.unwrap();
        assert_eq!(
            sid3_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        assert_eq!(sid3_strategy_info.next_undelegation_batch_id, 5);
        let undelegation_batch_sid1_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), U64Key::new(1)))
            .unwrap();
        let undelegation_batch_sid2_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), U64Key::new(2)))
            .unwrap();
        let undelegation_batch_sid3_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), U64Key::new(3)))
            .unwrap();
        assert_ne!(undelegation_batch_sid1_opt, None);
        assert_ne!(undelegation_batch_sid2_opt, None);
        assert_ne!(undelegation_batch_sid3_opt, None);
        let undelegation_batch_sid1 = undelegation_batch_sid1_opt.unwrap();
        let undelegation_batch_sid2 = undelegation_batch_sid2_opt.unwrap();
        let undelegation_batch_sid3 = undelegation_batch_sid3_opt.unwrap();
        assert_eq!(
            undelegation_batch_sid1,
            BatchUndelegationRecord {
                amount: Uint128::new(100_u128),
                shares: Decimal::from_ratio(1000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Option::from(env.block.time),
                est_release_time: Option::from(
                    env.block
                        .time
                        .plus_seconds(sid1_strategy_info.unbonding_period)
                ),
                withdrawal_time: Option::from(env.block.time.plus_seconds(
                    sid1_strategy_info.unbonding_period + sid1_strategy_info.unbonding_buffer
                )),
                undelegation_batch_status: UndelegationBatchStatus::InProgress,
                released: false
            }
        );
        assert_eq!(
            undelegation_batch_sid2,
            BatchUndelegationRecord {
                amount: Uint128::new(200_u128),
                shares: Decimal::from_ratio(2000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Option::from(env.block.time),
                est_release_time: Option::from(
                    env.block
                        .time
                        .plus_seconds(sid2_strategy_info.unbonding_period)
                ),
                withdrawal_time: Option::from(env.block.time.plus_seconds(
                    sid2_strategy_info.unbonding_period + sid2_strategy_info.unbonding_buffer
                )),
                undelegation_batch_status: UndelegationBatchStatus::InProgress,
                released: false
            }
        );
        assert_eq!(
            undelegation_batch_sid3,
            BatchUndelegationRecord {
                amount: Uint128::new(300_u128),
                shares: Decimal::from_ratio(3000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Option::from(env.block.time),
                est_release_time: Option::from(
                    env.block
                        .time
                        .plus_seconds(sid3_strategy_info.unbonding_period)
                ),
                withdrawal_time: Option::from(env.block.time.plus_seconds(
                    sid3_strategy_info.unbonding_period + sid3_strategy_info.unbonding_buffer
                )),
                undelegation_batch_status: UndelegationBatchStatus::InProgress,
                released: false
            }
        );

        /*
            Test - 4. Strategies with no undelegations
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 4,
                    next_reconciliation_batch_id: 1,
                    is_active: false,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 4,
                    next_reconciliation_batch_id: 1,
                    is_active: false,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(2000_u128, 1_u128),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: sic3_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 4,
                    next_reconciliation_batch_id: 1,
                    is_active: false,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(4), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::zero(),
                    shares: Decimal::zero(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(env.block.time),
                    est_release_time: Option::from(env.block.time),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::Pending,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(4), U64Key::new(2)),
                &BatchUndelegationRecord {
                    amount: Uint128::zero(),
                    shares: Decimal::zero(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(env.block.time),
                    est_release_time: Option::from(env.block.time),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::Pending,
                    released: false,
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(4), U64Key::new(3)),
                &BatchUndelegationRecord {
                    amount: Uint128::zero(),
                    shares: Decimal::zero(),
                    unbonding_slashing_ratio: Decimal::one(),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(env.block.time),
                    est_release_time: Option::from(env.block.time),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::Pending,
                    released: false,
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {
                strategies: vec![1, 2, 3],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 2);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "strategies_with_no_undelegations".to_string(),
                    value: "3".to_string()
                }
            ]
        ));
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic1_address.clone()),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(100_u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic2_address.clone()),
                    msg: to_binary(&sic_execute_msg::UndelegateRewards {
                        amount: Uint128::new(200_u128),
                    })
                    .unwrap(),
                    funds: vec![],
                }),
            ]
        ));
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.next_undelegation_batch_id, 5);
        assert_eq!(
            sid1_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(sid2_strategy_info_opt, None);
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(sid2_strategy_info.next_undelegation_batch_id, 5);
        assert_eq!(
            sid2_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        let undelegation_batch_sid1_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), U64Key::new(1)))
            .unwrap();
        let undelegation_batch_sid2_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), U64Key::new(2)))
            .unwrap();
        assert_ne!(undelegation_batch_sid1_opt, None);
        assert_ne!(undelegation_batch_sid2_opt, None);
        let undelegation_batch_sid1 = undelegation_batch_sid1_opt.unwrap();
        let undelegation_batch_sid2 = undelegation_batch_sid2_opt.unwrap();
        assert_eq!(
            undelegation_batch_sid1,
            BatchUndelegationRecord {
                amount: Uint128::new(100_u128),
                shares: Decimal::from_ratio(1000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Option::from(env.block.time),
                est_release_time: Option::from(
                    env.block
                        .time
                        .plus_seconds(sid1_strategy_info.unbonding_period)
                ),
                withdrawal_time: Option::from(env.block.time.plus_seconds(
                    sid1_strategy_info.unbonding_period + sid1_strategy_info.unbonding_buffer
                )),
                undelegation_batch_status: UndelegationBatchStatus::InProgress,
                released: false
            }
        );
        assert_eq!(
            undelegation_batch_sid2,
            BatchUndelegationRecord {
                amount: Uint128::new(200_u128),
                shares: Decimal::from_ratio(2000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Option::from(env.block.time),
                est_release_time: Option::from(
                    env.block
                        .time
                        .plus_seconds(sid2_strategy_info.unbonding_period)
                ),
                withdrawal_time: Option::from(env.block.time.plus_seconds(
                    sid2_strategy_info.unbonding_period + sid2_strategy_info.unbonding_buffer
                )),
                undelegation_batch_status: UndelegationBatchStatus::InProgress,
                released: false
            }
        );
    }

    #[test]
    fn test_withdraw_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let user1 = Addr::unchecked("user1");

        /*
            Test - 1. User reward info does not exist
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: 0,
                strategy_id: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserRewardInfoDoesNotExist {}));

        /*
            Test - 2. User undelegation record does not exist
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(deps.as_mut().storage, &user1, &UserRewardInfo::default())
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: 0,
                strategy_id: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UndelegationRecordNotFound {}));

        /*
           Test - 3. Undelegation is still in unbonding period. withdrawal is none
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    }],
                    pending_airdrops: vec![],
                    undelegation_records: vec![UserUndelegationRecord {
                        id: 0,
                        amount: Uint128::new(100_u128),
                        shares: Default::default(),
                        strategy_id: 1,
                        undelegation_batch_id: 3,
                    }],
                    pending_rewards: Uint128::zero(),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(3), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Default::default(),
                    unbonding_slashing_ratio: Decimal::from_ratio(3_u128, 4_u128),
                    undelegation_s_t_ratio: Default::default(),
                    create_time: Option::from(Timestamp::from_seconds(150)),
                    est_release_time: Option::from(Timestamp::from_seconds(150 + 7200)),
                    withdrawal_time: None,
                    undelegation_batch_status: UndelegationBatchStatus::Pending,
                    released: false,
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: 0,
                strategy_id: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::UndelegationInUnbondingPeriod {}
        ));
        /*
            Test - 4. Undelegation is still in unbonding period. withdrawal is gt block time
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    }],
                    pending_airdrops: vec![],
                    undelegation_records: vec![UserUndelegationRecord {
                        id: 0,
                        amount: Uint128::new(100_u128),
                        shares: Default::default(),
                        strategy_id: 1,
                        undelegation_batch_id: 3,
                    }],
                    pending_rewards: Uint128::zero(),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(3), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Default::default(),
                    unbonding_slashing_ratio: Decimal::from_ratio(3_u128, 4_u128),
                    undelegation_s_t_ratio: Default::default(),
                    create_time: Option::from(Timestamp::from_seconds(150)),
                    est_release_time: Option::from(Timestamp::from_seconds(150 + 7200)),
                    withdrawal_time: Option::from(Timestamp::from_seconds(
                        env.block.time.plus_seconds(15000).seconds(),
                    )),
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: 0,
                strategy_id: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::UndelegationInUnbondingPeriod {}
        ));

        /*
           Test - 5. Undelegation slashing not checked
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    }],
                    pending_airdrops: vec![],
                    undelegation_records: vec![UserUndelegationRecord {
                        id: 0,
                        amount: Uint128::new(100_u128),
                        shares: Default::default(),
                        strategy_id: 1,
                        undelegation_batch_id: 3,
                    }],
                    pending_rewards: Uint128::zero(),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(3), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Default::default(),
                    unbonding_slashing_ratio: Decimal::from_ratio(3_u128, 4_u128),
                    undelegation_s_t_ratio: Default::default(),
                    create_time: Option::from(Timestamp::from_seconds(150)),
                    est_release_time: Option::from(Timestamp::from_seconds(150 + 7200)),
                    withdrawal_time: Option::from(Timestamp::from_seconds(
                        env.block.time.minus_seconds(1000).seconds(),
                    )),
                    undelegation_batch_status: UndelegationBatchStatus::InProgress,
                    released: false,
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: 0,
                strategy_id: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::UndelegationBatchNotReleased {}
        ));
    }

    #[test]
    fn test_withdraw_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let user1 = Addr::unchecked("user1");
        let sic1_address = Addr::unchecked("sic1_address");

        /*
           Test - 1. User has only 1 undelegation record
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::new("sid1".to_string(), sic1_address.clone(), 10, 10),
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(5000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    }],
                    pending_airdrops: vec![],
                    undelegation_records: vec![UserUndelegationRecord {
                        id: 0,
                        amount: Uint128::new(100_u128),
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        strategy_id: 1,
                        undelegation_batch_id: 5,
                    }],
                    pending_rewards: Uint128::zero(),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(5), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Decimal::from_ratio(4000_u128, 1_u128),
                    unbonding_slashing_ratio: Decimal::from_ratio(3_u128, 4_u128),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(Timestamp::from_seconds(150)),
                    est_release_time: Option::from(Timestamp::from_seconds(150 + 7200)),
                    withdrawal_time: Option::from(Timestamp::from_seconds(
                        env.block.time.minus_seconds(1000).seconds(),
                    )),
                    undelegation_batch_status: UndelegationBatchStatus::Done,
                    released: true,
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: 0,
                strategy_id: 1,
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: String::from(user1.clone()),
                amount: vec![Coin::new(75_u128, "uluna".to_string())]
            })]
        ));
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.undelegation_records.len(), 0);

        /*
           Test - 2. User has multiple undelegation records.
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::new("sid1".to_string(), sic1_address.clone(), 10, 10),
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![
                        UserStrategyInfo {
                            strategy_id: 1,
                            shares: Decimal::from_ratio(5000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                        UserStrategyInfo {
                            strategy_id: 2,
                            shares: Decimal::from_ratio(5000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                    ],
                    pending_airdrops: vec![],
                    undelegation_records: vec![
                        UserUndelegationRecord {
                            id: 0,
                            amount: Uint128::new(100_u128),
                            shares: Decimal::from_ratio(1000_u128, 1_u128),
                            strategy_id: 1,
                            undelegation_batch_id: 5,
                        },
                        UserUndelegationRecord {
                            id: 1,
                            amount: Uint128::new(100_u128),
                            shares: Decimal::from_ratio(1000_u128, 1_u128),
                            strategy_id: 2,
                            undelegation_batch_id: 6,
                        },
                    ],
                    pending_rewards: Uint128::zero(),
                },
            )
            .unwrap();
        UNDELEGATION_BATCH_MAP
            .save(
                deps.as_mut().storage,
                (U64Key::new(5), U64Key::new(1)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(400_u128),
                    shares: Decimal::from_ratio(4000_u128, 1_u128),
                    unbonding_slashing_ratio: Decimal::from_ratio(3_u128, 4_u128),
                    undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                    create_time: Option::from(Timestamp::from_seconds(150)),
                    est_release_time: Option::from(Timestamp::from_seconds(150 + 7200)),
                    withdrawal_time: Option::from(Timestamp::from_seconds(
                        env.block.time.minus_seconds(1000).seconds(),
                    )),
                    undelegation_batch_status: UndelegationBatchStatus::Done,
                    released: true,
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: 0,
                strategy_id: 1,
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: String::from(user1.clone()),
                amount: vec![Coin::new(75_u128, "uluna".to_string())]
            })]
        ));
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.undelegation_records.len(), 1);
        assert!(check_equal_vec(
            user1_reward_info.undelegation_records,
            vec![UserUndelegationRecord {
                id: 1,
                amount: Uint128::new(100_u128),
                shares: Decimal::from_ratio(1000_u128, 1_u128),
                strategy_id: 2,
                undelegation_batch_id: 6,
            }]
        ));
    }

    #[test]
    fn test_undelegate_user_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let user1 = Addr::unchecked("user1");
        let sic1_address = Addr::unchecked("sic1_address");

        /*
           Test - 1. Zero funds undelegations
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::zero(),
                strategy_id: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::CannotUndelegateZeroFunds {}));

        /*
           Test - 2. Strategy info does not exist
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(100_u128),
                strategy_id: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::StrategyInfoDoesNotExist {}));

        /*
           Test - 3. User reward info does not exist
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(100_u128),
                strategy_id: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserRewardInfoDoesNotExist {}));

        /*
           Test - 4. User did not deposit to strategy
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(deps.as_mut().storage, &user1, &UserRewardInfo::default())
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(100_u128),
                strategy_id: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserNotInStrategy {}));

        /*
           Test - 5. User did not have enough shares to undelegate
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(3000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    }],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::zero(),
                },
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(400_u128),
                strategy_id: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::UserDoesNotHaveEnoughRewards {}
        ));

        /*
           Test - 6. User withdrew from strategy_id 0 with user reward info not present
        */
        let user2 = Addr::unchecked("user2");
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user2.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(400_u128),
                strategy_id: 0,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserRewardInfoDoesNotExist {}));

        /*
           Test - 7. User did not have enough pending_rewards
        */
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(3000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    }],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::new(100_u128),
                },
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(400_u128),
                strategy_id: 0,
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::UserDoesNotHaveEnoughRewards {}
        ));

        /*
            Test - 8. User has 0 pending_rewards
        */
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(3000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    }],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::zero(),
                },
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(400_u128),
                strategy_id: 0,
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::UserDoesNotHaveEnoughRewards {}
        ));
    }

    #[test]
    fn test_undelegate_user_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let user1 = Addr::unchecked("user1");
        let sic1_address = Addr::unchecked("sic1_address");
        let _sic2_address = Addr::unchecked("sic2_address");

        /*
           Test - 1. User undelegates for the first time from a strategy
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 5000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(3000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    }],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::zero(),
                },
            )
            .unwrap();
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.next_strategy_id = 0;
                    Ok(state)
                },
            )
            .unwrap();

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(200_u128),
                strategy_id: 1,
            },
        )
        .unwrap();
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(
            sid1_strategy_info.current_undelegated_shares,
            Decimal::from_ratio(2000_u128, 1_u128)
        );

        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert!(check_equal_vec(
            user1_reward_info.strategies,
            vec![UserStrategyInfo {
                strategy_id: 1,
                shares: Decimal::from_ratio(1000_u128, 1_u128),
                airdrop_pointer: vec![
                    DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "anc".to_string()),
                    DecCoin::new(Decimal::from_ratio(400_u128, 5000_u128), "mir".to_string()),
                ]
            }]
        ));
        assert!(check_equal_vec(
            user1_reward_info.pending_airdrops,
            vec![
                Coin::new(120_u128, "anc".to_string()),
                Coin::new(240_u128, "mir".to_string())
            ]
        ));
        assert!(check_equal_vec(
            user1_reward_info.undelegation_records,
            vec![UserUndelegationRecord {
                id: 0,
                amount: Uint128::new(200_u128),
                shares: Decimal::from_ratio(2000_u128, 1_u128),
                strategy_id: 1,
                undelegation_batch_id: 0
            }]
        ));
        let undelegation_batch_info_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(0), U64Key::new(1)))
            .unwrap();
        assert_ne!(undelegation_batch_info_opt, None);
        let undelegation_batch_info = undelegation_batch_info_opt.unwrap();
        assert_eq!(
            undelegation_batch_info,
            BatchUndelegationRecord {
                amount: Uint128::zero(),
                shares: Decimal::zero(),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: None,
                est_release_time: None,
                withdrawal_time: None,
                undelegation_batch_status: UndelegationBatchStatus::Pending,
                released: false
            }
        );
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.next_undelegation_id, 1);

        /*
           Test - 2. User undelegates again
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.next_strategy_id = 1;
                    Ok(state)
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(3000_u128, 1_u128),
                    current_undelegated_shares: Decimal::from_ratio(2000_u128, 1_u128),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 5000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        airdrop_pointer: vec![
                            DecCoin::new(
                                Decimal::from_ratio(200_u128, 5000_u128),
                                "anc".to_string(),
                            ),
                            DecCoin::new(
                                Decimal::from_ratio(400_u128, 5000_u128),
                                "mir".to_string(),
                            ),
                        ],
                    }],
                    pending_airdrops: vec![
                        Coin::new(120_u128, "anc".to_string()),
                        Coin::new(240_u128, "mir".to_string()),
                    ],
                    undelegation_records: vec![UserUndelegationRecord {
                        id: 0,
                        amount: Uint128::new(200_u128),
                        shares: Decimal::from_ratio(2000_u128, 1_u128),
                        strategy_id: 1,
                        undelegation_batch_id: 0,
                    }],
                    pending_rewards: Uint128::zero(),
                },
            )
            .unwrap();

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(50_u128),
                strategy_id: 1,
            },
        )
        .unwrap();
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(
            sid1_strategy_info.current_undelegated_shares,
            Decimal::from_ratio(2500_u128, 1_u128)
        );

        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert!(check_equal_vec(
            user1_reward_info.strategies,
            vec![UserStrategyInfo {
                strategy_id: 1,
                shares: Decimal::from_ratio(500_u128, 1_u128),
                airdrop_pointer: vec![
                    DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "anc".to_string()),
                    DecCoin::new(Decimal::from_ratio(400_u128, 5000_u128), "mir".to_string()),
                ]
            }]
        ));
        assert!(check_equal_vec(
            user1_reward_info.pending_airdrops,
            vec![
                Coin::new(120_u128, "anc".to_string()),
                Coin::new(240_u128, "mir".to_string())
            ]
        ));
        assert!(check_equal_vec(
            user1_reward_info.undelegation_records,
            vec![
                UserUndelegationRecord {
                    id: 0,
                    amount: Uint128::new(200_u128),
                    shares: Decimal::from_ratio(2000_u128, 1_u128),
                    strategy_id: 1,
                    undelegation_batch_id: 0
                },
                UserUndelegationRecord {
                    id: 1,
                    amount: Uint128::new(50_u128),
                    shares: Decimal::from_ratio(500_u128, 1_u128),
                    strategy_id: 1,
                    undelegation_batch_id: 0
                },
            ]
        ));
        let undelegation_batch_info_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(0), U64Key::new(1)))
            .unwrap();
        assert_ne!(undelegation_batch_info_opt, None);
        let undelegation_batch_info = undelegation_batch_info_opt.unwrap();
        assert_eq!(
            undelegation_batch_info,
            BatchUndelegationRecord {
                amount: Uint128::zero(),
                shares: Decimal::zero(),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: None,
                est_release_time: None,
                withdrawal_time: None,
                undelegation_batch_status: UndelegationBatchStatus::Pending,
                released: false
            }
        );
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.next_undelegation_id, 2);

        /*
           Test - 3. User undelegates from strategy_id 0 with some pending rewards
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.next_strategy_id = 1;
                    state.rewards_in_scc = Uint128::new(100_u128);
                    Ok(state)
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        airdrop_pointer: vec![
                            DecCoin::new(
                                Decimal::from_ratio(200_u128, 5000_u128),
                                "anc".to_string(),
                            ),
                            DecCoin::new(
                                Decimal::from_ratio(400_u128, 5000_u128),
                                "mir".to_string(),
                            ),
                        ],
                    }],
                    pending_airdrops: vec![
                        Coin::new(120_u128, "anc".to_string()),
                        Coin::new(240_u128, "mir".to_string()),
                    ],
                    undelegation_records: vec![UserUndelegationRecord {
                        id: 0,
                        amount: Uint128::new(200_u128),
                        shares: Decimal::from_ratio(2000_u128, 1_u128),
                        strategy_id: 1,
                        undelegation_batch_id: 0,
                    }],
                    pending_rewards: Uint128::new(50_u128),
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(50_u128),
                strategy_id: 0,
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: user1.to_string(),
                amount: vec![Coin::new(50_u128, "uluna".to_string())]
            })]
        ));

        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.pending_rewards, Uint128::zero());
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.rewards_in_scc, Uint128::new(50_u128));

        /*
            Test - 5. User partially undelegates from strategy_id 0
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.next_strategy_id = 1;
                    state.rewards_in_scc = Uint128::new(300_u128);
                    Ok(state)
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        airdrop_pointer: vec![
                            DecCoin::new(
                                Decimal::from_ratio(200_u128, 5000_u128),
                                "anc".to_string(),
                            ),
                            DecCoin::new(
                                Decimal::from_ratio(400_u128, 5000_u128),
                                "mir".to_string(),
                            ),
                        ],
                    }],
                    pending_airdrops: vec![
                        Coin::new(120_u128, "anc".to_string()),
                        Coin::new(240_u128, "mir".to_string()),
                    ],
                    undelegation_records: vec![UserUndelegationRecord {
                        id: 0,
                        amount: Uint128::new(200_u128),
                        shares: Decimal::from_ratio(2000_u128, 1_u128),
                        strategy_id: 1,
                        undelegation_batch_id: 0,
                    }],
                    pending_rewards: Uint128::new(100_u128),
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(50_u128),
                strategy_id: 0,
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: user1.to_string(),
                amount: vec![Coin::new(50_u128, "uluna".to_string())]
            })]
        ));

        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.pending_rewards, Uint128::new(50_u128));
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.rewards_in_scc, Uint128::new(250_u128));
    }

    #[test]
    fn test_claim_airdrops_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        fn get_airdrop_claim_msg() -> Binary {
            Binary::from(vec![1, 2, 3, 4, 5, 6, 7, 8, 9])
        }

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: 1,
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
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: 1,
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
        CW20_TOKEN_CONTRACTS_REGISTRY
            .save(
                deps.as_mut().storage,
                "anc".to_string(),
                &Cw20TokenContractsInfo {
                    airdrop_contract: Addr::unchecked("abc"),
                    cw20_token_contract: Addr::unchecked("def"),
                },
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: 1,
                amount: Uint128::new(100_u128),
                denom: "anc".to_string(),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::StrategyInfoDoesNotExist {}));

        /*
           Test - 4. Zero amount
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: 1,
                amount: Uint128::zero(),
                denom: "anc".to_string(),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ZeroAmount {}));
    }

    #[test]
    fn test_claim_airdrops_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let anc_cw20_contract: Addr = Addr::unchecked("anc-cw20-contract");
        let mir_cw20_contract: Addr = Addr::unchecked("mir-cw20-contract");
        let anc_airdrop_contract: Addr = Addr::unchecked("anc-airdrop-contract");
        let mir_airdrop_contract: Addr = Addr::unchecked("mir-airdrop-contract");

        let sic_contract: Addr = Addr::unchecked("sic-contract");

        fn get_airdrop_claim_msg() -> Binary {
            Binary::from(vec![1, 2, 3, 4, 5, 6, 7, 8, 9])
        }

        CW20_TOKEN_CONTRACTS_REGISTRY
            .save(
                deps.as_mut().storage,
                "anc".to_string(),
                &Cw20TokenContractsInfo {
                    airdrop_contract: anc_airdrop_contract.clone(),
                    cw20_token_contract: anc_cw20_contract.clone(),
                },
            )
            .unwrap();
        CW20_TOKEN_CONTRACTS_REGISTRY
            .save(
                deps.as_mut().storage,
                "mir".to_string(),
                &Cw20TokenContractsInfo {
                    airdrop_contract: mir_airdrop_contract.clone(),
                    cw20_token_contract: mir_cw20_contract.clone(),
                },
            )
            .unwrap();

        /*
           Test - 1. Claiming airdrops from the sic for the first time
        */
        let mut strategy_info = StrategyInfo::new("sid1".to_string(), sic_contract.clone(), 10, 10);
        strategy_info.total_shares = Decimal::from_ratio(100_000_000_u128, 1_u128);
        STRATEGY_MAP
            .save(deps.as_mut().storage, U64Key::new(1), &strategy_info)
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: 1,
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

        let strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
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
        let mut strategy_info = StrategyInfo::new("sid1".to_string(), sic_contract.clone(), 10, 10);
        strategy_info.total_shares = Decimal::from_ratio(100_000_000_u128, 1_u128);
        strategy_info.global_airdrop_pointer = vec![DecCoin::new(
            Decimal::from_ratio(100_u128, 100_000_000_u128),
            "anc".to_string(),
        )];
        strategy_info.total_airdrops_accumulated = vec![Coin::new(100_u128, "anc".to_string())];

        STRATEGY_MAP
            .save(deps.as_mut().storage, U64Key::new(1), &strategy_info)
            .unwrap();

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.total_accumulated_airdrops = vec![
                        Coin::new(200_u128, "anc".to_string()),
                        Coin::new(500_u128, "mir".to_string()),
                    ];
                    Ok(state)
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: 1,
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
                    cw20_token_contract: mir_cw20_contract.clone(),
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

        let strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
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

        /*
            Test - 3. Claiming airdrops a mir airdrop with anc airdrop with some undelegated shares
        */
        let mut strategy_info = StrategyInfo::new("sid1".to_string(), sic_contract.clone(), 10, 10);
        strategy_info.total_shares = Decimal::from_ratio(100_000_000_u128, 1_u128);
        strategy_info.global_airdrop_pointer = vec![DecCoin::new(
            Decimal::from_ratio(100_u128, 100_000_000_u128),
            "anc".to_string(),
        )];
        strategy_info.total_airdrops_accumulated = vec![Coin::new(100_u128, "anc".to_string())];
        strategy_info.current_undelegated_shares = Decimal::from_ratio(50_000_000_u128, 1_u128);

        STRATEGY_MAP
            .save(deps.as_mut().storage, U64Key::new(1), &strategy_info)
            .unwrap();

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.total_accumulated_airdrops = vec![
                        Coin::new(200_u128, "anc".to_string()),
                        Coin::new(500_u128, "mir".to_string()),
                    ];
                    Ok(state)
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                strategy_id: 1,
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
                    cw20_token_contract: mir_cw20_contract.clone(),
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

        let strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
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
                    Decimal::from_ratio(100_u128, 50_000_000_u128),
                    "mir".to_string()
                )
            ]
        ));
    }

    #[test]
    fn test_withdraw_airdrops_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        /*
           Test - 1. User reward info does not exist
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::WithdrawAirdrops {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserRewardInfoDoesNotExist {}));
    }

    #[test]
    fn test_withdraw_airdrops_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
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
        CW20_TOKEN_CONTRACTS_REGISTRY
            .save(
                deps.as_mut().storage,
                "anc".to_string(),
                &Cw20TokenContractsInfo {
                    airdrop_contract: anc_airdrop_contract.clone(),
                    cw20_token_contract: anc_token_contract.clone(),
                },
            )
            .unwrap();
        CW20_TOKEN_CONTRACTS_REGISTRY
            .save(
                deps.as_mut().storage,
                "mir".to_string(),
                &Cw20TokenContractsInfo {
                    airdrop_contract: mir_airdrop_contract.clone(),
                    cw20_token_contract: mir_token_contract.clone(),
                },
            )
            .unwrap();

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: (21 * 24 * 3600),
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
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
            )
            .unwrap();

        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(5000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    }],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Default::default(),
                },
            )
            .unwrap();
        STATE
            .update(deps.as_mut().storage, |mut state| -> StdResult<_> {
                state.total_accumulated_airdrops = vec![
                    Coin::new(500_u128, "anc".to_string()),
                    Coin::new(700_u128, "mir".to_string()),
                ];
                Ok(state)
            })
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

        let user_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        assert_ne!(user_reward_info_opt, None);
        let user_reward_info = user_reward_info_opt.unwrap();
        assert!(check_equal_vec(
            user_reward_info.pending_airdrops,
            vec![
                Coin::new(0_u128, "anc".to_string()),
                Coin::new(0_u128, "mir".to_string())
            ]
        ));

        /*
           Test - 2. Not all airdrops are transferred
        */
        CW20_TOKEN_CONTRACTS_REGISTRY
            .save(
                deps.as_mut().storage,
                "anc".to_string(),
                &Cw20TokenContractsInfo {
                    airdrop_contract: anc_airdrop_contract.clone(),
                    cw20_token_contract: anc_token_contract.clone(),
                },
            )
            .unwrap();
        CW20_TOKEN_CONTRACTS_REGISTRY
            .save(
                deps.as_mut().storage,
                "mir".to_string(),
                &Cw20TokenContractsInfo {
                    airdrop_contract: mir_airdrop_contract.clone(),
                    cw20_token_contract: mir_token_contract.clone(),
                },
            )
            .unwrap();

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address,
                    unbonding_period: 3600,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Decimal::zero(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(500_u128, 5000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "mir".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 5000_u128), "pyl".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(500_u128, "anc".to_string()),
                        Coin::new(200_u128, "mir".to_string()),
                        Coin::new(400_u128, "pyl".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();

        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![UserStrategyInfo {
                        strategy_id: 1,
                        shares: Decimal::from_ratio(5000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    }],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Default::default(),
                },
            )
            .unwrap();
        STATE
            .update(deps.as_mut().storage, |mut state| -> StdResult<_> {
                state.total_accumulated_airdrops = vec![
                    Coin::new(500_u128, "anc".to_string()),
                    Coin::new(700_u128, "mir".to_string()),
                    Coin::new(400_u128, "pyl".to_string()),
                ];
                Ok(state)
            })
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
        let user_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        assert_ne!(user_reward_info_opt, None);
        let user_reward_info = user_reward_info_opt.unwrap();
        assert!(check_equal_vec(
            user_reward_info.pending_airdrops,
            vec![
                Coin::new(0_u128, "anc".to_string()),
                Coin::new(0_u128, "mir".to_string()),
                Coin::new(400_u128, "pyl".to_string())
            ]
        ));
    }

    #[test]
    fn test_update_user_portfolio_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let user1 = Addr::unchecked("user1");
        /*
            Test - 1. Strategy does not exist
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::UpdateUserPortfolio {
                user_portfolio: vec![UserStrategyPortfolio {
                    strategy_id: 1,
                    deposit_fraction: Uint128::new(100_u128),
                }],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InvalidUserPortfolio {}));

        /*
           Test - 2. Adding an invalid portfolio which causes the entire deposit fraction to go beyond 1
        */
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![
                        UserStrategyPortfolio {
                            strategy_id: 1,
                            deposit_fraction: Uint128::new(50_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 2,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                    ],
                    strategies: vec![],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Default::default(),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo::default("sid3".to_string()),
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("user1", &[]),
            ExecuteMsg::UpdateUserPortfolio {
                user_portfolio: vec![UserStrategyPortfolio {
                    strategy_id: 3,
                    deposit_fraction: Uint128::new(133_u128),
                }],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InvalidUserPortfolio {}));

        /*
            Test - 3. Updating an existing portfolio which causes the entire deposit fraction to go beyond 1
        */
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![
                        UserStrategyPortfolio {
                            strategy_id: 1,
                            deposit_fraction: Uint128::new(50_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 2,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                    ],
                    strategies: vec![],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Default::default(),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo::default("sid2".to_string()),
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("user1", &[]),
            ExecuteMsg::UpdateUserPortfolio {
                user_portfolio: vec![
                    UserStrategyPortfolio {
                        strategy_id: 1,
                        deposit_fraction: Uint128::new(50_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_id: 2,
                        deposit_fraction: Uint128::new(75_u128),
                    },
                ],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InvalidUserPortfolio {}));
    }

    #[test]
    fn test_update_user_portfolio_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let user1 = Addr::unchecked("user1");

        /*
           Test - 1. New user
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo::default("sid2".to_string()),
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![UserStrategyPortfolio {
                        strategy_id: 1,
                        deposit_fraction: Uint128::new(50_u128),
                    }],
                    strategies: vec![],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Default::default(),
                },
            )
            .unwrap();
        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("user1", &[]),
            ExecuteMsg::UpdateUserPortfolio {
                user_portfolio: vec![
                    UserStrategyPortfolio {
                        strategy_id: 1,
                        deposit_fraction: Uint128::new(50_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_id: 2,
                        deposit_fraction: Uint128::new(25_u128),
                    },
                ],
            },
        )
        .unwrap();
        let user_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user_reward_info_opt, None);
        let user_reward_info = user_reward_info_opt.unwrap();
        assert!(check_equal_vec(
            user_reward_info.user_portfolio,
            vec![
                UserStrategyPortfolio {
                    strategy_id: 1,
                    deposit_fraction: Uint128::new(50_u128),
                },
                UserStrategyPortfolio {
                    strategy_id: 2,
                    deposit_fraction: Uint128::new(25_u128),
                },
            ]
        ));
    }

    #[test]
    fn test_update_user_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let user1 = Addr::unchecked("user1");

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-pools", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Empty user requests
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_delegator_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "zero_update_user_rewards_requests".to_string(),
                value: "1".to_string()
            }]
        ));

        /*
           Test - 3. User sends 0 funds
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_delegator_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(0_u128),
                    strategy_id: None,
                }],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 2);
        assert_eq!(res.messages.len(), 0);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "users_with_zero_deposits".to_string(),
                    value: "user1".to_string()
                }
            ]
        ));
    }

    #[test]
    fn test_update_user_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let user1 = Addr::unchecked("user1");
        let user2 = Addr::unchecked("user2");
        let user3 = Addr::unchecked("user3");
        let sic1_address = Addr::unchecked("sic1_address");
        let sic2_address = Addr::unchecked("sic2_address");
        let sic3_address = Addr::unchecked("sic3_address");

        /*
           Test - 1. User deposits to a new strategy for the first time(no user_reward_info)
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_delegator_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(100_u128),
                    strategy_id: Some(1),
                }],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(sic1_address.clone()),
                msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                funds: vec![Coin::new(100_u128, "uluna".to_string())]
            })]
        ));
        let sid1_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_opt, None);
        let sid1_strategy = sid1_strategy_opt.unwrap();
        assert_eq!(
            sid1_strategy.total_shares,
            Decimal::from_ratio(2000_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert!(check_equal_user_strategies(
            user1_reward_info.strategies,
            vec![UserStrategyInfo {
                strategy_id: 1,
                shares: Decimal::from_ratio(1000_u128, 1_u128),
                airdrop_pointer: vec![]
            }]
        ));
        assert_eq!(user1_reward_info.pending_rewards, Uint128::zero());
        assert_eq!(user1_reward_info.user_portfolio.len(), 0);
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.rewards_in_scc, Uint128::new(0_u128));

        /*
           Test - 2. User deposits to an already deposited strategy on-demand
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic3_address.clone(), Uint128::new(0_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.rewards_in_scc = Uint128::new(200_u128);
                    Ok(state)
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: (21 * 24 * 3600),
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
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
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(7000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: sic3_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(3000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 3000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(600_u128, 3000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![
                        UserStrategyPortfolio {
                            strategy_id: 1,
                            deposit_fraction: Uint128::new(50_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 2,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 3,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                    ],
                    strategies: vec![
                        UserStrategyInfo {
                            strategy_id: 1,
                            shares: Decimal::from_ratio(2000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                        UserStrategyInfo {
                            strategy_id: 2,
                            shares: Decimal::from_ratio(3000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                        UserStrategyInfo {
                            strategy_id: 3,
                            shares: Decimal::from_ratio(500_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                    ],
                    pending_airdrops: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(50_u128, "mir".to_string()),
                    ],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::new(200_u128),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_delegator_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(500_u128),
                    strategy_id: Some(3),
                }],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(sic3_address.clone()),
                msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                funds: vec![Coin::new(500_u128, "uluna".to_string())]
            })]
        ));
        let sid3_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(3))
            .unwrap();
        assert_ne!(sid3_strategy_opt, None);
        let sid3_strategy = sid3_strategy_opt.unwrap();
        assert_eq!(
            sid3_strategy.total_shares,
            Decimal::from_ratio(8000_u128, 1_u128)
        );
        assert_eq!(
            sid3_strategy.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert!(check_equal_user_strategies(
            user1_reward_info.strategies,
            vec![
                UserStrategyInfo {
                    strategy_id: 1,
                    shares: Decimal::from_ratio(2000_u128, 1_u128),
                    airdrop_pointer: vec![]
                },
                UserStrategyInfo {
                    strategy_id: 2,
                    shares: Decimal::from_ratio(3000_u128, 1_u128),
                    airdrop_pointer: vec![]
                },
                UserStrategyInfo {
                    strategy_id: 3,
                    shares: Decimal::from_ratio(5500_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 3000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(600_u128, 3000_u128), "mir".to_string()),
                    ]
                },
            ]
        ));
        assert_eq!(user1_reward_info.pending_rewards, Uint128::new(200_u128));
        assert_eq!(user1_reward_info.user_portfolio.len(), 3);
        assert!(check_equal_vec(
            user1_reward_info.user_portfolio,
            vec![
                UserStrategyPortfolio {
                    strategy_id: 1,
                    deposit_fraction: Uint128::new(50_u128)
                },
                UserStrategyPortfolio {
                    strategy_id: 2,
                    deposit_fraction: Uint128::new(25_u128)
                },
                UserStrategyPortfolio {
                    strategy_id: 3,
                    deposit_fraction: Uint128::new(25_u128)
                },
            ]
        ));
        assert!(check_equal_vec(
            user1_reward_info.pending_airdrops,
            vec![
                Coin::new(133_u128, "anc".to_string()),
                Coin::new(150_u128, "mir".to_string())
            ]
        ));
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.rewards_in_scc, Uint128::new(200_u128));

        /*
           Test - 3. User deposits to a strategy which has been slashed
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic3_address.clone(), Uint128::new(100_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.rewards_in_scc = Uint128::new(200_u128);
                    Ok(state)
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: (21 * 24 * 3600),
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
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
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(7000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: sic3_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(3000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 3000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(600_u128, 3000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![
                        UserStrategyPortfolio {
                            strategy_id: 1,
                            deposit_fraction: Uint128::new(50_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 2,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 3,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                    ],
                    strategies: vec![
                        UserStrategyInfo {
                            strategy_id: 1,
                            shares: Decimal::from_ratio(2000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                        UserStrategyInfo {
                            strategy_id: 2,
                            shares: Decimal::from_ratio(3000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                        UserStrategyInfo {
                            strategy_id: 3,
                            shares: Decimal::from_ratio(500_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                    ],
                    pending_airdrops: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(50_u128, "mir".to_string()),
                    ],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::new(200_u128),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_delegator_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(500_u128),
                    strategy_id: Some(3),
                }],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: String::from(sic3_address.clone()),
                msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                funds: vec![Coin::new(500_u128, "uluna".to_string())]
            })]
        ));
        let sid3_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(3))
            .unwrap();
        assert_ne!(sid3_strategy_opt, None);
        let sid3_strategy = sid3_strategy_opt.unwrap();
        assert_eq!(
            sid3_strategy.total_shares,
            Decimal::from_ratio(18000_u128, 1_u128)
        );
        assert_eq!(
            sid3_strategy.shares_per_token_ratio,
            Decimal::from_ratio(30_u128, 1_u128)
        );
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert!(check_equal_user_strategies(
            user1_reward_info.strategies,
            vec![
                UserStrategyInfo {
                    strategy_id: 1,
                    shares: Decimal::from_ratio(2000_u128, 1_u128),
                    airdrop_pointer: vec![]
                },
                UserStrategyInfo {
                    strategy_id: 2,
                    shares: Decimal::from_ratio(3000_u128, 1_u128),
                    airdrop_pointer: vec![]
                },
                UserStrategyInfo {
                    strategy_id: 3,
                    shares: Decimal::from_ratio(15500_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 3000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(600_u128, 3000_u128), "mir".to_string()),
                    ]
                },
            ]
        ));
        assert_eq!(user1_reward_info.pending_rewards, Uint128::new(200_u128));
        assert_eq!(user1_reward_info.user_portfolio.len(), 3);
        assert!(check_equal_vec(
            user1_reward_info.user_portfolio,
            vec![
                UserStrategyPortfolio {
                    strategy_id: 1,
                    deposit_fraction: Uint128::new(50_u128)
                },
                UserStrategyPortfolio {
                    strategy_id: 2,
                    deposit_fraction: Uint128::new(25_u128)
                },
                UserStrategyPortfolio {
                    strategy_id: 3,
                    deposit_fraction: Uint128::new(25_u128)
                },
            ]
        ));
        assert!(check_equal_vec(
            user1_reward_info.pending_airdrops,
            vec![
                Coin::new(133_u128, "anc".to_string()),
                Coin::new(150_u128, "mir".to_string())
            ]
        ));
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.rewards_in_scc, Uint128::new(200_u128));

        /*
            Test - 4. User deposits to money and splits it across his portfolio
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.rewards_in_scc = Uint128::new(300_u128);
                    Ok(state)
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: (21 * 24 * 3600),
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 1000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(200_u128, "anc".to_string()),
                        Coin::new(400_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 1000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(100_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![
                        UserStrategyPortfolio {
                            strategy_id: 1,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 2,
                            deposit_fraction: Uint128::new(50_u128),
                        },
                    ],
                    strategies: vec![],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::new(300_u128),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_delegator_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(100_u128),
                    strategy_id: None,
                }],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic1_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(25_u128, "uluna".to_string())]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic2_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(50_u128, "uluna".to_string())]
                }),
            ]
        ));
        let sid1_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_opt, None);
        let sid1_strategy = sid1_strategy_opt.unwrap();
        assert_eq!(
            sid1_strategy.total_shares,
            Decimal::from_ratio(1250_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let sid2_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(sid2_strategy_opt, None);
        let sid2_strategy = sid2_strategy_opt.unwrap();
        assert_eq!(
            sid2_strategy.total_shares,
            Decimal::from_ratio(1500_u128, 1_u128)
        );
        assert_eq!(
            sid2_strategy.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert!(check_equal_user_strategies(
            user1_reward_info.strategies,
            vec![
                UserStrategyInfo {
                    strategy_id: 1,
                    shares: Decimal::from_ratio(250_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 1000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_id: 2,
                    shares: Decimal::from_ratio(500_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 1000_u128), "mir".to_string()),
                    ]
                }
            ]
        ));
        assert_eq!(user1_reward_info.pending_rewards, Uint128::new(325_u128));
        assert_eq!(user1_reward_info.pending_airdrops.len(), 0);
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.rewards_in_scc, Uint128::new(325_u128));

        /*
            Test - 5. User deposits to money but has an empty portfolio
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.rewards_in_scc = Uint128::new(300_u128);
                    Ok(state)
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: (21 * 24 * 3600),
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 1000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(200_u128, "anc".to_string()),
                        Coin::new(400_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 1000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(100_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::new(300_u128),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_delegator_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(100_u128),
                    strategy_id: None,
                }],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        let sid1_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_opt, None);
        let sid1_strategy = sid1_strategy_opt.unwrap();
        assert_eq!(
            sid1_strategy.total_shares,
            Decimal::from_ratio(1000_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let sid2_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(sid2_strategy_opt, None);
        let sid2_strategy = sid2_strategy_opt.unwrap();
        assert_eq!(
            sid2_strategy.total_shares,
            Decimal::from_ratio(1000_u128, 1_u128)
        );
        assert_eq!(
            sid2_strategy.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.strategies.len(), 0);
        assert_eq!(user1_reward_info.pending_rewards, Uint128::new(400_u128));
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.rewards_in_scc, Uint128::new(400_u128));

        /*
           Test - 5. User newly deposits with no strategy portfolio and no strategy specified.
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.rewards_in_scc = Uint128::new(100_u128);
                    Ok(state)
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: (21 * 24 * 3600),
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: (21 * 24 * 3600),
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1.clone(),
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Default::default(),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP.remove(deps.as_mut().storage, &user1);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_delegator_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(100_u128),
                    strategy_id: None,
                }],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        let sid1_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_opt, None);
        let sid1_strategy = sid1_strategy_opt.unwrap();
        assert_eq!(
            sid1_strategy.total_shares,
            Decimal::from_ratio(1000_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let sid2_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(sid2_strategy_opt, None);
        let sid2_strategy = sid2_strategy_opt.unwrap();
        assert_eq!(
            sid2_strategy.total_shares,
            Decimal::from_ratio(1000_u128, 1_u128)
        );
        assert_eq!(
            sid2_strategy.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.strategies.len(), 0);
        assert_eq!(user1_reward_info.pending_rewards, Uint128::new(100_u128));
        assert_eq!(user1_reward_info.user_portfolio.len(), 0);
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.rewards_in_scc, Uint128::new(200_u128));

        /*
           Test - 5. User deposits across his portfolio with existing deposits
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(200_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(100_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.rewards_in_scc = Uint128::new(300_u128);
                    Ok(state)
                },
            )
            .unwrap();

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: (21 * 24 * 3600),
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 1000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(300_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 1000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(200_u128, "anc".to_string()),
                        Coin::new(400_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![
                        UserStrategyPortfolio {
                            strategy_id: 1,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 2,
                            deposit_fraction: Uint128::new(50_u128),
                        },
                    ],
                    strategies: vec![
                        UserStrategyInfo {
                            strategy_id: 1,
                            shares: Decimal::from_ratio(1000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                        UserStrategyInfo {
                            strategy_id: 2,
                            shares: Decimal::from_ratio(500_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                    ],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::new(300_u128),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_delegator_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(100_u128),
                    strategy_id: None,
                }],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic1_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(25_u128, "uluna".to_string())]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic2_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(50_u128, "uluna".to_string())]
                }),
            ]
        ));
        let sid1_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_opt, None);
        let sid1_strategy = sid1_strategy_opt.unwrap();
        assert_eq!(
            sid1_strategy.total_shares,
            Decimal::from_ratio(1125_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy.shares_per_token_ratio,
            Decimal::from_ratio(5_u128, 1_u128)
        );
        let sid2_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(sid2_strategy_opt, None);
        let sid2_strategy = sid2_strategy_opt.unwrap();
        assert_eq!(
            sid2_strategy.total_shares,
            Decimal::from_ratio(1500_u128, 1_u128)
        );
        assert_eq!(
            sid2_strategy.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert!(check_equal_user_strategies(
            user1_reward_info.strategies,
            vec![
                UserStrategyInfo {
                    strategy_id: 1,
                    shares: Decimal::from_ratio(1125_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 1000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_id: 2,
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 1000_u128), "mir".to_string()),
                    ]
                }
            ]
        ));
        assert_eq!(user1_reward_info.pending_rewards, Uint128::new(325_u128));
        assert!(check_equal_vec(
            user1_reward_info.pending_airdrops,
            vec![
                Coin::new(200_u128, "anc".to_string()),
                Coin::new(500_u128, "mir".to_string()),
            ]
        ));
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.rewards_in_scc, Uint128::new(325_u128));

        /*
            Test - 5. Multiple user deposits across their existing portfolios with existing deposits
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic3_address.clone(), Uint128::new(0_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_token), None);

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.rewards_in_scc = Uint128::new(800_u128);
                    Ok(state)
                },
            )
            .unwrap();

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: (21 * 24 * 3600),
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(8000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 8000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 8000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(100_u128, "anc".to_string()),
                        Coin::new(300_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: sic2_address.clone(),
                    unbonding_period: (21 * 24 * 3600),
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(7000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 7000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 7000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(200_u128, "anc".to_string()),
                        Coin::new(400_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: sic3_address.clone(),
                    unbonding_period: (21 * 24 * 3600),
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(300_u128, 5000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(500_u128, 5000_u128), "mir".to_string()),
                    ],
                    total_airdrops_accumulated: vec![
                        Coin::new(300_u128, "anc".to_string()),
                        Coin::new(500_u128, "mir".to_string()),
                    ],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![
                        UserStrategyPortfolio {
                            strategy_id: 1,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 2,
                            deposit_fraction: Uint128::new(50_u128),
                        },
                    ],
                    strategies: vec![
                        UserStrategyInfo {
                            strategy_id: 1,
                            shares: Decimal::from_ratio(2000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                        UserStrategyInfo {
                            strategy_id: 2,
                            shares: Decimal::from_ratio(3000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                    ],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::new(300_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user2,
                &UserRewardInfo {
                    user_portfolio: vec![
                        UserStrategyPortfolio {
                            strategy_id: 1,
                            deposit_fraction: Uint128::new(50_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 2,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 3,
                            deposit_fraction: Uint128::new(12_u128),
                        },
                    ],
                    strategies: vec![
                        UserStrategyInfo {
                            strategy_id: 1,
                            shares: Decimal::from_ratio(1000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                        UserStrategyInfo {
                            strategy_id: 2,
                            shares: Decimal::from_ratio(2000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                        UserStrategyInfo {
                            strategy_id: 3,
                            shares: Decimal::from_ratio(2000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                    ],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::new(500_u128),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user3,
                &UserRewardInfo {
                    user_portfolio: vec![
                        UserStrategyPortfolio {
                            strategy_id: 1,
                            deposit_fraction: Uint128::new(50_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 2,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                        UserStrategyPortfolio {
                            strategy_id: 3,
                            deposit_fraction: Uint128::new(25_u128),
                        },
                    ],
                    strategies: vec![
                        UserStrategyInfo {
                            strategy_id: 1,
                            shares: Decimal::from_ratio(1000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                        UserStrategyInfo {
                            strategy_id: 2,
                            shares: Decimal::from_ratio(2000_u128, 1_u128),
                            airdrop_pointer: vec![],
                        },
                    ],
                    pending_airdrops: vec![],
                    undelegation_records: vec![],
                    pending_rewards: Uint128::new(0_u128),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_delegator_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![
                    UpdateUserRewardsRequest {
                        user: user1.clone(),
                        funds: Uint128::new(100_u128),
                        strategy_id: None,
                    },
                    UpdateUserRewardsRequest {
                        user: user2.clone(),
                        funds: Uint128::new(400_u128),
                        strategy_id: None,
                    },
                    UpdateUserRewardsRequest {
                        user: user3.clone(),
                        funds: Uint128::new(600_u128),
                        strategy_id: None,
                    },
                ],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 3);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic1_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(525_u128, "uluna".to_string())]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic2_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(300_u128, "uluna".to_string())]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(sic3_address.clone()),
                    msg: to_binary(&sic_execute_msg::TransferRewards {}).unwrap(),
                    funds: vec![Coin::new(198_u128, "uluna".to_string())]
                }),
            ]
        ));
        let sid1_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(sid1_strategy_opt, None);
        let sid1_strategy = sid1_strategy_opt.unwrap();
        assert_eq!(
            sid1_strategy.total_shares,
            Decimal::from_ratio(13250_u128, 1_u128)
        );
        assert_eq!(
            sid1_strategy.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let sid2_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(sid2_strategy_opt, None);
        let sid2_strategy = sid2_strategy_opt.unwrap();
        assert_eq!(
            sid2_strategy.total_shares,
            Decimal::from_ratio(10000_u128, 1_u128)
        );
        assert_eq!(
            sid2_strategy.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let sid3_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(3))
            .unwrap();
        assert_ne!(sid3_strategy_opt, None);
        let sid3_strategy = sid3_strategy_opt.unwrap();
        assert_eq!(
            sid3_strategy.total_shares,
            Decimal::from_ratio(6980_u128, 1_u128)
        );
        assert_eq!(
            sid3_strategy.shares_per_token_ratio,
            Decimal::from_ratio(10_u128, 1_u128)
        );
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert!(check_equal_user_strategies(
            user1_reward_info.strategies,
            vec![
                UserStrategyInfo {
                    strategy_id: 1,
                    shares: Decimal::from_ratio(2250_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 8000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 8000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_id: 2,
                    shares: Decimal::from_ratio(3500_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 7000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 7000_u128), "mir".to_string()),
                    ]
                }
            ]
        ));
        assert_eq!(user1_reward_info.pending_rewards, Uint128::new(325_u128));
        assert!(check_equal_vec(
            user1_reward_info.pending_airdrops,
            vec![
                Coin::new(110_u128, "anc".to_string()),
                Coin::new(246_u128, "mir".to_string()),
            ]
        ));
        let user2_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user2)
            .unwrap();
        assert_ne!(user2_reward_info_opt, None);
        let user2_reward_info = user2_reward_info_opt.unwrap();
        assert!(check_equal_user_strategies(
            user2_reward_info.strategies,
            vec![
                UserStrategyInfo {
                    strategy_id: 1,
                    shares: Decimal::from_ratio(3000_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 8000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 8000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_id: 2,
                    shares: Decimal::from_ratio(3000_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 7000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 7000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_id: 3,
                    shares: Decimal::from_ratio(2480_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(300_u128, 5000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(500_u128, 5000_u128), "mir".to_string()),
                    ]
                }
            ]
        ));
        assert_eq!(user2_reward_info.pending_rewards, Uint128::new(552_u128));
        assert!(check_equal_vec(
            user2_reward_info.pending_airdrops,
            vec![
                Coin::new(189_u128, "anc".to_string()),
                Coin::new(351_u128, "mir".to_string()),
            ]
        ));
        let user3_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user3)
            .unwrap();
        assert_ne!(user3_reward_info_opt, None);
        let user3_reward_info = user3_reward_info_opt.unwrap();
        assert!(check_equal_user_strategies(
            user3_reward_info.strategies,
            vec![
                UserStrategyInfo {
                    strategy_id: 1,
                    shares: Decimal::from_ratio(4000_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 8000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 8000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_id: 2,
                    shares: Decimal::from_ratio(3500_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 7000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 7000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_id: 3,
                    shares: Decimal::from_ratio(1500_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(300_u128, 5000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(500_u128, 5000_u128), "mir".to_string()),
                    ]
                }
            ]
        ));
        assert_eq!(user3_reward_info.pending_rewards, Uint128::new(0_u128));
        assert!(check_equal_vec(
            user3_reward_info.pending_airdrops,
            vec![
                Coin::new(69_u128, "anc".to_string()),
                Coin::new(151_u128, "mir".to_string()),
            ]
        ));
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.rewards_in_scc, Uint128::new(877_u128));
    }

    #[test]
    fn test_register_strategy_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::RegisterStrategy {
                strategy_name: "sid".to_string(),
                sic_contract_address: "abc".to_string(),
                unbonding_buffer: 10,
                unbonding_period: 10,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test_register_strategy_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RegisterStrategy {
                strategy_name: "sid1".to_string(),
                sic_contract_address: "abc".to_string(),
                unbonding_buffer: 100u64,
                unbonding_period: 100u64,
            },
        )
        .unwrap();

        let strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap();
        assert_ne!(strategy_info_opt, None);
        let strategy_info = strategy_info_opt.unwrap();
        assert_eq!(
            strategy_info,
            StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 100u64,
                unbonding_buffer: 100,
                next_undelegation_batch_id: 0,
                next_reconciliation_batch_id: 0,
                is_active: false,
                total_shares: Default::default(),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            }
        );

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RegisterStrategy {
                strategy_name: "sid2".to_string(),
                sic_contract_address: "abc".to_string(),
                unbonding_buffer: 100u64,
                unbonding_period: 100u64,
            },
        )
        .unwrap();

        let strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(2))
            .unwrap();
        assert_ne!(strategy_info_opt, None);
        let strategy_info = strategy_info_opt.unwrap();
        assert_eq!(
            strategy_info,
            StrategyInfo {
                name: "sid2".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 100u64,
                unbonding_buffer: 100,
                next_undelegation_batch_id: 0,
                next_reconciliation_batch_id: 0,
                is_active: false,
                total_shares: Default::default(),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            }
        );
    }

    #[test]
    fn test_update_user_airdrops_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
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
            mock_info("delegator_contract", &[]),
            ExecuteMsg::UpdateUserAirdrops {
                update_user_airdrops_requests: vec![],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "zero_user_airdrop_requests".to_string(),
                value: "1".to_string()
            }]
        ));
    }

    #[test]
    fn test_update_user_airdrops_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("uluna")),
            Some(String::from("delegator_contract")),
            None,
        );

        let user1 = Addr::unchecked("user-1");
        let user2 = Addr::unchecked("user-2");
        let user3 = Addr::unchecked("user-3");
        let user4 = Addr::unchecked("user-4");

        /*
           Test - 1. First airdrops
        */
        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("delegator_contract", &[]),
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
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.total_accumulated_airdrops =
                        vec![Coin::new(100_u128, "abc"), Coin::new(200_u128, "def")];
                    Ok(state)
                },
            )
            .unwrap();

        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user1,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![],
                    pending_airdrops: vec![Coin::new(10_u128, "abc"), Coin::new(200_u128, "def")],
                    undelegation_records: vec![],
                    pending_rewards: Default::default(),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user2,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![],
                    pending_airdrops: vec![Coin::new(20_u128, "abc"), Coin::new(100_u128, "def")],
                    undelegation_records: vec![],
                    pending_rewards: Default::default(),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user3,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![],
                    pending_airdrops: vec![Coin::new(30_u128, "abc"), Coin::new(50_u128, "def")],
                    undelegation_records: vec![],
                    pending_rewards: Default::default(),
                },
            )
            .unwrap();
        USER_REWARD_INFO_MAP
            .save(
                deps.as_mut().storage,
                &user4,
                &UserRewardInfo {
                    user_portfolio: vec![],
                    strategies: vec![],
                    pending_airdrops: vec![Coin::new(40_u128, "abc"), Coin::new(80_u128, "def")],
                    undelegation_records: vec![],
                    pending_rewards: Default::default(),
                },
            )
            .unwrap();

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("delegator_contract", &[]),
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

    #[test]
    fn test_validate_user_portfolio() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env, None, None, None);

        /*
           Test - 1. Non-existent strategy
        */
        let err = validate_user_portfolio(
            deps.as_mut().storage,
            &vec![UserStrategyPortfolio {
                strategy_id: 1,
                deposit_fraction: Uint128::new(50_u128),
            }],
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::StrategyInfoDoesNotExist {}));

        /*
            Test - 2. Invalid deposit fraction
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo::default("sid2".to_string()),
            )
            .unwrap();

        let err = validate_user_portfolio(
            deps.as_mut().storage,
            &vec![
                UserStrategyPortfolio {
                    strategy_id: 1,
                    deposit_fraction: Uint128::new(50_u128),
                },
                UserStrategyPortfolio {
                    strategy_id: 2,
                    deposit_fraction: Uint128::new(75_u128),
                },
            ],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::InvalidPortfolioDepositFraction {}
        ));

        /*
            Test - 3. Valid portfolio
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo::default("sid2".to_string()),
            )
            .unwrap();

        let res = validate_user_portfolio(
            deps.as_mut().storage,
            &vec![
                UserStrategyPortfolio {
                    strategy_id: 1,
                    deposit_fraction: Uint128::new(50_u128),
                },
                UserStrategyPortfolio {
                    strategy_id: 2,
                    deposit_fraction: Uint128::new(50_u128),
                },
            ],
        )
        .unwrap();
        assert!(res);
    }

    #[test]
    fn test_get_expected_strategy_or_default() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env, None, None, None);

        /*
           Test - 1. Strategy does not exist or is removed
        */
        let strategy_id = get_expected_strategy_or_default(deps.as_mut().storage, 1, 0).unwrap();
        assert_eq!(strategy_id, 0);

        /*
           Test - 2. Strategy is not active
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: false,
                    total_shares: Default::default(),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Default::default(),
                },
            )
            .unwrap();
        let strategy_id = get_expected_strategy_or_default(deps.as_mut().storage, 1, 0).unwrap();
        assert_eq!(strategy_id, 0);

        /*
           Test - 3. Strategy is good
        */
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Default::default(),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Default::default(),
                },
            )
            .unwrap();
        let strategy_id = get_expected_strategy_or_default(deps.as_mut().storage, 1, 0).unwrap();
        assert_eq!(strategy_id, 1);
    }

    #[test]
    fn test_get_strategy_split() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env, None, None, None);

        /*
           Test - 1. There is a strategy override and the strategy is not active
        */
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo::default("sid1".to_string()),
            )
            .unwrap();

        let split = get_strategy_split(
            deps.as_mut().storage,
            &config,
            Some(1),
            &UserRewardInfo::default(),
            Uint128::new(100_u128),
        )
        .unwrap();

        assert_eq!(split, hashmap![0 => Uint128::new(100_u128)]);

        /*
            Test - 2. There is a strategy override and the strategy is active
        */
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Default::default(),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Default::default(),
                },
            )
            .unwrap();

        let split = get_strategy_split(
            deps.as_mut().storage,
            &config,
            Some(1),
            &UserRewardInfo::default(),
            Uint128::new(100_u128),
        )
        .unwrap();

        assert_eq!(split, hashmap![1 => Uint128::new(100_u128)]);

        /*
            Test - 3. There is no strategy override and all strategies are active
        */
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Default::default(),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Default::default(),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Default::default(),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Default::default(),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Default::default(),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Default::default(),
                },
            )
            .unwrap();

        let mut user_reward_info = UserRewardInfo::default();
        user_reward_info.user_portfolio = vec![
            UserStrategyPortfolio {
                strategy_id: 1,
                deposit_fraction: Uint128::new(25_u128),
            },
            UserStrategyPortfolio {
                strategy_id: 2,
                deposit_fraction: Uint128::new(25_u128),
            },
            UserStrategyPortfolio {
                strategy_id: 3,
                deposit_fraction: Uint128::new(25_u128),
            },
        ];

        let split = get_strategy_split(
            deps.as_mut().storage,
            &config,
            None,
            &user_reward_info,
            Uint128::new(100_u128),
        )
        .unwrap();

        assert_eq!(
            split,
            hashmap![0 => Uint128::new(25_u128), 1 => Uint128::new(25_u128), 2 => Uint128::new(25_u128), 3 => Uint128::new(25_u128)]
        );

        /*
            Test - 4. There is no strategy override and 2/3 strategies are inactive
        */
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: true,
                    total_shares: Default::default(),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Default::default(),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(2),
                &StrategyInfo {
                    name: "sid2".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: false,
                    total_shares: Default::default(),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Default::default(),
                },
            )
            .unwrap();
        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(3),
                &StrategyInfo {
                    name: "sid3".to_string(),
                    sic_contract_address: Addr::unchecked("abc"),
                    unbonding_period: 0,
                    unbonding_buffer: 0,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: false,
                    total_shares: Default::default(),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Default::default(),
                },
            )
            .unwrap();

        let mut user_reward_info = UserRewardInfo::default();
        user_reward_info.user_portfolio = vec![
            UserStrategyPortfolio {
                strategy_id: 1,
                deposit_fraction: Uint128::new(25_u128),
            },
            UserStrategyPortfolio {
                strategy_id: 2,
                deposit_fraction: Uint128::new(25_u128),
            },
            UserStrategyPortfolio {
                strategy_id: 3,
                deposit_fraction: Uint128::new(25_u128),
            },
        ];

        let split = get_strategy_split(
            deps.as_mut().storage,
            &config,
            None,
            &user_reward_info,
            Uint128::new(100_u128),
        )
        .unwrap();

        assert_eq!(
            split,
            hashmap![0 => Uint128::new(75_u128), 1 => Uint128::new(25_u128)]
        );
    }

    #[test]
    fn test_get_strategy_shares_per_token_ratio() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env, None, None, None);

        let sic1_address = Addr::unchecked("sic1_address");

        /*
           Test - 1. S_T ratio is less than 10
        */
        let mut contracts_to_tokens: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_tokens.insert(sic1_address.clone(), Uint128::new(500_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_tokens), None);

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: false,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();

        let strategy_info = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap()
            .unwrap();
        let s_t_ratio =
            get_strategy_shares_per_token_ratio(deps.as_ref().querier, &strategy_info).unwrap();

        assert_eq!(s_t_ratio, Decimal::from_ratio(2_u128, 1_u128));

        /*
           Test - 2. S_T ratio is greater than 10
        */
        let mut contracts_to_tokens: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_tokens.insert(sic1_address.clone(), Uint128::new(50_u128));
        deps.querier
            .update_stader_balances(Some(contracts_to_tokens), None);

        STRATEGY_MAP
            .save(
                deps.as_mut().storage,
                U64Key::new(1),
                &StrategyInfo {
                    name: "sid1".to_string(),
                    sic_contract_address: sic1_address.clone(),
                    unbonding_period: 3600,
                    unbonding_buffer: 3600,
                    next_undelegation_batch_id: 0,
                    next_reconciliation_batch_id: 0,
                    is_active: false,
                    total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                    current_undelegated_shares: Default::default(),
                    global_airdrop_pointer: vec![],
                    total_airdrops_accumulated: vec![],
                    shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
                },
            )
            .unwrap();

        let strategy_info = STRATEGY_MAP
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap()
            .unwrap();
        let s_t_ratio =
            get_strategy_shares_per_token_ratio(deps.as_ref().querier, &strategy_info).unwrap();

        assert_eq!(s_t_ratio, Decimal::from_ratio(20_u128, 1_u128));
    }

    #[test]
    fn test_get_strategy_apr() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env, None, None, None);

        let _deleg1 = Addr::unchecked("deleg0001".to_string());
        let initial_shares_per_token_ratio = Decimal::from_ratio(1_000_000_00_u128, 1_u128);

        /*
           Test - 1. Vault apr when shares_per_token_ratio is still 1
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_strategy_apr(
            Decimal::from_ratio(1_000_000_00_u128, 1_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state.contract_genesis_timestamp.plus_seconds(500),
        );
        assert_eq!(apr, Decimal::zero());

        /*
           Test - 2. Vault apr when shares_per_token_ratio becomes 0 ( will never happen )
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_strategy_apr(
            Decimal::zero(),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state.contract_genesis_timestamp.plus_seconds(500),
        );
        assert_eq!(apr, Decimal::from_ratio(6307200_u128, 1_u128));

        /*
            Test - 3. Vault apr when shares_per_token_ratio becomes 0.9 after 1000s
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_strategy_apr(
            Decimal::from_ratio(9_000_000_00_u128, 10_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state.contract_genesis_timestamp.plus_seconds(1000),
        );
        assert_eq!(apr, Decimal::from_ratio(3153600_u128, 10_u128));

        /*
            Test - 4. Vault apr when shares_per_token_ratio becomes 0.8 after 5000s
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_strategy_apr(
            Decimal::from_ratio(8_000_000_00_u128, 10_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state.contract_genesis_timestamp.plus_seconds(5000),
        );
        assert_eq!(apr, Decimal::from_ratio(126144_u128, 1_u128));

        /*
           Test - 5. Vault apr when shares_per_token_ratio becomes 0.009 after 10000s.
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_strategy_apr(
            Decimal::from_ratio(9_000_000_00_u128, 1000_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state.contract_genesis_timestamp.plus_seconds(10000),
        );
        assert_eq!(apr, Decimal::from_ratio(31252176_u128, 100_u128));

        /*
           Test - 6. Vault apr when shares_per_token_ratio becomes 0.94 after 1 year
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_strategy_apr(
            Decimal::from_ratio(94_000_000_00_u128, 100_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state
                .contract_genesis_timestamp
                .plus_seconds(365 * 24 * 3600),
        );
        assert_eq!(apr, Decimal::from_ratio(6_u128, 1_u128));

        /*
            Test - 7. Vault apr when shares_per_token_ratio becomes 0.90 after 2 year
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_strategy_apr(
            Decimal::from_ratio(9_000_000_000_u128, 100_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state
                .contract_genesis_timestamp
                .plus_seconds(365 * 24 * 3600 * 2),
        );
        assert_eq!(apr, Decimal::from_ratio(5_u128, 1_u128));

        /*
            Test - 8. Vault apr when shares_per_token_ratio becomes 0.90 after half a year
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_strategy_apr(
            Decimal::from_ratio(9_000_000_000_u128, 100_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state
                .contract_genesis_timestamp
                .plus_seconds(365 * 12 * 3600),
        );
        assert_eq!(apr, Decimal::from_ratio(20_u128, 1_u128));
    }
}
