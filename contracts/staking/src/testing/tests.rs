#[cfg(test)]
mod tests {
    use crate::contract::{
        check_slashing, compute_withdrawable_funds, execute, instantiate, query,
    };
    use crate::error::ContractError;
    use crate::helpers::{
        get_active_validators_sorted_by_stake, get_validator_for_deposit, validate, Verify,
    };
    use crate::msg::{
        Cw20HookMsg, ExecuteMsg, GetFundsClaimRecord, InstantiateMsg, MerkleAirdropMsg,
        QueryConfigResponse, QueryMsg, QueryStateResponse,
    };
    use crate::state::{
        AirdropRate, BatchUndelegationRecord, Config, ConfigUpdateRequest, OperationControls,
        OperationControlsUpdateRequest, State, UndelegationInfo, VMeta,
        BATCH_UNDELEGATION_REGISTRY, CONFIG, OPERATION_CONTROLS, STATE, USERS, VALIDATOR_META,
    };
    use crate::testing::mock_querier::{mock_dependencies, WasmMockQuerier};
    use crate::testing::test_helpers::check_equal_vec;
    use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
    use cosmwasm_std::{
        from_binary, to_binary, Addr, Attribute, BankMsg, Coin, Decimal, Delegation,
        DistributionMsg, Env, FullDelegation, MessageInfo, OwnedDeps, StakingMsg, StdResult,
        SubMsg, Uint128, Validator, WasmMsg,
    };
    use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
    use reward::msg::ExecuteMsg as RewardExecuteMsg;
    use reward::state::{TmpManagerStore, TMP_MANAGER_STORE};

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
                amount: Coin::new(1000, "uluna"),
                can_redelegate: Coin::new(1000, "uluna"),
                accumulated_rewards: vec![Coin::new(20, "uluna"), Coin::new(30, "urew1")],
            },
            FullDelegation {
                delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                validator: "valid0002".to_string(),
                amount: Coin::new(1000, "uluna"),
                can_redelegate: Coin::new(0, "uluna"),
                accumulated_rewards: vec![Coin::new(40, "uluna"), Coin::new(60, "urew1")],
            },
            FullDelegation {
                delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                validator: "valid0003".to_string(),
                amount: Coin::new(1000, "uluna"),
                can_redelegate: Coin::new(0, "uluna"),
                accumulated_rewards: vec![],
            },
        ]
    }

    pub fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
        info: &MessageInfo,
        env: &Env,
    ) {
        let msg = InstantiateMsg {
            unbonding_period: 3600 * 24 * 21,
            undelegation_cooldown: 10,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
            reward_contract: "reward_contract".to_string(),
            airdrops_registry_contract: "airdrop_registry_contract".to_string(),
            airdrop_withdrawal_contract: "airdrop_withdrawal_contract".to_string(),
            protocol_fee_contract: "protocol_fee_contract".to_string(),
            protocol_reward_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            protocol_deposit_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            protocol_withdraw_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            reinvest_cooldown: 10,
        };

        instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    }

    #[test]
    fn proper_initialization_fail() {
        let mut deps = mock_dependencies(&[]);
        let _info = mock_info("creator", &[]);
        let _env = mock_env();

        /*
           Reward fee above limit
        */
        let msg = InstantiateMsg {
            unbonding_period: 3600 * 24 * 21,
            undelegation_cooldown: 10,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
            reward_contract: "reward_contract".to_string(),
            airdrops_registry_contract: "airdrop_registry_contract".to_string(),
            airdrop_withdrawal_contract: "airdrop_withdrawal_contract".to_string(),
            protocol_fee_contract: "protocol_fee_contract".to_string(),
            protocol_reward_fee: Decimal::from_ratio(11_u128, 100_u128), // 1%
            protocol_deposit_fee: Decimal::from_ratio(2_u128, 100_u128), // 1%
            protocol_withdraw_fee: Decimal::from_ratio(2_u128, 100_u128), // 1%
            reinvest_cooldown: 10,
        };
        let info = mock_info("creator", &[]);

        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::ProtocolFeeAboveLimit {}));

        /*
            Deposit fee above limit
        */
        let msg = InstantiateMsg {
            unbonding_period: 3600 * 24 * 21,
            undelegation_cooldown: 10,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
            reward_contract: "reward_contract".to_string(),
            airdrops_registry_contract: "airdrop_registry_contract".to_string(),
            airdrop_withdrawal_contract: "airdrop_withdrawal_contract".to_string(),
            protocol_fee_contract: "protocol_fee_contract".to_string(),
            protocol_reward_fee: Decimal::from_ratio(2_u128, 100_u128), // 1%
            protocol_deposit_fee: Decimal::from_ratio(6_u128, 100_u128), // 1%
            protocol_withdraw_fee: Decimal::from_ratio(2_u128, 100_u128), // 1%
            reinvest_cooldown: 10,
        };
        let info = mock_info("creator", &[]);

        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::ProtocolFeeAboveLimit {}));

        /*
           Withdraw fee above limit
        */
        let msg = InstantiateMsg {
            unbonding_period: 3600 * 24 * 21,
            undelegation_cooldown: 10,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
            reward_contract: "reward_contract".to_string(),
            airdrops_registry_contract: "airdrop_registry_contract".to_string(),
            airdrop_withdrawal_contract: "airdrop_withdrawal_contract".to_string(),
            protocol_fee_contract: "protocol_fee_contract".to_string(),
            protocol_reward_fee: Decimal::from_ratio(2_u128, 100_u128), // 1%
            protocol_deposit_fee: Decimal::from_ratio(2_u128, 100_u128), // 1%
            protocol_withdraw_fee: Decimal::from_ratio(8_u128, 100_u128), // 1%
            reinvest_cooldown: 10,
        };
        let info = mock_info("creator", &[]);

        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::ProtocolFeeAboveLimit {}));
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);
        let _info = mock_info("creator", &[]);
        let env = mock_env();

        let msg = InstantiateMsg {
            unbonding_period: 3600 * 24 * 21,
            undelegation_cooldown: 10,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
            reward_contract: "reward_contract".to_string(),
            airdrops_registry_contract: "airdrop_registry_contract".to_string(),
            airdrop_withdrawal_contract: "airdrop_withdrawal_contract".to_string(),
            protocol_fee_contract: "protocol_fee_contract".to_string(),
            protocol_reward_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            protocol_deposit_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            protocol_withdraw_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            reinvest_cooldown: 10,
        };
        let expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "uluna".to_string(),
            unbonding_period: 3600 * 24 * 21,
            undelegation_cooldown: 10,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
            reward_contract: Addr::unchecked("reward_contract"),
            cw20_token_contract: Addr::unchecked("0"),
            airdrop_registry_contract: Addr::unchecked("airdrop_registry_contract"),
            airdrop_withdrawal_contract: Addr::unchecked("airdrop_withdrawal_contract"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_contract"),
            protocol_reward_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            protocol_deposit_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            protocol_withdraw_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            reinvest_cooldown: 10,
        };
        let info = mock_info("creator", &[]);

        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(DistributionMsg::SetWithdrawAddress {
                address: "reward_contract".to_string()
            })]
        ));

        let config_res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let config: QueryConfigResponse = from_binary(&config_res).unwrap();
        assert_eq!(config.config, expected_config);

        let state_res = query(deps.as_ref(), mock_env(), QueryMsg::State {}).unwrap();
        let state: QueryStateResponse = from_binary(&state_res).unwrap();
        assert_eq!(
            state.state,
            State {
                total_staked: Uint128::zero(),
                exchange_rate: Decimal::one(),
                last_reconciled_batch_id: 0,
                current_undelegation_batch_id: 1,
                last_undelegation_time: env
                    .block
                    .time
                    .minus_seconds(config.config.undelegation_cooldown),
                last_reinvest_time: env
                    .block
                    .time
                    .minus_seconds(config.config.reinvest_cooldown),
                validators: vec![],
                reconciled_funds_to_withdraw: Uint128::zero()
            }
        );
    }

    #[test]
    fn test_get_active_validators_sorted_by_stake() {
        let mut deps = mock_dependencies(&[]);
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let _res = instantiate_contract(&mut deps, &info, &env);

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");

        /*
           Test - 1. Empty validator pool
        */
        let err = get_active_validators_sorted_by_stake(
            deps.as_mut().querier,
            vec![],
            convert_to_delegation(get_delegations()).as_slice(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoValidatorsInPool {}));

        /*
           Test - 2. All validators are jailed
        */
        deps.querier.update_staking("uluna", &[], &[]);
        let err = get_active_validators_sorted_by_stake(
            deps.as_mut().querier,
            vec![valid1.clone(), valid2.clone(), valid3.clone()],
            convert_to_delegation(get_delegations()).as_slice(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::AllValidatorsJailed {}));

        /*
            Test - 3. Successful
        */
        fn get_validators_test_3() -> Vec<Validator> {
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
        fn get_delegations_test_3() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(1000, "uluna"),
                    can_redelegate: Coin::new(1000, "uluna"),
                    accumulated_rewards: vec![Coin::new(20, "uluna"), Coin::new(30, "urew1")],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(2000, "uluna"),
                    can_redelegate: Coin::new(0, "uluna"),
                    accumulated_rewards: vec![Coin::new(40, "uluna"), Coin::new(60, "urew1")],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0003".to_string(),
                    amount: Coin::new(3000, "uluna"),
                    can_redelegate: Coin::new(0, "uluna"),
                    accumulated_rewards: vec![Coin::new(40, "uluna"), Coin::new(60, "urew1")],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_validators_test_3(),
            &*get_delegations_test_3(),
        );
        let res = get_active_validators_sorted_by_stake(
            deps.as_mut().querier,
            vec![valid1.clone(), valid2.clone(), valid3.clone()],
            convert_to_delegation(get_delegations_test_3()).as_slice(),
        )
        .unwrap();
        assert!(check_equal_vec(
            res,
            vec![
                (Uint128::new(1000_u128), valid1.to_string()),
                (Uint128::new(2000_u128), valid2.to_string()),
                (Uint128::new(3000_u128), valid3.to_string())
            ]
        ));
    }

    #[test]
    fn test_get_validator_for_deposit() {
        let mut deps = mock_dependencies(&[]);
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let _res = instantiate_contract(&mut deps, &info, &env);

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");

        /*
           Test - 1. Empty validator pool
        */
        let err = get_validator_for_deposit(
            deps.as_mut().querier,
            vec![],
            convert_to_delegation(get_delegations()).as_slice(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoValidatorsInPool {}));

        /*
           Test - 2. Get Validator with no delegation
        */
        fn get_validators_test_1() -> Vec<Validator> {
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
        fn get_delegations_test_1() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(1000, "uluna"),
                    can_redelegate: Coin::new(1000, "uluna"),
                    accumulated_rewards: vec![Coin::new(20, "uluna"), Coin::new(30, "urew1")],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(1000, "uluna"),
                    can_redelegate: Coin::new(0, "uluna"),
                    accumulated_rewards: vec![Coin::new(40, "uluna"), Coin::new(60, "urew1")],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_validators_test_1(),
            &*get_delegations_test_1(),
        );
        let res = get_validator_for_deposit(
            deps.as_mut().querier,
            vec![valid1.clone(), valid2.clone(), valid3.clone()],
            convert_to_delegation(get_delegations_test_1()).as_slice(),
        )
        .unwrap();
        assert_eq!(res, valid3);

        /*
           Test - 3. Validator with smallest delegation
        */
        fn get_validators_test_2() -> Vec<Validator> {
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
        fn get_delegations_test_2() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(1000, "uluna"),
                    can_redelegate: Coin::new(1000, "uluna"),
                    accumulated_rewards: vec![Coin::new(20, "uluna"), Coin::new(30, "urew1")],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(2000, "uluna"),
                    can_redelegate: Coin::new(0, "uluna"),
                    accumulated_rewards: vec![Coin::new(40, "uluna"), Coin::new(60, "urew1")],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0003".to_string(),
                    amount: Coin::new(3000, "uluna"),
                    can_redelegate: Coin::new(0, "uluna"),
                    accumulated_rewards: vec![Coin::new(40, "uluna"), Coin::new(60, "urew1")],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_validators_test_2(),
            &*get_delegations_test_2(),
        );
        let res = get_validator_for_deposit(
            deps.as_mut().querier,
            vec![valid1.clone(), valid2.clone(), valid3.clone()],
            convert_to_delegation(get_delegations_test_2()).as_slice(),
        )
        .unwrap();
        assert_eq!(res, valid1);
    }

    #[test]
    fn test_validate() {
        let mut deps = mock_dependencies(&[]);
        let env = mock_env();
        let info = mock_info("creator", &[]);
        let _res = instantiate_contract(&mut deps, &info, &env);

        /*
           Test - 1. Check send manager
        */
        let info = mock_info("not-creator", &[]);
        let mut config = CONFIG.load(deps.as_mut().storage).unwrap();
        config.manager = Addr::unchecked("creator");
        let err = validate(&config, &info, &env, vec![Verify::SenderManager]).unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Check NonZeroSingleInfoFund
        */
        let info = mock_info("not-creator", &[]);
        let mut config = CONFIG.load(deps.as_mut().storage).unwrap();
        config.manager = Addr::unchecked("creator");
        let err = validate(&config, &info, &env, vec![Verify::NonZeroSingleInfoFund]).unwrap_err();
        assert!(matches!(err, ContractError::NoFunds {}));

        let info = mock_info(
            "creator",
            &[
                Coin::new(100_u128, "uluna"),
                Coin::new(1000_u128, "uusd".to_string()),
            ],
        );
        let mut config = CONFIG.load(deps.as_mut().storage).unwrap();
        config.manager = Addr::unchecked("creator");
        let err = validate(&config, &info, &env, vec![Verify::NonZeroSingleInfoFund]).unwrap_err();
        assert!(matches!(err, ContractError::MultipleFunds {}));

        let info = mock_info("creator", &[Coin::new(100_u128, "ulunsda")]);
        let mut config = CONFIG.load(deps.as_mut().storage).unwrap();
        config.manager = Addr::unchecked("creator");
        let err = validate(&config, &info, &env, vec![Verify::NonZeroSingleInfoFund]).unwrap_err();
        assert!(matches!(err, ContractError::InvalidDenom {}));

        /*
            Test - 3. Check NoFunds
        */
        let info = mock_info("creator", &[Coin::new(100_u128, "uluna")]);
        let mut config = CONFIG.load(deps.as_mut().storage).unwrap();
        config.manager = Addr::unchecked("creator");
        let err = validate(&config, &info, &env, vec![Verify::NoFunds]).unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));
    }

    #[test]
    fn test_update_operation_controls() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        /*
           Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::UpdateOperationFlags {
                operation_controls_update_request: OperationControlsUpdateRequest {
                    deposit_paused: None,
                    queue_undelegate_paused: None,
                    undelegate_paused: None,
                    withdraw_paused: None,
                    reinvest_paused: None,
                    reconcile_paused: None,
                    claim_airdrops_paused: None,
                    redeem_rewards_paused: None,
                },
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
            Successful update
        */
        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: true,
                    queue_undelegate_paused: false,
                    undelegate_paused: true,
                    withdraw_paused: false,
                    reinvest_paused: true,
                    reconcile_paused: false,
                    claim_airdrops_paused: true,
                    redeem_rewards_paused: true,
                },
            )
            .unwrap();
        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateOperationFlags {
                operation_controls_update_request: OperationControlsUpdateRequest {
                    deposit_paused: Some(false),
                    queue_undelegate_paused: None,
                    undelegate_paused: Some(false),
                    withdraw_paused: None,
                    reinvest_paused: None,
                    reconcile_paused: Some(true),
                    claim_airdrops_paused: Some(false),
                    redeem_rewards_paused: None,
                },
            },
        )
        .unwrap();
        let operation_controls = OPERATION_CONTROLS.load(deps.as_mut().storage).unwrap();
        assert_eq!(
            operation_controls,
            OperationControls {
                deposit_paused: false,
                queue_undelegate_paused: false,
                undelegate_paused: false,
                withdraw_paused: false,
                reinvest_paused: true,
                reconcile_paused: true,
                claim_airdrops_paused: false,
                redeem_rewards_paused: true,
            }
        );
    }

    #[test]
    fn test_set_and_accept_manager_successful() {
        let mut deps = mock_dependencies(&[]);
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let _res = instantiate_contract(&mut deps, &info, &env);

        let tmp_manager_store = TMP_MANAGER_STORE.may_load(deps.as_mut().storage).unwrap();
        assert_eq!(tmp_manager_store, None);
        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::SetManager {
                manager: "new_manager1".to_string(),
            },
        )
        .unwrap();
        let tmp_manager_store = TMP_MANAGER_STORE.load(deps.as_mut().storage).unwrap();
        assert_eq!(tmp_manager_store.manager, "new_manager1".to_string());

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("new_manager1", &[]),
            ExecuteMsg::AcceptManager {},
        )
        .unwrap();
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        let tmp_manager_store = TMP_MANAGER_STORE.may_load(deps.as_mut().storage).unwrap();

        assert_eq!(config.manager, Addr::unchecked("new_manager1"));
        assert_eq!(tmp_manager_store, None);
        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("new_manager1", &[]),
            ExecuteMsg::SetManager {
                manager: "new_manager2".to_string(),
            },
        )
        .unwrap();
        let tmp_manager_store = TMP_MANAGER_STORE.load(deps.as_mut().storage).unwrap();
        assert_eq!(tmp_manager_store.manager, "new_manager2".to_string());

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("new_manager2", &[]),
            ExecuteMsg::AcceptManager {},
        )
        .unwrap();
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config.manager, Addr::unchecked("new_manager2"));
        let _tmp_manager_store = TMP_MANAGER_STORE.may_load(deps.as_mut().storage).unwrap();
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
        let _res = execute(
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

        /*
            Successful
        */
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
        let _res = execute(
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

    #[test]
    fn test_update_config() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);
        let initial_state = STATE.load(deps.as_mut().storage).unwrap();

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::UpdateConfig {
                config_request: ConfigUpdateRequest {
                    min_deposit: None,
                    max_deposit: None,
                    cw20_token_contract: None,
                    protocol_reward_fee: None,
                    protocol_withdraw_fee: None,
                    protocol_deposit_fee: None,
                    airdrop_registry_contract: None,
                    unbonding_period: None,
                    undelegation_cooldown: None,
                    reinvest_cooldown: None,
                },
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        // Authorized but no changes
        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                config_request: ConfigUpdateRequest {
                    min_deposit: None,
                    max_deposit: None,
                    cw20_token_contract: None,
                    protocol_reward_fee: None,
                    protocol_withdraw_fee: None,
                    protocol_deposit_fee: None,
                    airdrop_registry_contract: None,
                    unbonding_period: None,
                    undelegation_cooldown: None,
                    reinvest_cooldown: None,
                },
            },
        )
        .unwrap();
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state, initial_state);

        /*
            Test - 2.
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                config_request: ConfigUpdateRequest {
                    min_deposit: Some(Uint128::from(1_u128)),
                    max_deposit: Some(Uint128::from(10000000_u128)),
                    cw20_token_contract: Some("cw20_token_contract".parse().unwrap()),
                    protocol_reward_fee: Some(Decimal::from_ratio(11_u128, 100_u128)),
                    protocol_withdraw_fee: Some(Decimal::from_ratio(2_u128, 100_u128)),
                    protocol_deposit_fee: Some(Decimal::from_ratio(2_u128, 100_u128)),
                    airdrop_registry_contract: None,
                    unbonding_period: Some(100u64),
                    undelegation_cooldown: Some(10000u64),
                    reinvest_cooldown: Some(234u64),
                },
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ProtocolFeeAboveLimit {}));

        /*
            Test - 2.
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                config_request: ConfigUpdateRequest {
                    min_deposit: Some(Uint128::from(1_u128)),
                    max_deposit: Some(Uint128::from(10000000_u128)),
                    cw20_token_contract: Some("cw20_token_contract".parse().unwrap()),
                    protocol_reward_fee: Some(Decimal::from_ratio(2_u128, 100_u128)),
                    protocol_withdraw_fee: Some(Decimal::from_ratio(6_u128, 100_u128)),
                    protocol_deposit_fee: Some(Decimal::from_ratio(2_u128, 100_u128)),
                    airdrop_registry_contract: Some("airdrop_registry_contract".to_string()),
                    unbonding_period: Some(100u64),
                    undelegation_cooldown: Some(10000u64),
                    reinvest_cooldown: Some(234u64),
                },
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ProtocolFeeAboveLimit {}));

        /*
            Test - 2.
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                config_request: ConfigUpdateRequest {
                    min_deposit: Some(Uint128::from(1_u128)),
                    max_deposit: Some(Uint128::from(10000000_u128)),
                    cw20_token_contract: Some("cw20_token_contract".parse().unwrap()),
                    protocol_reward_fee: Some(Decimal::from_ratio(2_u128, 100_u128)),
                    protocol_withdraw_fee: Some(Decimal::from_ratio(2_u128, 100_u128)),
                    protocol_deposit_fee: Some(Decimal::from_ratio(8_u128, 100_u128)),
                    airdrop_registry_contract: None,
                    unbonding_period: Some(100u64),
                    undelegation_cooldown: Some(10000u64),
                    reinvest_cooldown: Some(234u64),
                },
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ProtocolFeeAboveLimit {}));

        /*
           Test - 2.
        */
        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                config_request: ConfigUpdateRequest {
                    min_deposit: Some(Uint128::from(1_u128)),
                    max_deposit: Some(Uint128::from(10000000_u128)),
                    cw20_token_contract: Some("cw20_token_contract".parse().unwrap()),
                    protocol_reward_fee: Some(Decimal::from_ratio(2_u128, 100_u128)),
                    protocol_withdraw_fee: Some(Decimal::from_ratio(2_u128, 100_u128)),
                    protocol_deposit_fee: Some(Decimal::from_ratio(2_u128, 100_u128)),
                    airdrop_registry_contract: Some("airdrop_registry_contract".to_string()),
                    unbonding_period: Some(100u64),
                    undelegation_cooldown: Some(10000u64),
                    reinvest_cooldown: Some(234u64),
                },
            },
        )
        .unwrap();
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config.min_deposit, Uint128::new(1_u128));
        assert_eq!(config.max_deposit, Uint128::new(10000000_u128));
        assert_eq!(
            config.cw20_token_contract,
            Addr::unchecked("cw20_token_contract")
        );
        assert_eq!(
            config.protocol_fee_contract,
            Addr::unchecked("protocol_fee_contract")
        );
        assert_eq!(
            config.airdrop_withdrawal_contract,
            Addr::unchecked("airdrop_withdrawal_contract")
        );
        assert_eq!(
            config.airdrop_registry_contract,
            Addr::unchecked("airdrop_registry_contract")
        );
        assert_eq!(
            config.protocol_reward_fee,
            Decimal::from_ratio(2_u128, 100_u128)
        );
        assert_eq!(
            config.protocol_withdraw_fee,
            Decimal::from_ratio(2_u128, 100_u128)
        );
        assert_eq!(
            config.protocol_deposit_fee,
            Decimal::from_ratio(2_u128, 100_u128)
        );
        assert_eq!(config.unbonding_period, 100u64);
        assert_eq!(config.undelegation_cooldown, 10000u64);
        assert_eq!(config.reinvest_cooldown, 234u64);
    }

    fn convert_to_delegation(full_delegations: Vec<FullDelegation>) -> Vec<Delegation> {
        full_delegations
            .iter()
            .map(|fd| Delegation {
                delegator: fd.delegator.clone(),
                validator: fd.validator.clone(),
                amount: fd.amount.clone(),
            })
            .collect::<Vec<Delegation>>()
    }

    #[test]
    fn test_check_slashing() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");

        /*
           Test - 1. There is no slashing
        */
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    Ok(state)
                },
            )
            .unwrap();

        // let delegations = get_delegations();
        check_slashing(
            &mut deps.as_mut(),
            &env,
            convert_to_delegation(get_delegations()).as_slice(),
        )
        .unwrap();
        let val1_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid1).unwrap();
        let val2_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid2).unwrap();
        let val3_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid3).unwrap();
        assert_eq!(
            val1_meta,
            VMeta {
                staked: Uint128::new(1000_u128),
                slashed: Uint128::zero(),
                filled: Default::default()
            }
        );
        assert_eq!(
            val2_meta,
            VMeta {
                staked: Uint128::new(1000_u128),
                slashed: Uint128::zero(),
                filled: Default::default()
            }
        );
        assert_eq!(
            val3_meta,
            VMeta {
                staked: Uint128::new(1000_u128),
                slashed: Uint128::zero(),
                filled: Default::default()
            }
        );

        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.total_staked, Uint128::new(3000_u128));
        assert_eq!(state.exchange_rate, Decimal::one());

        /*
            Test - 2. There is some slashing
        */
        fn get_validators_test_2() -> Vec<Validator> {
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

        fn get_delegations_test_2() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(500, "uluna"),
                    can_redelegate: Coin::new(1000, "uluna"),
                    accumulated_rewards: vec![Coin::new(20, "uluna"), Coin::new(30, "urew1")],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(1000, "uluna"),
                    can_redelegate: Coin::new(0, "uluna"),
                    accumulated_rewards: vec![Coin::new(40, "uluna"), Coin::new(60, "urew1")],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0003".to_string(),
                    amount: Coin::new(1000, "uluna"),
                    can_redelegate: Coin::new(0, "uluna"),
                    accumulated_rewards: vec![],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_validators_test_2(),
            &*get_delegations_test_2(),
        );
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Default::default(),
                },
            )
            .unwrap();

        check_slashing(
            &mut deps.as_mut(),
            &env,
            convert_to_delegation(get_delegations_test_2()).as_slice(),
        )
        .unwrap();
        let val1_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid1).unwrap();
        let val2_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid2).unwrap();
        let val3_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid3).unwrap();
        assert_eq!(
            val1_meta,
            VMeta {
                staked: Uint128::new(500_u128),
                slashed: Uint128::new(500_u128),
                filled: Default::default()
            }
        );
        assert_eq!(
            val2_meta,
            VMeta {
                staked: Uint128::new(1000_u128),
                slashed: Uint128::zero(),
                filled: Default::default()
            }
        );
        assert_eq!(
            val3_meta,
            VMeta {
                staked: Uint128::new(1000_u128),
                slashed: Uint128::zero(),
                filled: Default::default()
            }
        );

        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.total_staked, Uint128::new(2500_u128));
        assert_eq!(
            state.exchange_rate,
            Decimal::from_ratio(2500_u128, 3000_u128)
        );

        /*
            Test - 3. There is some yield
        */
        fn get_validators_test_3() -> Vec<Validator> {
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

        fn get_delegations_test_3() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(1500, "uluna"),
                    can_redelegate: Coin::new(1000, "uluna"),
                    accumulated_rewards: vec![Coin::new(20, "uluna"), Coin::new(30, "urew1")],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(1000, "uluna"),
                    can_redelegate: Coin::new(0, "uluna"),
                    accumulated_rewards: vec![Coin::new(40, "uluna"), Coin::new(60, "urew1")],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0003".to_string(),
                    amount: Coin::new(1000, "uluna"),
                    can_redelegate: Coin::new(0, "uluna"),
                    accumulated_rewards: vec![],
                },
            ]
        }
        deps.querier.update_staking(
            "uluna",
            &*get_validators_test_3(),
            &*get_delegations_test_3(),
        );
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Default::default(),
                },
            )
            .unwrap();

        check_slashing(
            &mut deps.as_mut(),
            &env,
            convert_to_delegation(get_delegations_test_3()).as_slice(),
        )
        .unwrap();
        let val1_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid1).unwrap();
        let val2_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid2).unwrap();
        let val3_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid3).unwrap();
        assert_eq!(
            val1_meta,
            VMeta {
                staked: Uint128::new(1500_u128),
                slashed: Uint128::zero(),
                filled: Default::default()
            }
        );
        assert_eq!(
            val2_meta,
            VMeta {
                staked: Uint128::new(1000_u128),
                slashed: Uint128::zero(),
                filled: Default::default()
            }
        );
        assert_eq!(
            val3_meta,
            VMeta {
                staked: Uint128::new(1000_u128),
                slashed: Uint128::zero(),
                filled: Default::default()
            }
        );

        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.total_staked, Uint128::new(3500_u128));
        assert_eq!(
            state.exchange_rate,
            Decimal::from_ratio(3500_u128, 3000_u128)
        );
    }

    #[test]
    fn test_add_validator_fail() {
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
            ExecuteMsg::AddValidator {
                val_addr: Addr::unchecked("test_val"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
            Test - 2. Validator already added
        */
        let val_addr = Addr::unchecked("val_addr");
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![val_addr.clone()];
                    Ok(state)
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                val_addr: val_addr.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorAlreadyAdded {}));

        /*
            Test - 3. Validator not discoverable
        */
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                val_addr: Addr::unchecked("test_val"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotDiscoverable {}));
    }

    #[test]
    fn test_add_validator_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        /*
           Test - 1. Successful add
        */
        let val_addr = Addr::unchecked("valid0001");
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                val_addr: val_addr.clone(),
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert_eq!(
            res.attributes,
            vec![Attribute {
                key: "new_validator".to_string(),
                value: val_addr.to_string()
            }]
        );
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.validators, vec![val_addr.clone()]);
        let val_meta = VALIDATOR_META
            .load(deps.as_mut().storage, &val_addr)
            .unwrap();
        assert_eq!(val_meta, VMeta::new());
    }

    #[test]
    fn test_redeem_rewards() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");

        /*
           Redeem rewards
        */
        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: true,
                    queue_undelegate_paused: false,
                    undelegate_paused: false,
                    withdraw_paused: false,
                    reinvest_paused: false,
                    reconcile_paused: false,
                    claim_airdrops_paused: false,
                    redeem_rewards_paused: true,
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[Coin::new(10_u128, "uluna".to_string())]),
            ExecuteMsg::RedeemRewards { validators: None },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::OperationPaused(String { .. })));

        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: false,
                    queue_undelegate_paused: false,
                    undelegate_paused: false,
                    withdraw_paused: false,
                    reinvest_paused: false,
                    reconcile_paused: false,
                    claim_airdrops_paused: false,
                    redeem_rewards_paused: false,
                },
            )
            .unwrap();

        /*
           Test - 1 - No failed validators
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Uint128::new(1000_u128),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Uint128::new(1000_u128),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Uint128::new(1000_u128),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemRewards { validators: None },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 3);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0001".to_string()
                }),
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0002".to_string()
                }),
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0003".to_string()
                })
            ]
        ));

        /*
            Test - 2 - Some failed validators
        */
        let valid4 = Addr::unchecked("valid0004");
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![
                        valid1.clone(),
                        valid2.clone(),
                        valid3.clone(),
                        valid4.clone(),
                    ];
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Uint128::new(1000_u128),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Uint128::new(1000_u128),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Uint128::new(1000_u128),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(deps.as_mut().storage, &valid4, &VMeta::new())
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemRewards { validators: None },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert_eq!(
            res.attributes,
            vec![Attribute {
                key: "failed_validators".to_string(),
                value: "valid0004".to_string()
            }]
        );
        assert_eq!(res.messages.len(), 3);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0001".to_string()
                }),
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0002".to_string()
                }),
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0003".to_string()
                })
            ]
        ));

        /*
            Test - 3 - Only selected validators
        */
        let valid4 = Addr::unchecked("valid0004");
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![
                        valid1.clone(),
                        valid2.clone(),
                        valid3.clone(),
                        valid4.clone(),
                    ];
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Uint128::new(1000_u128),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Uint128::new(1000_u128),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Uint128::zero(),
                    filled: Uint128::new(1000_u128),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(deps.as_mut().storage, &valid4, &VMeta::new())
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemRewards {
                validators: Some(vec![valid1, valid2]),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0001".to_string()
                }),
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0002".to_string()
                }),
            ]
        ));
    }

    #[test]
    fn test_rebalance_pool_fail() {
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
            ExecuteMsg::RebalancePool {
                amount: Uint128::new(100_u128),
                val_addr: Addr::unchecked("val_addr"),
                redel_addr: Addr::unchecked("redel_addr"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
            Test - 2. Zero amount
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RebalancePool {
                amount: Uint128::zero(),
                val_addr: Addr::unchecked("val_addr"),
                redel_addr: Addr::unchecked("redel_addr"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ZeroAmount {}));

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    Ok(state)
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        /*
            Test - 2. Validators cannot be the same
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RebalancePool {
                amount: Uint128::new(100_u128),
                val_addr: Addr::unchecked("val_addr"),
                redel_addr: Addr::unchecked("val_addr"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorsCannotBeSame {}));

        /*
            Test - 3. Validator not added
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone()];
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RebalancePool {
                amount: Uint128::new(100_u128),
                val_addr: Addr::unchecked("val_addr1"),
                redel_addr: Addr::unchecked("val_addr"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        /*
            Test - 4. Insufficient funds
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone()];
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RebalancePool {
                amount: Uint128::new(10000_u128),
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InSufficientFunds {}));

        /*
            Test - 5. Redelegation in progress
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
                Validator {
                    address: "valid0003".to_string(),
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
                    amount: Coin::new(1000, "uluna"),
                    can_redelegate: Coin::new(0, "uluna"),
                    accumulated_rewards: vec![Coin::new(20, "uluna"), Coin::new(30, "urew1")],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(1000, "uluna"),
                    can_redelegate: Coin::new(0, "uluna"),
                    accumulated_rewards: vec![Coin::new(40, "uluna"), Coin::new(60, "urew1")],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0003".to_string(),
                    amount: Coin::new(1000, "uluna"),
                    can_redelegate: Coin::new(0, "uluna"),
                    accumulated_rewards: vec![],
                },
            ]
        }
        deps.querier
            .update_staking("uluna", &*get_validators_test(), &*get_delegations_test());
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone()];
                    Ok(state)
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RebalancePool {
                amount: Uint128::new(600_u128),
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::RedelegationInProgress {}));
    }

    #[test]
    fn test_rebalance_pool_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        /*
           Test - 1. Success
        */
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    Ok(state)
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RebalancePool {
                amount: Uint128::new(100_u128),
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(StakingMsg::Redelegate {
                src_validator: valid1.to_string(),
                dst_validator: valid2.to_string(),
                amount: Coin::new(100_u128, "uluna".to_string())
            })]
        ));
        let val1_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid1).unwrap();
        let val2_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid2).unwrap();
        assert_eq!(
            val1_meta,
            VMeta {
                staked: Uint128::new(900_u128),
                slashed: Default::default(),
                filled: Default::default()
            }
        );
        assert_eq!(
            val2_meta,
            VMeta {
                staked: Uint128::new(1100_u128),
                slashed: Default::default(),
                filled: Default::default()
            }
        );
    }

    #[test]
    fn test_remove_validator_from_pool_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: Addr::unchecked("abcde"),
                redel_addr: Addr::unchecked("redel_ade"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Validator not added
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: Addr::unchecked("abcde"),
                redel_addr: Addr::unchecked("redel_ade"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        /*
            Test - 3. Validator addresses should not be the same
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: Addr::unchecked("abcde"),
                redel_addr: Addr::unchecked("abcde"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorsCannotBeSame {}));

        /*
           Test - 4. Redelegation in progress
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid2.clone(),
                redel_addr: valid3.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::RedelegationInProgress {}));
    }

    #[test]
    fn test_remove_validator_from_pool_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        /*
           Test - 1. Validator with delegation
        */
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(StakingMsg::Redelegate {
                src_validator: valid1.to_string(),
                dst_validator: valid2.to_string(),
                amount: Coin::new(1000_u128, "uluna".to_string())
            })]
        ));
        let val1_meta = VALIDATOR_META
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert_eq!(val1_meta, None);
        let val2_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid2).unwrap();
        assert_eq!(
            val2_meta,
            VMeta {
                staked: Uint128::new(2000_u128),
                slashed: Default::default(),
                filled: Default::default()
            }
        );
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert!(check_equal_vec(
            state.validators,
            vec![valid2.clone(), valid3.clone()]
        ));
    }

    #[test]
    fn test_deposit_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");

        /*
           Deposit paused
        */
        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: true,
                    queue_undelegate_paused: false,
                    undelegate_paused: false,
                    withdraw_paused: false,
                    reinvest_paused: false,
                    reconcile_paused: false,
                    claim_airdrops_paused: false,
                    redeem_rewards_paused: false,
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[Coin::new(10_u128, "uluna".to_string())]),
            ExecuteMsg::Deposit {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::OperationPaused(String { .. })));

        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: false,
                    queue_undelegate_paused: false,
                    undelegate_paused: false,
                    withdraw_paused: false,
                    reinvest_paused: false,
                    reconcile_paused: false,
                    claim_airdrops_paused: false,
                    redeem_rewards_paused: false,
                },
            )
            .unwrap();

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    Ok(state)
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);

        /*
           Test - 1. crossed max deposit
        */
        CONFIG
            .update(
                deps.as_mut().storage,
                |mut config| -> Result<_, ContractError> {
                    config.max_deposit = Uint128::new(100_u128);
                    Ok(config)
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[Coin::new(120_u128, "uluna".to_string())]),
            ExecuteMsg::Deposit {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::MaxDeposit {}));

        /*
           Test - 1. crossed max deposit
        */
        CONFIG
            .update(
                deps.as_mut().storage,
                |mut config| -> Result<_, ContractError> {
                    config.min_deposit = Uint128::new(10_u128);
                    Ok(config)
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[Coin::new(5_u128, "uluna".to_string())]),
            ExecuteMsg::Deposit {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::MinDeposit {}));
    }

    #[test]
    fn test_deposit_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        /*
           Test - 1. Successful deposit
        */
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    state.total_staked = Uint128::new(3000_u128);
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[Coin::new(1000_u128, "uluna".to_string())]),
            ExecuteMsg::Deposit {},
        )
        .unwrap();
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(res.messages.len(), 3);
        assert_eq!(
            res.messages,
            vec![
                SubMsg::new(BankMsg::Send {
                    to_address: "protocol_fee_contract".to_string(),
                    amount: vec![Coin::new(10_u128, "uluna")]
                }),
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(990_u128, "uluna".to_string())
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: config.cw20_token_contract.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Mint {
                        recipient: "other".to_string(),
                        amount: Uint128::new(990_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                })
            ]
        );
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.total_staked, Uint128::new(3990_u128));
        let val1_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid1).unwrap();
        assert_eq!(
            val1_meta,
            VMeta {
                staked: Uint128::new(1990_u128),
                slashed: Uint128::zero(),
                filled: Default::default()
            }
        );
    }

    #[test]
    fn test_queue_undelegation() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        CONFIG
            .update(
                deps.as_mut().storage,
                |mut config| -> Result<_, ContractError> {
                    config.cw20_token_contract = Addr::unchecked("cw20_contract");
                    Ok(config)
                },
            )
            .unwrap();

        /*
           Test - 1. Successful undelegation
        */
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    state.total_staked = Uint128::new(3000_u128);
                    state.current_undelegation_batch_id = 3;
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                3,
                &BatchUndelegationRecord {
                    undelegated_tokens: Uint128::new(1000_u128),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: false,
                    undelegation_er: Default::default(),
                    undelegated_stake: Default::default(),
                    unbonding_slashing_ratio: Default::default(),
                },
            )
            .unwrap();
        let user1 = Addr::unchecked("user1");
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("cw20_contract", &[]),
            ExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: "user1".to_string(),
                amount: Uint128::new(100_u128),
                msg: to_binary(&Cw20HookMsg::QueueUndelegate {}).unwrap(),
            }),
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let user_undelegation_record = USERS.load(deps.as_mut().storage, (&user1, 3)).unwrap();
        assert_eq!(
            user_undelegation_record,
            UndelegationInfo {
                batch_id: 3,
                token_amount: Uint128::new(100_u128)
            }
        );
        let batch_undel_record = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, 3)
            .unwrap();
        assert_eq!(
            batch_undel_record.undelegated_tokens,
            Uint128::new(1100_u128)
        );
    }

    #[test]
    fn test_reinvest() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        /*
           reinvest paused
        */
        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: false,
                    queue_undelegate_paused: false,
                    undelegate_paused: false,
                    withdraw_paused: false,
                    reinvest_paused: true,
                    reconcile_paused: false,
                    claim_airdrops_paused: false,
                    redeem_rewards_paused: false,
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::OperationPaused(String { .. })));

        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: false,
                    queue_undelegate_paused: false,
                    undelegate_paused: false,
                    withdraw_paused: false,
                    reinvest_paused: false,
                    reconcile_paused: false,
                    claim_airdrops_paused: false,
                    redeem_rewards_paused: false,
                },
            )
            .unwrap();

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.last_reinvest_time = env.block.time.minus_seconds(3);
                    Ok(state)
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ReinvestInCooldown {}));
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.last_reinvest_time, env.block.time.minus_seconds(3));

        /*
           Test - 1. Successful run
        */
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        deps.querier.update_balance(
            config.reward_contract.clone(),
            vec![Coin::new(1000_u128, "uluna".to_string())],
        );
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    state.total_staked = Uint128::new(3000_u128);
                    Ok(state)
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: config.reward_contract.to_string(),
                    msg: to_binary(&RewardExecuteMsg::Transfer {
                        reward_amount: Uint128::new(990_u128),
                        reward_withdraw_contract: env.contract.address,
                        protocol_fee_amount: Uint128::new(10_u128),
                        protocol_fee_contract: config.protocol_fee_contract
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(990_u128, "uluna".to_string())
                })
            ]
        ));
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.last_reinvest_time, env.block.time);
        assert_eq!(state.total_staked, Uint128::new(3990_u128));
        assert_eq!(
            state.exchange_rate,
            Decimal::from_ratio(3990_u128, 3000_u128)
        );
        assert_eq!(state.last_reinvest_time, env.block.time);
        let val1_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid1).unwrap();
        assert_eq!(val1_meta.staked, Uint128::new(1990_u128));
    }

    #[test]
    fn test_claim_airdrops_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        /*
           Test - 1. Airdrop not registered
        */
        CONFIG
            .update(
                deps.as_mut().storage,
                |mut config| -> Result<_, ContractError> {
                    config.airdrop_withdrawal_contract =
                        Addr::unchecked("airdrop_withdrawal_contract");
                    config.airdrop_registry_contract = Addr::unchecked("airdrop_registry_contract");
                    Ok(config)
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                rates: vec![
                    AirdropRate {
                        denom: "unreg_token".to_string(), // special name to test for an unregistered token
                        amount: Uint128::new(1000_u128),
                        stage: 0,
                        proof: vec![
                            "unreg_token_proof1".to_string(),
                            "unreg_token_proof2".to_string(),
                        ],
                    },
                    AirdropRate {
                        denom: "anc".to_string(),
                        amount: Uint128::new(1000_u128),
                        stage: 0,
                        proof: vec!["anc_proof1".to_string(), "anc_proof2".to_string()],
                    },
                ],
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::AirdropNotRegistered(String { .. })
        ));
    }

    #[test]
    fn test_claim_airdrops_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        /*
           Test - 1. Success
        */
        CONFIG
            .update(
                deps.as_mut().storage,
                |mut config| -> Result<_, ContractError> {
                    config.airdrop_withdrawal_contract =
                        Addr::unchecked("airdrop_withdrawal_contract");
                    config.airdrop_registry_contract = Addr::unchecked("airdrop_registry_contract");
                    Ok(config)
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ClaimAirdrops {
                rates: vec![
                    AirdropRate {
                        denom: "anc".to_string(),
                        amount: Uint128::new(1000_u128),
                        stage: 0,
                        proof: vec!["anc_proof1".to_string(), "anc_proof2".to_string()],
                    },
                    AirdropRate {
                        denom: "mir".to_string(),
                        amount: Uint128::new(2000_u128),
                        stage: 0,
                        proof: vec!["mir_proof1".to_string(), "mir_proof2".to_string()],
                    },
                ],
            },
        )
        .unwrap();
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(res.messages.len(), 4);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: "anc_airdrop_contract".to_string(),
                    msg: to_binary(&MerkleAirdropMsg::Claim {
                        stage: 0,
                        amount: Uint128::new(1000_u128),
                        proof: vec!["anc_proof1".to_string(), "anc_proof2".to_string()],
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: "mir_airdrop_contract".to_string(),
                    msg: to_binary(&MerkleAirdropMsg::Claim {
                        stage: 0,
                        amount: Uint128::new(2000_u128),
                        proof: vec!["mir_proof1".to_string(), "mir_proof2".to_string()],
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: "anc_cw20_contract".to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: config.airdrop_withdrawal_contract.to_string(),
                        amount: Uint128::new(1000_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: "mir_cw20_contract".to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: config.airdrop_withdrawal_contract.to_string(),
                        amount: Uint128::new(2000_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                })
            ]
        ));
    }

    #[test]
    fn test_compute_withdrawable_funds_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        let user1 = Addr::unchecked("user1");

        /*
           Test - 1. Undelegation batch not found
        */
        let err = compute_withdrawable_funds(deps.as_mut().storage, 0, &user1).unwrap_err();
        assert!(matches!(err, ContractError::UndelegationBatchNotFound {}));

        /*
           Test - 2. Batch not reconciled
        */
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                1,
                &BatchUndelegationRecord {
                    undelegated_tokens: Uint128::new(10000_u128),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: false,
                    undelegation_er: Default::default(),
                    undelegated_stake: Default::default(),
                    unbonding_slashing_ratio: Default::default(),
                },
            )
            .unwrap();
        let err = compute_withdrawable_funds(deps.as_mut().storage, 1, &user1).unwrap_err();
        assert!(matches!(
            err,
            ContractError::UndelegationBatchNotReconciled {}
        ));

        /*
            Test - 3. User undelegation record not found
        */
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                1,
                &BatchUndelegationRecord {
                    undelegated_tokens: Uint128::new(10000_u128),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: true,
                    undelegation_er: Default::default(),
                    undelegated_stake: Default::default(),
                    unbonding_slashing_ratio: Default::default(),
                },
            )
            .unwrap();
        let err = compute_withdrawable_funds(deps.as_mut().storage, 1, &user1).unwrap_err();
        assert!(matches!(err, ContractError::UndelegationEntryNotFound {}));
    }

    #[test]
    fn test_compute_withdrawable_funds_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        let user1 = Addr::unchecked("user1");

        /*
           Test - 1. Success
        */
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                1,
                &BatchUndelegationRecord {
                    undelegated_tokens: Uint128::new(10000_u128),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: true,
                    undelegation_er: Decimal::one(),
                    undelegated_stake: Uint128::new(10000_u128),
                    unbonding_slashing_ratio: Decimal::from_ratio(3_u128, 4_u128),
                },
            )
            .unwrap();
        USERS
            .save(
                deps.as_mut().storage,
                (&user1, 1),
                &UndelegationInfo {
                    batch_id: 1,
                    token_amount: Uint128::new(1000_u128),
                },
            )
            .unwrap();
        let res = compute_withdrawable_funds(deps.as_mut().storage, 1, &user1).unwrap();
        assert_eq!(
            res,
            GetFundsClaimRecord {
                user_withdrawal_amount: Uint128::new(743_u128),
                protocol_fee: Uint128::new(7_u128),
                undelegated_tokens: Uint128::new(1000_u128)
            }
        );
    }

    #[test]
    fn test_withdraw_funds_to_wallet() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        let user1 = Addr::unchecked("user1");

        /*
           withdraw paused
        */
        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: false,
                    queue_undelegate_paused: false,
                    undelegate_paused: false,
                    withdraw_paused: true,
                    reinvest_paused: false,
                    reconcile_paused: false,
                    claim_airdrops_paused: false,
                    redeem_rewards_paused: false,
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::WithdrawFundsToWallet { batch_id: 0 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::OperationPaused(String { .. })));

        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: false,
                    queue_undelegate_paused: false,
                    undelegate_paused: false,
                    withdraw_paused: false,
                    reinvest_paused: false,
                    reconcile_paused: false,
                    claim_airdrops_paused: false,
                    redeem_rewards_paused: false,
                },
            )
            .unwrap();

        STATE
            .update(deps.as_mut().storage, |mut state| -> StdResult<_> {
                state.reconciled_funds_to_withdraw = Uint128::new(1200);
                Ok(state)
            })
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                1,
                &BatchUndelegationRecord {
                    undelegated_tokens: Uint128::new(10000_u128),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: true,
                    undelegation_er: Decimal::one(),
                    undelegated_stake: Uint128::new(10000_u128),
                    unbonding_slashing_ratio: Decimal::from_ratio(3_u128, 4_u128),
                },
            )
            .unwrap();
        USERS
            .save(
                deps.as_mut().storage,
                (&user1, 1),
                &UndelegationInfo {
                    batch_id: 1,
                    token_amount: Uint128::new(1000_u128),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("user1", &[]),
            ExecuteMsg::WithdrawFundsToWallet { batch_id: 1 },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages[0],
            SubMsg::new(BankMsg::Send {
                to_address: user1.to_string(),
                amount: vec![Coin::new(743_u128, "uluna".to_string())]
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(BankMsg::Send {
                to_address: "protocol_fee_contract".to_string(),
                amount: vec![Coin::new(7_u128, "uluna".to_string())]
            })
        );
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.reconciled_funds_to_withdraw, Uint128::new(450));
        let user_undel_info = USERS.may_load(deps.as_mut().storage, (&user1, 1)).unwrap();
        assert_eq!(user_undel_info, None);
    }

    #[test]
    fn test_undelegate_stake_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    Ok(state)
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);

        /*
           Paused
        */
        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: false,
                    queue_undelegate_paused: false,
                    undelegate_paused: true,
                    withdraw_paused: false,
                    reinvest_paused: false,
                    reconcile_paused: false,
                    claim_airdrops_paused: false,
                    redeem_rewards_paused: false,
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::Undelegate {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::OperationPaused(String { .. })));

        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: false,
                    queue_undelegate_paused: false,
                    undelegate_paused: false,
                    withdraw_paused: false,
                    reinvest_paused: false,
                    reconcile_paused: false,
                    claim_airdrops_paused: false,
                    redeem_rewards_paused: false,
                },
            )
            .unwrap();

        /*
           Test - 1. Undelegation in cooldown
        */
        CONFIG
            .update(
                deps.as_mut().storage,
                |mut config| -> Result<_, ContractError> {
                    config.undelegation_cooldown = 1000;
                    Ok(config)
                },
            )
            .unwrap();
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.last_undelegation_time = env.block.time.minus_seconds(100);
                    Ok(state)
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::Undelegate {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UndelegationInCooldown {}));

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        /*
            Test - 2. No-Op
        */
        CONFIG
            .update(
                deps.as_mut().storage,
                |mut config| -> Result<_, ContractError> {
                    config.undelegation_cooldown = 1000;
                    Ok(config)
                },
            )
            .unwrap();
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.last_undelegation_time = env.block.time.minus_seconds(2000);
                    Ok(state)
                },
            )
            .unwrap();
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    state.current_undelegation_batch_id = 1;
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                1,
                &BatchUndelegationRecord {
                    undelegated_tokens: Uint128::zero(),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: false,
                    undelegation_er: Default::default(),
                    undelegated_stake: Default::default(),
                    unbonding_slashing_ratio: Default::default(),
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::Undelegate {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoOp {}));

        /*
            Test - 3. Validators do not have sufficient funds
        */
        CONFIG
            .update(
                deps.as_mut().storage,
                |mut config| -> Result<_, ContractError> {
                    config.undelegation_cooldown = 1000;
                    Ok(config)
                },
            )
            .unwrap();
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.last_undelegation_time = env.block.time.minus_seconds(2000);
                    Ok(state)
                },
            )
            .unwrap();
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    state.current_undelegation_batch_id = 1;
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                1,
                &BatchUndelegationRecord {
                    undelegated_tokens: Uint128::new(4000_u128),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: false,
                    undelegation_er: Default::default(),
                    undelegated_stake: Default::default(),
                    unbonding_slashing_ratio: Default::default(),
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::Undelegate {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InSufficientFunds {}));
    }

    #[test]
    fn test_undelegate_stake_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        /*
           Test - 1. Successful run
        */
        CONFIG
            .update(
                deps.as_mut().storage,
                |mut config| -> Result<_, ContractError> {
                    config.undelegation_cooldown = 1000;
                    Ok(config)
                },
            )
            .unwrap();
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.last_undelegation_time = env.block.time.minus_seconds(2000);
                    Ok(state)
                },
            )
            .unwrap();
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.validators = vec![valid1.clone(), valid2.clone(), valid3.clone()];
                    state.current_undelegation_batch_id = 1;
                    state.total_staked = Uint128::new(3000_u128);
                    Ok(state)
                },
            )
            .unwrap();
        deps.querier
            .update_staking("uluna", &*get_validators(), &*get_delegations());
        deps.querier
            .update_stader_balances(Some(Uint128::new(3000_u128)), None);
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid3,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1000_u128),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                1,
                &BatchUndelegationRecord {
                    undelegated_tokens: Uint128::new(2000_u128),
                    create_time: env.block.time.minus_seconds(10000),
                    est_release_time: None,
                    reconciled: false,
                    undelegation_er: Default::default(),
                    undelegated_stake: Default::default(),
                    unbonding_slashing_ratio: Default::default(),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::Undelegate {},
        )
        .unwrap();
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(res.messages.len(), 3);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(StakingMsg::Undelegate {
                    validator: valid3.to_string(),
                    amount: Coin::new(1000_u128, "uluna".to_string())
                }),
                SubMsg::new(StakingMsg::Undelegate {
                    validator: valid2.to_string(),
                    amount: Coin::new(1000_u128, "uluna".to_string())
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: config.cw20_token_contract.to_string(),
                    msg: to_binary(&cw20::Cw20ExecuteMsg::Burn {
                        amount: Uint128::new(2000_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                })
            ]
        ));
        let val3_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid3).unwrap();
        let val2_meta = VALIDATOR_META.load(deps.as_mut().storage, &valid2).unwrap();
        assert_eq!(
            val3_meta,
            VMeta {
                staked: Uint128::zero(),
                slashed: Default::default(),
                filled: Default::default()
            }
        );
        assert_eq!(
            val2_meta,
            VMeta {
                staked: Uint128::zero(),
                slashed: Default::default(),
                filled: Default::default()
            }
        );
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        let undel_batch = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, 1)
            .unwrap();
        assert_eq!(
            undel_batch,
            BatchUndelegationRecord {
                undelegated_tokens: Uint128::new(2000_u128),
                create_time: env.block.time.minus_seconds(10000),
                est_release_time: Some(env.block.time.plus_seconds(config.unbonding_period)),
                reconciled: false,
                undelegation_er: Decimal::one(),
                undelegated_stake: Uint128::new(2000_u128),
                unbonding_slashing_ratio: Default::default()
            }
        );
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.total_staked, Uint128::new(1000_u128));
        assert_eq!(state.last_undelegation_time, env.block.time);
        let new_undel_batch = BATCH_UNDELEGATION_REGISTRY
            .may_load(deps.as_mut().storage, 2)
            .unwrap();
        assert_ne!(new_undel_batch, None);
    }

    #[test]
    fn test_reconcile_funds() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let _res = instantiate_contract(&mut deps, &info, &env);

        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: false,
                    queue_undelegate_paused: false,
                    undelegate_paused: false,
                    withdraw_paused: false,
                    reinvest_paused: false,
                    reconcile_paused: true,
                    claim_airdrops_paused: false,
                    redeem_rewards_paused: false,
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::ReconcileFunds {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::OperationPaused(String { .. })));

        OPERATION_CONTROLS
            .save(
                deps.as_mut().storage,
                &OperationControls {
                    deposit_paused: false,
                    queue_undelegate_paused: false,
                    undelegate_paused: false,
                    withdraw_paused: false,
                    reinvest_paused: false,
                    reconcile_paused: false,
                    claim_airdrops_paused: false,
                    redeem_rewards_paused: false,
                },
            )
            .unwrap();

        /*
           Test - 1. No undelegation slashing
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.current_undelegation_batch_id = 3;
                    state.last_reconciled_batch_id = 1;
                    state.reconciled_funds_to_withdraw = Uint128::new(1800);
                    Ok(state)
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                2,
                &BatchUndelegationRecord {
                    undelegated_tokens: Uint128::new(3000_u128),
                    create_time: env.block.time.minus_seconds(20000),
                    est_release_time: Some(env.block.time.minus_seconds(300)),
                    reconciled: false,
                    undelegation_er: Decimal::one(),
                    undelegated_stake: Uint128::new(3000_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                3,
                &BatchUndelegationRecord {
                    undelegated_tokens: Uint128::new(2000_u128),
                    create_time: env.block.time.minus_seconds(10000),
                    est_release_time: Some(env.block.time.minus_seconds(100)),
                    reconciled: false,
                    undelegation_er: Decimal::one(),
                    undelegated_stake: Uint128::new(2000_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();
        deps.querier.update_balance(
            env.contract.address.clone(),
            vec![Coin::new(7000_u128, "uluna".to_string())],
        );
        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::ReconcileFunds {},
        )
        .unwrap();
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.reconciled_funds_to_withdraw, Uint128::new(6800));
        let batch_2 = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, 2)
            .unwrap();
        let batch_3 = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, 3)
            .unwrap();
        assert_eq!(
            batch_2,
            BatchUndelegationRecord {
                undelegated_tokens: Uint128::new(3000_u128),
                create_time: env.block.time.minus_seconds(20000),
                est_release_time: Some(env.block.time.minus_seconds(300)),
                reconciled: true,
                undelegation_er: Decimal::one(),
                undelegated_stake: Uint128::new(3000_u128),
                unbonding_slashing_ratio: Decimal::one()
            }
        );
        assert_eq!(
            batch_3,
            BatchUndelegationRecord {
                undelegated_tokens: Uint128::new(2000_u128),
                create_time: env.block.time.minus_seconds(10000),
                est_release_time: Some(env.block.time.minus_seconds(100)),
                reconciled: true,
                undelegation_er: Decimal::one(),
                undelegated_stake: Uint128::new(2000_u128),
                unbonding_slashing_ratio: Decimal::one()
            }
        );

        /*
           Test - 2. Some undelegation slashing
        */
        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.current_undelegation_batch_id = 3;
                    state.last_reconciled_batch_id = 1;
                    state.reconciled_funds_to_withdraw = Uint128::new(1800);
                    Ok(state)
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                2,
                &BatchUndelegationRecord {
                    undelegated_tokens: Uint128::new(3000_u128),
                    create_time: env.block.time.minus_seconds(20000),
                    est_release_time: Some(env.block.time.minus_seconds(300)),
                    reconciled: false,
                    undelegation_er: Decimal::one(),
                    undelegated_stake: Uint128::new(3000_u128),
                    unbonding_slashing_ratio: Default::default(),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                3,
                &BatchUndelegationRecord {
                    undelegated_tokens: Uint128::new(2000_u128),
                    create_time: env.block.time.minus_seconds(10000),
                    est_release_time: Some(env.block.time.minus_seconds(100)),
                    reconciled: false,
                    undelegation_er: Decimal::one(),
                    undelegated_stake: Uint128::new(2000_u128),
                    unbonding_slashing_ratio: Default::default(),
                },
            )
            .unwrap();
        deps.querier.update_balance(
            env.contract.address.clone(),
            vec![Coin::new(6000_u128, "uluna".to_string())],
        );
        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::ReconcileFunds {},
        )
        .unwrap();
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.reconciled_funds_to_withdraw, Uint128::new(6000));
        let batch_2 = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, 2)
            .unwrap();
        let batch_3 = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, 3)
            .unwrap();
        assert_eq!(
            batch_2,
            BatchUndelegationRecord {
                undelegated_tokens: Uint128::new(3000_u128),
                create_time: env.block.time.minus_seconds(20000),
                est_release_time: Some(env.block.time.minus_seconds(300)),
                reconciled: true,
                undelegation_er: Decimal::one(),
                undelegated_stake: Uint128::new(3000_u128),
                unbonding_slashing_ratio: Decimal::from_ratio(42_u128, 50_u128)
            }
        );
        assert_eq!(
            batch_3,
            BatchUndelegationRecord {
                undelegated_tokens: Uint128::new(2000_u128),
                create_time: env.block.time.minus_seconds(10000),
                est_release_time: Some(env.block.time.minus_seconds(100)),
                reconciled: true,
                undelegation_er: Decimal::one(),
                undelegated_stake: Uint128::new(2000_u128),
                unbonding_slashing_ratio: Decimal::from_ratio(42_u128, 50_u128)
            }
        );
    }
}
