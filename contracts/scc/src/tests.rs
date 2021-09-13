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
        BatchUndelegationRecord, Cw20TokenContractsInfo, State, StrategyInfo, UserRewardInfo,
        UserStrategyInfo, UserStrategyPortfolio, UserUndelegationRecord,
        CW20_TOKEN_CONTRACTS_REGISTRY, STATE, STRATEGY_MAP, UNDELEGATION_BATCH_MAP,
        USER_REWARD_INFO_MAP,
    };
    use crate::test_helpers::{check_equal_reward_info, check_equal_user_strategies};
    use crate::ContractError;
    use cosmwasm_std::{
        coins, from_binary, to_binary, Addr, Attribute, BankMsg, Binary, Coin, Decimal, DepsMut,
        Empty, Env, MessageInfo, OwnedDeps, QuerierWrapper, Response, StdResult, SubMsg, Timestamp,
        Uint128, WasmMsg,
    };
    use cw_storage_plus::U64Key;
    use sic_base::msg::{ExecuteMsg as sic_execute_msg, QueryMsg as sic_query_msg};
    use stader_utils::coin_utils::DecCoin;
    use stader_utils::mock::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use stader_utils::test_helpers::check_equal_vec;
    use std::borrow::Borrow;
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

    fn get_pools_contract_address() -> Addr {
        Addr::unchecked("pools_contract")
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
                total_accumulated_airdrops: vec![],
                current_undelegated_strategies: vec![]
            }
        );
    }

    #[test]
    fn test__try_update_cw20_contracts_registry_fail() {
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
            ExecuteMsg::RegisterCw20Contracts {
                denom: "anc".to_string(),
                cw20_contract: Addr::unchecked("abc"),
                airdrop_contract: Addr::unchecked("def"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test__try_update_cw20_contracts_registry_success() {
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

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RegisterCw20Contracts {
                denom: "anc".to_string(),
                cw20_contract: Addr::unchecked("abc"),
                airdrop_contract: Addr::unchecked("def"),
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
    fn test__try_fetch_undelegated_rewards_from_strategies_fail() {
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
                strategies: vec!["sid1".to_string(), "sid2".to_string()],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 4);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "sid1,sid2".to_string()
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
                    key: "undelegation_batches_slashing_checked".to_string(),
                    value: "".to_string()
                }
            ]
        ));

        /*
           Test - 4. Undelegation batches not found
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 2,
                reconciled_batch_id_pointer: 1,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
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
                sic_contract_address: Addr::unchecked("def"),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 3,
                reconciled_batch_id_pointer: 2,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec!["sid1".to_string(), "sid2".to_string()],
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
                    value: "sid1:1,sid2:2".to_string()
                },
                Attribute {
                    key: "undelegation_batches_in_unbonding_period".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "undelegation_batches_slashing_checked".to_string(),
                    value: "".to_string()
                }
            ]
        ));
        let sid1_strategy_info = STRATEGY_MAP.load(deps.as_mut().storage, "sid1").unwrap();
        let sid2_strategy_info = STRATEGY_MAP.load(deps.as_mut().storage, "sid2").unwrap();
        assert_eq!(sid1_strategy_info.reconciled_batch_id_pointer, 2);
        assert_eq!(sid1_strategy_info.undelegation_batch_id_pointer, 2);
        assert_eq!(sid2_strategy_info.reconciled_batch_id_pointer, 3);
        assert_eq!(sid2_strategy_info.undelegation_batch_id_pointer, 3);

        /*
            Test - 5. Undelegation batches in unbonding period
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 2,
                reconciled_batch_id_pointer: 1,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
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
                sic_contract_address: Addr::unchecked("def"),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 3,
                reconciled_batch_id_pointer: 2,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(1), "sid1"),
            &BatchUndelegationRecord {
                amount: Uint128::new(100_u128),
                shares: Decimal::from_ratio(1000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Timestamp::from_seconds(1631094920),
                est_release_time: Timestamp::from_seconds(1631094990),
                slashing_checked: false,
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(2), "sid2"),
            &BatchUndelegationRecord {
                amount: Uint128::new(400_u128),
                shares: Decimal::from_ratio(1000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Timestamp::from_seconds(1631094920),
                est_release_time: Timestamp::from_seconds(1631095990),
                slashing_checked: false,
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec!["sid1".to_string(), "sid2".to_string()],
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
                    value: "sid1:1,sid2:2".to_string()
                },
                Attribute {
                    key: "undelegation_batches_slashing_checked".to_string(),
                    value: "".to_string()
                }
            ]
        ));
        let sid1_strategy_info = STRATEGY_MAP.load(deps.as_mut().storage, "sid1").unwrap();
        let sid2_strategy_info = STRATEGY_MAP.load(deps.as_mut().storage, "sid2").unwrap();
        assert_eq!(sid1_strategy_info.reconciled_batch_id_pointer, 1);
        assert_eq!(sid1_strategy_info.undelegation_batch_id_pointer, 2);
        assert_eq!(sid2_strategy_info.reconciled_batch_id_pointer, 2);
        assert_eq!(sid2_strategy_info.undelegation_batch_id_pointer, 3);

        /*
            Test - 6. Undelegation batches have already been accounted for slashing
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 2,
                reconciled_batch_id_pointer: 1,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
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
                sic_contract_address: Addr::unchecked("def"),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 3,
                reconciled_batch_id_pointer: 2,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(1), "sid1"),
            &BatchUndelegationRecord {
                amount: Uint128::new(100_u128),
                shares: Decimal::from_ratio(1000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Timestamp::from_seconds(123),
                est_release_time: Timestamp::from_seconds(125),
                slashing_checked: true,
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(2), "sid2"),
            &BatchUndelegationRecord {
                amount: Uint128::new(400_u128),
                shares: Decimal::from_ratio(4000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Timestamp::from_seconds(123),
                est_release_time: Timestamp::from_seconds(125),
                slashing_checked: true,
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec!["sid1".to_string(), "sid2".to_string()],
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
                    key: "undelegation_batches_slashing_checked".to_string(),
                    value: "sid1:1,sid2:2".to_string()
                }
            ]
        ));
        let sid1_strategy_info = STRATEGY_MAP.load(deps.as_mut().storage, "sid1").unwrap();
        let sid2_strategy_info = STRATEGY_MAP.load(deps.as_mut().storage, "sid2").unwrap();
        assert_eq!(sid1_strategy_info.reconciled_batch_id_pointer, 2);
        assert_eq!(sid1_strategy_info.undelegation_batch_id_pointer, 2);
        assert_eq!(sid2_strategy_info.reconciled_batch_id_pointer, 3);
        assert_eq!(sid2_strategy_info.undelegation_batch_id_pointer, 3);
    }

    #[test]
    fn test__try_fetch_undelegated_rewards_from_strategies_success() {
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

        /*
           Test - 1. Strategies have no undelegation slashing
        */
        let mut contracts_to_fulfillable_undelegation: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_fulfillable_undelegation.insert(sic1_address.clone(), Uint128::new(100_u128));
        contracts_to_fulfillable_undelegation.insert(sic2_address.clone(), Uint128::new(400_u128));
        deps.querier
            .update_wasm(None, Some(contracts_to_fulfillable_undelegation));

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 2,
                reconciled_batch_id_pointer: 1,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
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
                sic_contract_address: sic2_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 3,
                reconciled_batch_id_pointer: 2,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(1), "sid1"),
            &BatchUndelegationRecord {
                amount: Uint128::new(100_u128),
                shares: Default::default(),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Default::default(),
                create_time: Timestamp::from_seconds(123),
                est_release_time: Timestamp::from_seconds(125),
                slashing_checked: false,
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(2), "sid2"),
            &BatchUndelegationRecord {
                amount: Uint128::new(400_u128),
                shares: Default::default(),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Default::default(),
                create_time: Timestamp::from_seconds(123),
                est_release_time: Timestamp::from_seconds(125),
                slashing_checked: false,
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec!["sid1".to_string(), "sid2".to_string()],
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
                    key: "undelegation_batches_slashing_checked".to_string(),
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
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        assert_ne!(sid2_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.undelegation_batch_id_pointer, 2);
        assert_eq!(sid1_strategy_info.reconciled_batch_id_pointer, 2);
        assert_eq!(sid2_strategy_info.undelegation_batch_id_pointer, 3);
        assert_eq!(sid2_strategy_info.reconciled_batch_id_pointer, 3);
        let sid1_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(1), "sid1"))
            .unwrap();
        let sid2_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(2), "sid2"))
            .unwrap();
        assert_ne!(sid1_undelegation_batch_opt, None);
        assert_ne!(sid2_undelegation_batch_opt, None);
        let sid1_undelegation_batch = sid1_undelegation_batch_opt.unwrap();
        let sid2_undelegation_batch = sid2_undelegation_batch_opt.unwrap();
        assert!(sid1_undelegation_batch.slashing_checked);
        assert!(sid2_undelegation_batch.slashing_checked);
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
            .update_wasm(None, Some(contracts_to_fulfillable_undelegation));

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 2,
                reconciled_batch_id_pointer: 1,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
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
                sic_contract_address: sic2_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 3,
                reconciled_batch_id_pointer: 2,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(1), "sid1"),
            &BatchUndelegationRecord {
                amount: Uint128::new(100_u128),
                shares: Default::default(),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Default::default(),
                create_time: Timestamp::from_seconds(123),
                est_release_time: Timestamp::from_seconds(125),
                slashing_checked: false,
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(2), "sid2"),
            &BatchUndelegationRecord {
                amount: Uint128::new(400_u128),
                shares: Default::default(),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Default::default(),
                create_time: Timestamp::from_seconds(123),
                est_release_time: Timestamp::from_seconds(125),
                slashing_checked: false,
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec!["sid1".to_string(), "sid2".to_string()],
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
                    key: "undelegation_batches_slashing_checked".to_string(),
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
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        assert_ne!(sid2_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.undelegation_batch_id_pointer, 2);
        assert_eq!(sid1_strategy_info.reconciled_batch_id_pointer, 2);
        assert_eq!(sid2_strategy_info.undelegation_batch_id_pointer, 3);
        assert_eq!(sid2_strategy_info.reconciled_batch_id_pointer, 3);
        let sid1_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(1), "sid1"))
            .unwrap();
        let sid2_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(2), "sid2"))
            .unwrap();
        assert_ne!(sid1_undelegation_batch_opt, None);
        assert_ne!(sid2_undelegation_batch_opt, None);
        let sid1_undelegation_batch = sid1_undelegation_batch_opt.unwrap();
        let sid2_undelegation_batch = sid2_undelegation_batch_opt.unwrap();
        assert!(sid1_undelegation_batch.slashing_checked);
        assert!(sid2_undelegation_batch.slashing_checked);
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
            .update_wasm(None, Some(contracts_to_fulfillable_undelegation));

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 2,
                reconciled_batch_id_pointer: 1,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
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
                sic_contract_address: sic2_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 3,
                reconciled_batch_id_pointer: 2,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(1), "sid1"),
            &BatchUndelegationRecord {
                amount: Uint128::new(100_u128),
                shares: Default::default(),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Default::default(),
                create_time: Timestamp::from_seconds(123),
                est_release_time: Timestamp::from_seconds(125),
                slashing_checked: false,
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(2), "sid2"),
            &BatchUndelegationRecord {
                amount: Uint128::new(400_u128),
                shares: Default::default(),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Default::default(),
                create_time: Timestamp::from_seconds(123),
                est_release_time: Timestamp::from_seconds(125),
                slashing_checked: false,
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::FetchUndelegatedRewardsFromStrategies {
                strategies: vec!["sid1".to_string(), "sid2".to_string()],
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
                    key: "undelegation_batches_slashing_checked".to_string(),
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
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        assert_ne!(sid2_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.undelegation_batch_id_pointer, 2);
        assert_eq!(sid1_strategy_info.reconciled_batch_id_pointer, 2);
        assert_eq!(sid2_strategy_info.undelegation_batch_id_pointer, 3);
        assert_eq!(sid2_strategy_info.reconciled_batch_id_pointer, 3);
        let sid1_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(1), "sid1"))
            .unwrap();
        let sid2_undelegation_batch_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(2), "sid2"))
            .unwrap();
        assert_ne!(sid1_undelegation_batch_opt, None);
        assert_ne!(sid2_undelegation_batch_opt, None);
        let sid1_undelegation_batch = sid1_undelegation_batch_opt.unwrap();
        let sid2_undelegation_batch = sid2_undelegation_batch_opt.unwrap();
        assert!(sid1_undelegation_batch.slashing_checked);
        assert!(sid2_undelegation_batch.slashing_checked);
        assert_eq!(
            sid1_undelegation_batch.unbonding_slashing_ratio,
            Decimal::one()
        );
        assert_eq!(
            sid2_undelegation_batch.unbonding_slashing_ratio,
            Decimal::one()
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
            mock_info("non-creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {
                strategies: vec!["sid1".to_string(), "sid2".to_string()],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}))
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
        deps.querier.update_wasm(Some(contracts_to_token), None);

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 4,
                reconciled_batch_id_pointer: 1,
                is_active: false,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::from_ratio(1000_u128, 1_u128),
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
                sic_contract_address: sic2_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 4,
                reconciled_batch_id_pointer: 1,
                is_active: false,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::from_ratio(2000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid3",
            &StrategyInfo {
                name: "sid3".to_string(),
                sic_contract_address: sic3_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 4,
                reconciled_batch_id_pointer: 1,
                is_active: false,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::from_ratio(3000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        let undelegation_batch_sid1_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), "sid1"))
            .unwrap();
        let undelegation_batch_sid2_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), "sid2"))
            .unwrap();
        let undelegation_batch_sid3_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), "sid3"))
            .unwrap();
        assert_eq!(undelegation_batch_sid1_opt, None);
        assert_eq!(undelegation_batch_sid2_opt, None);
        assert_eq!(undelegation_batch_sid3_opt, None);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {
                strategies: vec!["sid1".to_string(), "sid2".to_string(), "sid3".to_string()],
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
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.undelegation_batch_id_pointer, 5);
        assert_eq!(
            sid1_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        assert_eq!(
            sid1_strategy_info.total_shares,
            Decimal::from_ratio(4000_u128, 1_u128)
        );
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap();
        assert_ne!(sid2_strategy_info_opt, None);
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(sid2_strategy_info.undelegation_batch_id_pointer, 5);
        assert_eq!(
            sid2_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        assert_eq!(
            sid2_strategy_info.total_shares,
            Decimal::from_ratio(3000_u128, 1_u128)
        );
        let sid3_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid3")
            .unwrap();
        assert_ne!(sid3_strategy_info_opt, None);
        let sid3_strategy_info = sid3_strategy_info_opt.unwrap();
        assert_eq!(
            sid3_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        assert_eq!(sid3_strategy_info.undelegation_batch_id_pointer, 5);
        assert_eq!(
            sid3_strategy_info.total_shares,
            Decimal::from_ratio(2000_u128, 1_u128)
        );

        let undelegation_batch_sid1_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), "sid1"))
            .unwrap();
        let undelegation_batch_sid2_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), "sid2"))
            .unwrap();
        let undelegation_batch_sid3_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), "sid3"))
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
                create_time: env.block.time,
                est_release_time: env.block.time.plus_seconds(
                    sid1_strategy_info.unbonding_period + sid1_strategy_info.unbonding_buffer
                ),
                slashing_checked: false
            }
        );
        assert_eq!(
            undelegation_batch_sid2,
            BatchUndelegationRecord {
                amount: Uint128::new(200_u128),
                shares: Decimal::from_ratio(2000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: env.block.time,
                est_release_time: env.block.time.plus_seconds(
                    sid2_strategy_info.unbonding_period + sid2_strategy_info.unbonding_buffer
                ),
                slashing_checked: false
            }
        );
        assert_eq!(
            undelegation_batch_sid3,
            BatchUndelegationRecord {
                amount: Uint128::new(300_u128),
                shares: Decimal::from_ratio(3000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: env.block.time,
                est_release_time: env.block.time.plus_seconds(
                    sid3_strategy_info.unbonding_period + sid3_strategy_info.unbonding_buffer
                ),
                slashing_checked: false
            }
        );

        /*
           Test - 3. Failed strategies
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 4,
                reconciled_batch_id_pointer: 1,
                is_active: false,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::from_ratio(1000_u128, 1_u128),
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
                sic_contract_address: sic2_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 4,
                reconciled_batch_id_pointer: 1,
                is_active: false,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::from_ratio(2000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid3",
            &StrategyInfo {
                name: "sid3".to_string(),
                sic_contract_address: sic3_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 4,
                reconciled_batch_id_pointer: 1,
                is_active: false,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::from_ratio(3000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {
                strategies: vec![
                    "sid1".to_string(),
                    "sid2".to_string(),
                    "sid3".to_string(),
                    "sid4".to_string(),
                ],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 2);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "sid4".to_string()
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
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.undelegation_batch_id_pointer, 5);
        assert_eq!(
            sid1_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap();
        assert_ne!(sid2_strategy_info_opt, None);
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(sid2_strategy_info.undelegation_batch_id_pointer, 5);
        assert_eq!(
            sid2_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        let sid3_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid3")
            .unwrap();
        assert_ne!(sid3_strategy_info_opt, None);
        let sid3_strategy_info = sid3_strategy_info_opt.unwrap();
        assert_eq!(
            sid3_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        assert_eq!(sid3_strategy_info.undelegation_batch_id_pointer, 5);
        let undelegation_batch_sid1_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), "sid1"))
            .unwrap();
        let undelegation_batch_sid2_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), "sid2"))
            .unwrap();
        let undelegation_batch_sid3_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), "sid3"))
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
                create_time: env.block.time,
                est_release_time: env.block.time.plus_seconds(
                    sid1_strategy_info.unbonding_period + sid1_strategy_info.unbonding_buffer
                ),
                slashing_checked: false
            }
        );
        assert_eq!(
            undelegation_batch_sid2,
            BatchUndelegationRecord {
                amount: Uint128::new(200_u128),
                shares: Decimal::from_ratio(2000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: env.block.time,
                est_release_time: env.block.time.plus_seconds(
                    sid2_strategy_info.unbonding_period + sid2_strategy_info.unbonding_buffer
                ),
                slashing_checked: false
            }
        );
        assert_eq!(
            undelegation_batch_sid3,
            BatchUndelegationRecord {
                amount: Uint128::new(300_u128),
                shares: Decimal::from_ratio(3000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: env.block.time,
                est_release_time: env.block.time.plus_seconds(
                    sid3_strategy_info.unbonding_period + sid3_strategy_info.unbonding_buffer
                ),
                slashing_checked: false
            }
        );

        /*
            Test - 4. Strategies with no undelegations
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 4,
                reconciled_batch_id_pointer: 1,
                is_active: false,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::from_ratio(1000_u128, 1_u128),
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
                sic_contract_address: sic2_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 4,
                reconciled_batch_id_pointer: 1,
                is_active: false,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::from_ratio(2000_u128, 1_u128),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid3",
            &StrategyInfo {
                name: "sid3".to_string(),
                sic_contract_address: sic3_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 4,
                reconciled_batch_id_pointer: 1,
                is_active: false,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Decimal::zero(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UndelegateFromStrategies {
                strategies: vec!["sid1".to_string(), "sid2".to_string(), "sid3".to_string()],
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
                    value: "sid3".to_string()
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
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap();
        assert_ne!(sid1_strategy_info_opt, None);
        let sid1_strategy_info = sid1_strategy_info_opt.unwrap();
        assert_eq!(sid1_strategy_info.undelegation_batch_id_pointer, 5);
        assert_eq!(
            sid1_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        let sid2_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid2")
            .unwrap();
        assert_ne!(sid2_strategy_info_opt, None);
        let sid2_strategy_info = sid2_strategy_info_opt.unwrap();
        assert_eq!(sid2_strategy_info.undelegation_batch_id_pointer, 5);
        assert_eq!(
            sid2_strategy_info.current_undelegated_shares,
            Decimal::zero()
        );
        let undelegation_batch_sid1_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), "sid1"))
            .unwrap();
        let undelegation_batch_sid2_opt = UNDELEGATION_BATCH_MAP
            .may_load(deps.as_mut().storage, (U64Key::new(4), "sid2"))
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
                create_time: env.block.time,
                est_release_time: env.block.time.plus_seconds(
                    sid1_strategy_info.unbonding_period + sid1_strategy_info.unbonding_buffer
                ),
                slashing_checked: false
            }
        );
        assert_eq!(
            undelegation_batch_sid2,
            BatchUndelegationRecord {
                amount: Uint128::new(200_u128),
                shares: Decimal::from_ratio(2000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::one(),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: env.block.time,
                est_release_time: env.block.time.plus_seconds(
                    sid2_strategy_info.unbonding_period + sid2_strategy_info.unbonding_buffer
                ),
                slashing_checked: false
            }
        );
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

        /*
            Test - 3. Undelegation batch not found
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::default("sid1".to_string()),
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
                undelegation_records: vec![UserUndelegationRecord {
                    id: Timestamp::from_seconds(123),
                    amount: Uint128::new(100_u128),
                    shares: Default::default(),
                    strategy_name: "sid1".to_string(),
                    undelegation_batch_id: 3,
                }],
                pending_rewards: Uint128::zero(),
            },
        );
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: "123000000000".to_string(),
                strategy_name: "sid1".to_string(),
                amount: Uint128::new(100_u128),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UndelegationBatchNotFound {}));

        /*
           Test - 4. Undelegation batch in unbonding period
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::default("sid1".to_string()),
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
                undelegation_records: vec![UserUndelegationRecord {
                    id: Timestamp::from_seconds(123),
                    amount: Uint128::new(100_u128),
                    shares: Default::default(),
                    strategy_name: "sid1".to_string(),
                    undelegation_batch_id: 3,
                }],
                pending_rewards: Uint128::zero(),
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(3), "sid1"),
            &BatchUndelegationRecord {
                amount: Uint128::new(400_u128),
                shares: Default::default(),
                unbonding_slashing_ratio: Decimal::from_ratio(3_u128, 4_u128),
                undelegation_s_t_ratio: Default::default(),
                create_time: Timestamp::from_seconds(150),
                est_release_time: Timestamp::from_seconds(1831013565),
                slashing_checked: false,
            },
        );
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: "123000000000".to_string(),
                strategy_name: "sid1".to_string(),
                amount: Uint128::new(100_u128),
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
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::default("sid1".to_string()),
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
                undelegation_records: vec![UserUndelegationRecord {
                    id: Timestamp::from_seconds(123),
                    amount: Uint128::new(100_u128),
                    shares: Default::default(),
                    strategy_name: "sid1".to_string(),
                    undelegation_batch_id: 3,
                }],
                pending_rewards: Uint128::zero(),
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(3), "sid1"),
            &BatchUndelegationRecord {
                amount: Uint128::new(400_u128),
                shares: Default::default(),
                unbonding_slashing_ratio: Decimal::from_ratio(3_u128, 4_u128),
                undelegation_s_t_ratio: Default::default(),
                create_time: Timestamp::from_seconds(150),
                est_release_time: Timestamp::from_seconds(150 + 7200),
                slashing_checked: false,
            },
        );
        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: "123000000000".to_string(),
                strategy_name: "sid1".to_string(),
                amount: Uint128::new(100_u128),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::SlashingNotChecked {}));
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

        /*
           Test - 1. User has only 1 undelegation record
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::new("sid1".to_string(), sic1_address.clone(), None, None),
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(5000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
                undelegation_records: vec![UserUndelegationRecord {
                    id: Timestamp::from_seconds(123),
                    amount: Uint128::new(100_u128),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    strategy_name: "sid1".to_string(),
                    undelegation_batch_id: 5,
                }],
                pending_rewards: Uint128::zero(),
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(5), "sid1"),
            &BatchUndelegationRecord {
                amount: Uint128::new(400_u128),
                shares: Decimal::from_ratio(4000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::from_ratio(3_u128, 4_u128),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Timestamp::from_seconds(150),
                est_release_time: Timestamp::from_seconds(150 + 7200),
                slashing_checked: true,
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: "123000000000".to_string(),
                strategy_name: "sid1".to_string(),
                amount: Uint128::new(100_u128),
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
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.undelegation_records.len(), 0);

        /*
           Test - 2. User has multiple undelegation records.
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::new("sid1".to_string(), sic1_address.clone(), None, None),
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![
                    UserStrategyInfo {
                        strategy_name: "sid1".to_string(),
                        shares: Decimal::from_ratio(5000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                    UserStrategyInfo {
                        strategy_name: "sid2".to_string(),
                        shares: Decimal::from_ratio(5000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                ],
                pending_airdrops: vec![],
                undelegation_records: vec![
                    UserUndelegationRecord {
                        id: Timestamp::from_seconds(123),
                        amount: Uint128::new(100_u128),
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        strategy_name: "sid1".to_string(),
                        undelegation_batch_id: 5,
                    },
                    UserUndelegationRecord {
                        id: Timestamp::from_seconds(126),
                        amount: Uint128::new(100_u128),
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        strategy_name: "sid2".to_string(),
                        undelegation_batch_id: 6,
                    },
                ],
                pending_rewards: Uint128::zero(),
            },
        );
        UNDELEGATION_BATCH_MAP.save(
            deps.as_mut().storage,
            (U64Key::new(5), "sid1"),
            &BatchUndelegationRecord {
                amount: Uint128::new(400_u128),
                shares: Decimal::from_ratio(4000_u128, 1_u128),
                unbonding_slashing_ratio: Decimal::from_ratio(3_u128, 4_u128),
                undelegation_s_t_ratio: Decimal::from_ratio(10_u128, 1_u128),
                create_time: Timestamp::from_seconds(150),
                est_release_time: Timestamp::from_seconds(150 + 7200),
                slashing_checked: true,
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::WithdrawRewards {
                undelegation_id: "123000000000".to_string(),
                strategy_name: "sid1".to_string(),
                amount: Uint128::new(100_u128),
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
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.undelegation_records.len(), 1);
        assert!(check_equal_vec(
            user1_reward_info.undelegation_records,
            vec![UserUndelegationRecord {
                id: Timestamp::from_seconds(126),
                amount: Uint128::new(100_u128),
                shares: Decimal::from_ratio(1000_u128, 1_u128),
                strategy_name: "sid2".to_string(),
                undelegation_batch_id: 6,
            }]
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
        let sic1_address = Addr::unchecked("sic1_address");

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
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(Some(contracts_to_token), None);

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(5000_u128, 1_u128),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(3000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Uint128::zero(),
            },
        );

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
        assert!(matches!(
            err,
            ContractError::UserDoesNotHaveEnoughRewards {}
        ));
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
        let sic1_address = Addr::unchecked("sic1_address");
        let sic2_address = Addr::unchecked("sic2_address");

        /*
           Test - 1. User undelegates for the first time from a strategy
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(Some(contracts_to_token), None);

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(3000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Uint128::zero(),
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(200_u128),
                strategy_name: "sid1".to_string(),
            },
        )
        .unwrap();
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
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
                strategy_name: "sid1".to_string(),
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
                id: env.block.time,
                amount: Uint128::new(200_u128),
                shares: Decimal::from_ratio(2000_u128, 1_u128),
                strategy_name: "sid1".to_string(),
                undelegation_batch_id: 0
            }]
        ));

        /*
           Test - 2. User undelegates again in the same blocktime
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(1000_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 5000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 5000_u128), "mir".to_string()),
                    ],
                }],
                pending_airdrops: vec![
                    Coin::new(120_u128, "anc".to_string()),
                    Coin::new(240_u128, "mir".to_string()),
                ],
                undelegation_records: vec![UserUndelegationRecord {
                    id: env.block.time,
                    amount: Uint128::new(200_u128),
                    shares: Decimal::from_ratio(2000_u128, 1_u128),
                    strategy_name: "sid1".to_string(),
                    undelegation_batch_id: 0,
                }],
                pending_rewards: Uint128::zero(),
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(50_u128),
                strategy_name: "sid1".to_string(),
            },
        )
        .unwrap();
        let sid1_strategy_info_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
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
                strategy_name: "sid1".to_string(),
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
                    id: env.block.time,
                    amount: Uint128::new(200_u128),
                    shares: Decimal::from_ratio(2000_u128, 1_u128),
                    strategy_name: "sid1".to_string(),
                    undelegation_batch_id: 0
                },
                UserUndelegationRecord {
                    id: env.block.time,
                    amount: Uint128::new(50_u128),
                    shares: Decimal::from_ratio(500_u128, 1_u128),
                    strategy_name: "sid1".to_string(),
                    undelegation_batch_id: 0
                },
            ]
        ));
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
                strategy_name: "sid".to_string(),
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
                strategy_name: "sid".to_string(),
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
                strategy_name: "sid".to_string(),
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
                strategy_name: "sid".to_string(),
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
                strategy_name: "sid".to_string(),
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
                sic_contract_address: sic1_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(5000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Default::default(),
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
                unbonding_period: 3600,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![UserStrategyInfo {
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(5000_u128, 1_u128),
                    airdrop_pointer: vec![],
                }],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Default::default(),
            },
        );
        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.total_accumulated_airdrops = vec![
                Coin::new(500_u128, "anc".to_string()),
                Coin::new(700_u128, "mir".to_string()),
                Coin::new(400_u128, "pyl".to_string()),
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
                Coin::new(500_u128, "mir".to_string()),
                Coin::new(400_u128, "pyl".to_string())
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
    fn test__try_withdraw_pending_rewards_fail() {
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
           Test - 1. User reward info not present
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("user1", &[]),
            ExecuteMsg::WithdrawPendingRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UserRewardInfoDoesNotExist {}))
    }

    #[test]
    fn test__try_withdraw_pending_rewards_success() {
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

        /*
           Test - 1. User reward info with non-zero pending rewards
        */
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Uint128::new(1000_u128),
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("user1", &[]),
            ExecuteMsg::WithdrawPendingRewards {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: "user1".to_string(),
                amount: vec![Coin::new(1000_u128, "uluna".to_string())]
            })]
        ));
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.pending_rewards, Uint128::zero());

        /*
            Test - 2. User reward info with zero pending rewards
        */
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Uint128::new(0_u128),
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("user1", &[]),
            ExecuteMsg::WithdrawPendingRewards {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "zero_pending_rewards".to_string(),
                value: "1".to_string()
            }]
        ));
        let user1_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user1_reward_info_opt, None);
        let user1_reward_info = user1_reward_info_opt.unwrap();
        assert_eq!(user1_reward_info.pending_rewards, Uint128::zero());
    }

    #[test]
    fn test__try_update_user_portfolio_fail() {
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
        /*
            Test - 1. Strategy does not exist
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::UpdateUserPortfolio {
                strategy_name: "sid1".to_string(),
                deposit_fraction: Decimal::one(),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::StrategyInfoDoesNotExist(String { .. })
        ));

        /*
           Test - 2. Adding an invalid portfolio which causes the entire deposit fraction to go beyond 1
        */
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![
                    UserStrategyPortfolio {
                        strategy_name: "sid1".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 2_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_name: "sid2".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                    },
                ],
                strategies: vec![],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Default::default(),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid3",
            &StrategyInfo::default("sid3".parse().unwrap()),
        );
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("user1", &[]),
            ExecuteMsg::UpdateUserPortfolio {
                strategy_name: "sid3".to_string(),
                deposit_fraction: Decimal::from_ratio(3_u128, 4_u128),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::InvalidPortfolioDepositFraction {}
        ));

        /*
            Test - 3. Updating an existing portfolio which causes the entire deposit fraction to go beyond 1
        */
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![
                    UserStrategyPortfolio {
                        strategy_name: "sid1".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 2_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_name: "sid2".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                    },
                ],
                strategies: vec![],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Default::default(),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo::default("sid2".parse().unwrap()),
        );
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("user1", &[]),
            ExecuteMsg::UpdateUserPortfolio {
                strategy_name: "sid2".to_string(),
                deposit_fraction: Decimal::from_ratio(3_u128, 4_u128),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::InvalidPortfolioDepositFraction {}
        ));
    }

    #[test]
    fn test__try_update_user_portfolio_success() {
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

        /*
           Test - 1. User doesn't have the portfolio and is new
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::default("sid1".to_string()),
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UpdateUserPortfolio {
                strategy_name: "sid1".to_string(),
                deposit_fraction: Decimal::from_ratio(3_u128, 4_u128),
            },
        )
        .unwrap();

        let user_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user_reward_info_opt, None);
        let user_reward_info = user_reward_info_opt.unwrap();
        assert_eq!(user_reward_info.user_portfolio.len(), 1);
        assert!(check_equal_vec(
            user_reward_info.user_portfolio,
            vec![UserStrategyPortfolio {
                strategy_name: "sid1".to_string(),
                deposit_fraction: Decimal::from_ratio(3_u128, 4_u128)
            }]
        ));

        /*
           Test - 2. User updates the deposit fraction of an existing portfolio
        */

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo::default("sid1".to_string()),
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![UserStrategyPortfolio {
                    strategy_name: "sid1".to_string(),
                    deposit_fraction: Decimal::from_ratio(1_u128, 2_u128),
                }],
                strategies: vec![],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Default::default(),
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(user1.as_str(), &[]),
            ExecuteMsg::UpdateUserPortfolio {
                strategy_name: "sid1".to_string(),
                deposit_fraction: Decimal::from_ratio(3_u128, 4_u128),
            },
        )
        .unwrap();
        let user_reward_info_opt = USER_REWARD_INFO_MAP
            .may_load(deps.as_mut().storage, &user1)
            .unwrap();
        assert_ne!(user_reward_info_opt, None);
        let user_reward_info = user_reward_info_opt.unwrap();
        assert_eq!(user_reward_info.user_portfolio.len(), 1);
        assert!(check_equal_vec(
            user_reward_info.user_portfolio,
            vec![UserStrategyPortfolio {
                strategy_name: "sid1".to_string(),
                deposit_fraction: Decimal::from_ratio(3_u128, 4_u128)
            }]
        ));
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

        let user1 = Addr::unchecked("user1");
        let user2 = Addr::unchecked("user2");
        let user3 = Addr::unchecked("user3");
        let sic1_address = Addr::unchecked("sic1_address");
        let sic2_address = Addr::unchecked("sic2_address");
        let sic3_address = Addr::unchecked("sic3_address");

        /*
           Test - 1. Empty user requests
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_pools_contract_address().as_ref(), &[]),
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
           Test - 2. User sends 0 funds
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_pools_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(0_u128),
                    strategy_name: None,
                }],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 3);
        assert_eq!(res.messages.len(), 0);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "inactive_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "users_with_zero_deposits".to_string(),
                    value: "user1".to_string()
                }
            ]
        ));

        /*
           Test - 3. Strategy not found
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_pools_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(100_u128),
                    strategy_name: Some("sid1".to_string()),
                }],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 3);
        assert_eq!(res.messages.len(), 0);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "sid1".to_string()
                },
                Attribute {
                    key: "inactive_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "users_with_zero_deposits".to_string(),
                    value: "".to_string()
                }
            ]
        ));

        /*
           Test - 4. Inactive strategy
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address,
                unbonding_period: 3600,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: false,
                total_shares: Default::default(),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Default::default(),
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_pools_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(100_u128),
                    strategy_name: Some("sid1".to_string()),
                }],
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 3);
        assert_eq!(res.messages.len(), 0);
        assert!(check_equal_vec(
            res.attributes,
            vec![
                Attribute {
                    key: "failed_strategies".to_string(),
                    value: "".to_string()
                },
                Attribute {
                    key: "inactive_strategies".to_string(),
                    value: "sid1".to_string()
                },
                Attribute {
                    key: "users_with_zero_deposits".to_string(),
                    value: "".to_string()
                }
            ]
        ));
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
        let sic1_address = Addr::unchecked("sic1_address");
        let sic2_address = Addr::unchecked("sic2_address");
        let sic3_address = Addr::unchecked("sic3_address");

        /*
           Test - 1. User deposits to a new strategy for the first time(no user_reward_info)
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(Some(contracts_to_token), None);

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_pools_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(100_u128),
                    strategy_name: Some("sid1".to_string()),
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
            .may_load(deps.as_mut().storage, "sid1")
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
                strategy_name: "sid1".to_string(),
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
        assert_eq!(state.total_accumulated_rewards, Uint128::new(100_u128));

        /*
           Test - 2. User deposits to an already deposited strategy on-demand
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic3_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(Some(contracts_to_token), None);
        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.total_accumulated_rewards = Uint128::new(200_u128);
                Ok(state)
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo {
                name: "sid2".to_string(),
                sic_contract_address: sic2_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(7000_u128, 1_u128),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid3",
            &StrategyInfo {
                name: "sid3".to_string(),
                sic_contract_address: sic3_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![
                    UserStrategyPortfolio {
                        strategy_name: "sid1".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 2_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_name: "sid2".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_name: "sid3".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                    },
                ],
                strategies: vec![
                    UserStrategyInfo {
                        strategy_name: "sid1".to_string(),
                        shares: Decimal::from_ratio(2000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                    UserStrategyInfo {
                        strategy_name: "sid2".to_string(),
                        shares: Decimal::from_ratio(3000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                    UserStrategyInfo {
                        strategy_name: "sid3".to_string(),
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
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_pools_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(500_u128),
                    strategy_name: Some("sid3".to_string()),
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
            .may_load(deps.as_mut().storage, "sid3")
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
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(2000_u128, 1_u128),
                    airdrop_pointer: vec![]
                },
                UserStrategyInfo {
                    strategy_name: "sid2".to_string(),
                    shares: Decimal::from_ratio(3000_u128, 1_u128),
                    airdrop_pointer: vec![]
                },
                UserStrategyInfo {
                    strategy_name: "sid3".to_string(),
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
                    strategy_name: "sid1".to_string(),
                    deposit_fraction: Decimal::from_ratio(1_u128, 2_u128)
                },
                UserStrategyPortfolio {
                    strategy_name: "sid2".to_string(),
                    deposit_fraction: Decimal::from_ratio(1_u128, 4_u128)
                },
                UserStrategyPortfolio {
                    strategy_name: "sid3".to_string(),
                    deposit_fraction: Decimal::from_ratio(1_u128, 4_u128)
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
        assert_eq!(state.total_accumulated_rewards, Uint128::new(700_u128));

        /*
            Test - 2. User deposits to money and splits it across his portfolio
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(Some(contracts_to_token), None);

        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.total_accumulated_rewards = Uint128::new(100_u128);
                Ok(state)
            },
        );

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo {
                name: "sid2".to_string(),
                sic_contract_address: sic2_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![
                    UserStrategyPortfolio {
                        strategy_name: "sid1".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_name: "sid2".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 2_u128),
                    },
                ],
                strategies: vec![],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Uint128::new(300_u128),
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_pools_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(100_u128),
                    strategy_name: None,
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
            .may_load(deps.as_mut().storage, "sid1")
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
            .may_load(deps.as_mut().storage, "sid2")
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
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(250_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 1000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_name: "sid2".to_string(),
                    shares: Decimal::from_ratio(500_u128, 1_u128),
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
                Coin::new(0_u128, "anc".to_string()),
                Coin::new(0_u128, "mir".to_string()),
            ]
        ));
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_accumulated_rewards, Uint128::new(200_u128));

        /*
            Test - 3. User deposits to money but has an empty portfolio
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(Some(contracts_to_token), None);

        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.total_accumulated_rewards = Uint128::new(100_u128);
                Ok(state)
            },
        );

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo {
                name: "sid2".to_string(),
                sic_contract_address: sic2_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Uint128::new(300_u128),
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_pools_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(100_u128),
                    strategy_name: None,
                }],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        let sid1_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
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
            .may_load(deps.as_mut().storage, "sid2")
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
        assert_eq!(state.total_accumulated_rewards, Uint128::new(200_u128));

        /*
           Test - 4. User newly deposits with no strategy portfolio and no strategy specified.
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(Some(contracts_to_token), None);

        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.total_accumulated_rewards = Uint128::new(100_u128);
                Ok(state)
            },
        );

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                current_undelegated_shares: Default::default(),
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
                sic_contract_address: sic2_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: true,
                total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );

        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1.clone(),
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Default::default(),
            },
        );
        USER_REWARD_INFO_MAP.remove(deps.as_mut().storage, &user1);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_pools_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(100_u128),
                    strategy_name: None,
                }],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        let sid1_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
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
            .may_load(deps.as_mut().storage, "sid2")
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
        assert_eq!(state.total_accumulated_rewards, Uint128::new(200_u128));

        /*
           Test - 4. User deposits across his portfolio with existing deposits
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(200_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(100_u128));
        deps.querier.update_wasm(Some(contracts_to_token), None);

        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.total_accumulated_rewards = Uint128::new(100_u128);
                Ok(state)
            },
        );

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo {
                name: "sid2".to_string(),
                sic_contract_address: sic2_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![
                    UserStrategyPortfolio {
                        strategy_name: "sid1".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_name: "sid2".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 2_u128),
                    },
                ],
                strategies: vec![
                    UserStrategyInfo {
                        strategy_name: "sid1".to_string(),
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                    UserStrategyInfo {
                        strategy_name: "sid2".to_string(),
                        shares: Decimal::from_ratio(500_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                ],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Uint128::new(300_u128),
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_pools_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(100_u128),
                    strategy_name: None,
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
            .may_load(deps.as_mut().storage, "sid1")
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
            .may_load(deps.as_mut().storage, "sid2")
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
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(1125_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 1000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 1000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_name: "sid2".to_string(),
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
        assert_eq!(state.total_accumulated_rewards, Uint128::new(200_u128));

        /*
            Test - 4. Multiple user deposits across their existing portfolios with existing deposits
        */
        let mut contracts_to_token: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_token.insert(sic1_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic2_address.clone(), Uint128::new(0_u128));
        contracts_to_token.insert(sic3_address.clone(), Uint128::new(0_u128));
        deps.querier.update_wasm(Some(contracts_to_token), None);

        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.total_accumulated_rewards = Uint128::new(100_u128);
                Ok(state)
            },
        );

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid2",
            &StrategyInfo {
                name: "sid2".to_string(),
                sic_contract_address: sic2_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid3",
            &StrategyInfo {
                name: "sid3".to_string(),
                sic_contract_address: sic3_address.clone(),
                unbonding_period: (21 * 24 * 3600),
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user1,
            &UserRewardInfo {
                user_portfolio: vec![
                    UserStrategyPortfolio {
                        strategy_name: "sid1".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_name: "sid2".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 2_u128),
                    },
                ],
                strategies: vec![
                    UserStrategyInfo {
                        strategy_name: "sid1".to_string(),
                        shares: Decimal::from_ratio(2000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                    UserStrategyInfo {
                        strategy_name: "sid2".to_string(),
                        shares: Decimal::from_ratio(3000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                ],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Uint128::new(300_u128),
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user2,
            &UserRewardInfo {
                user_portfolio: vec![
                    UserStrategyPortfolio {
                        strategy_name: "sid1".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 2_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_name: "sid2".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_name: "sid3".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 8_u128),
                    },
                ],
                strategies: vec![
                    UserStrategyInfo {
                        strategy_name: "sid1".to_string(),
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                    UserStrategyInfo {
                        strategy_name: "sid2".to_string(),
                        shares: Decimal::from_ratio(2000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                    UserStrategyInfo {
                        strategy_name: "sid3".to_string(),
                        shares: Decimal::from_ratio(2000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                ],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Uint128::new(500_u128),
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user3,
            &UserRewardInfo {
                user_portfolio: vec![
                    UserStrategyPortfolio {
                        strategy_name: "sid1".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 2_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_name: "sid2".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                    },
                    UserStrategyPortfolio {
                        strategy_name: "sid3".to_string(),
                        deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                    },
                ],
                strategies: vec![
                    UserStrategyInfo {
                        strategy_name: "sid1".to_string(),
                        shares: Decimal::from_ratio(1000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                    UserStrategyInfo {
                        strategy_name: "sid2".to_string(),
                        shares: Decimal::from_ratio(2000_u128, 1_u128),
                        airdrop_pointer: vec![],
                    },
                ],
                pending_airdrops: vec![],
                undelegation_records: vec![],
                pending_rewards: Uint128::new(0_u128),
            },
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(get_pools_contract_address().as_ref(), &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![
                    UpdateUserRewardsRequest {
                        user: user1.clone(),
                        funds: Uint128::new(100_u128),
                        strategy_name: None,
                    },
                    UpdateUserRewardsRequest {
                        user: user2.clone(),
                        funds: Uint128::new(400_u128),
                        strategy_name: None,
                    },
                    UpdateUserRewardsRequest {
                        user: user3.clone(),
                        funds: Uint128::new(600_u128),
                        strategy_name: None,
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
                    funds: vec![Coin::new(200_u128, "uluna".to_string())]
                }),
            ]
        ));
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_accumulated_rewards, Uint128::new(1200_u128));
        let sid1_strategy_opt = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
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
            .may_load(deps.as_mut().storage, "sid2")
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
            .may_load(deps.as_mut().storage, "sid3")
            .unwrap();
        assert_ne!(sid3_strategy_opt, None);
        let sid3_strategy = sid3_strategy_opt.unwrap();
        assert_eq!(
            sid3_strategy.total_shares,
            Decimal::from_ratio(7000_u128, 1_u128)
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
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(2250_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 8000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 8000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_name: "sid2".to_string(),
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
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(3000_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 8000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 8000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_name: "sid2".to_string(),
                    shares: Decimal::from_ratio(3000_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 7000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 7000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_name: "sid3".to_string(),
                    shares: Decimal::from_ratio(2500_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(300_u128, 5000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(500_u128, 5000_u128), "mir".to_string()),
                    ]
                }
            ]
        ));
        assert_eq!(user2_reward_info.pending_rewards, Uint128::new(550_u128));
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
                    strategy_name: "sid1".to_string(),
                    shares: Decimal::from_ratio(4000_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(100_u128, 8000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(300_u128, 8000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_name: "sid2".to_string(),
                    shares: Decimal::from_ratio(3500_u128, 1_u128),
                    airdrop_pointer: vec![
                        DecCoin::new(Decimal::from_ratio(200_u128, 7000_u128), "anc".to_string()),
                        DecCoin::new(Decimal::from_ratio(400_u128, 7000_u128), "mir".to_string()),
                    ]
                },
                UserStrategyInfo {
                    strategy_name: "sid3".to_string(),
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
                strategy_name: "sid".to_string(),
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
                strategy_name: "sid".to_string(),
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
                strategy_name: strategy_name.clone(),
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
                strategy_name: strategy_name.clone(),
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
                strategy_name: "sid".to_string(),
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
                strategy_name: "sid".to_string(),
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
                strategy_name: "sid".to_string(),
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
                strategy_name: "sid".to_string(),
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
                strategy_name: "sid".to_string(),
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
                strategy_name: "sid".to_string(),
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
                strategy_name: "sid".to_string(),
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
                unbonding_buffer: 100,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
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
                user_portfolio: vec![],
                strategies: vec![],
                pending_airdrops: vec![Coin::new(10_u128, "abc"), Coin::new(200_u128, "def")],
                undelegation_records: vec![],
                pending_rewards: Default::default(),
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user2,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![],
                pending_airdrops: vec![Coin::new(20_u128, "abc"), Coin::new(100_u128, "def")],
                undelegation_records: vec![],
                pending_rewards: Default::default(),
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user3,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![],
                pending_airdrops: vec![Coin::new(30_u128, "abc"), Coin::new(50_u128, "def")],
                undelegation_records: vec![],
                pending_rewards: Default::default(),
            },
        );
        USER_REWARD_INFO_MAP.save(
            deps.as_mut().storage,
            &user4,
            &UserRewardInfo {
                user_portfolio: vec![],
                strategies: vec![],
                pending_airdrops: vec![Coin::new(40_u128, "abc"), Coin::new(80_u128, "def")],
                undelegation_records: vec![],
                pending_rewards: Default::default(),
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
