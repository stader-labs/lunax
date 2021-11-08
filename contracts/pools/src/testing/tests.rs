#[cfg(test)]
mod tests {
    // use crate::contract::{execute, instantiate, query, reply, MESSAGE_REPLY_SWAP_ID};
    use crate::contract::{check_slashing, execute, instantiate, query};
    use crate::error::ContractError;
    use crate::msg::{
        ExecuteMsg, InstantiateMsg, MerkleAirdropMsg, QueryConfigResponse, QueryMsg,
        QueryStateResponse,
    };
    use crate::state::{
        AirdropRate, AirdropRegistryInfo, AirdropTransferRequest, BatchUndelegationRecord, Config,
        ConfigUpdateRequest, PoolConfigUpdateRequest, PoolRegistryInfo, State, VMeta,
        AIRDROP_REGISTRY, BATCH_UNDELEGATION_REGISTRY, CONFIG, POOL_REGISTRY, STATE,
        VALIDATOR_META,
    };
    use crate::testing::mock_querier;
    use crate::testing::mock_querier::mock_dependencies_for_validator_querier;
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
        MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{
        attr, from_binary, to_binary, Addr, Coin, Decimal, Env, FullDelegation, MessageInfo,
        OwnedDeps, Response, SubMsg, Uint128, Validator, WasmMsg,
    };
    use cw_storage_plus::U64Key;
    use delegator::msg::ExecuteMsg as DelegatorMsg;
    use reward::msg::ExecuteMsg as RewardExecuteMsg;
    use stader_utils::coin_utils::{check_equal_deccoin_vector, DecCoin};
    use terra_cosmwasm::TerraMsgWrapper;
    use validator::msg::ExecuteMsg as ValidatorMsg;

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
                amount: Coin::new(0, "uluna"),
                can_redelegate: Coin::new(0, "uluna"),
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
            delegator_contract: Addr::unchecked("delegator_addr").to_string(),
            scc_contract: Addr::unchecked("scc_addr").to_string(),
            unbonding_period: None,
            unbonding_buffer: None,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            delegator_contract: Addr::unchecked("delegator_addr").to_string(),
            scc_contract: Addr::unchecked("scc_addr").to_string(),
            unbonding_period: None,
            unbonding_buffer: None,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
        };
        let expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "uluna".to_string(),
            delegator_contract: Addr::unchecked("delegator_addr"),
            scc_contract: Addr::unchecked("scc_addr"),
            unbonding_period: 3600 * 24 * 21,
            unbonding_buffer: 3600,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
        };
        let info = mock_info("creator", &[]);

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let value: QueryConfigResponse = from_binary(&res).unwrap();
        assert_eq!(value.config, expected_config);

        let res = query(deps.as_ref(), mock_env(), QueryMsg::State {}).unwrap();
        let value: QueryStateResponse = from_binary(&res).unwrap();
        assert_eq!(value.state, State { next_pool_id: 0 });
    }

    #[test]
    fn test_add_pool() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        instantiate_contract(&mut deps, &info, &env, None);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::AddPool {
                name: "Community Validator".to_string(),
                validator_contract: "pool0_val_addr".to_string(),
                reward_contract: "pool0_rew_addr".to_string(),
                protocol_fee_percent: Decimal::from_ratio(1_u128, 1000_u128),
                protocol_fee_contract: "protocol_fee_addr".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        assert!(POOL_REGISTRY
            .may_load(deps.as_mut().storage, U64Key::new(0))
            .unwrap()
            .is_none());
        assert!(BATCH_UNDELEGATION_REGISTRY
            .may_load(deps.as_mut().storage, (U64Key::new(0), U64Key::new(1)))
            .unwrap()
            .is_none());

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddPool {
                name: "Community Validator".to_string(),
                validator_contract: "pool0_val_addr".to_string(),
                reward_contract: "pool0_rew_addr".to_string(),
                protocol_fee_percent: Decimal::from_ratio(1_u128, 1000_u128),
                protocol_fee_contract: "protocol_fee_addr".to_string(),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool0_val_addr".to_string(),
                msg: to_binary(&ValidatorMsg::SetRewardWithdrawAddress {
                    reward_contract: Addr::unchecked("pool0_rew_addr")
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.next_pool_id, 1_u64);

        let pool_meta = POOL_REGISTRY
            .may_load(deps.as_mut().storage, U64Key::new(0))
            .unwrap()
            .unwrap();
        let expected_pool_info = PoolRegistryInfo {
            name: "Community Validator".to_string(),
            validator_contract: Addr::unchecked("pool0_val_addr"),
            reward_contract: Addr::unchecked("pool0_rew_addr"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
            active: true,
            validators: vec![],
            staked: Default::default(),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            slashing_pointer: Decimal::one(),
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
            protocol_fee_percent: Decimal::from_ratio(1_u128, 1000_u128),
        };
        assert_eq!(pool_meta, expected_pool_info);

        let batch_undelegation_info = BATCH_UNDELEGATION_REGISTRY
            .may_load(deps.as_mut().storage, (U64Key::new(0), U64Key::new(1)))
            .unwrap()
            .unwrap();
        let expected_batch_undelegation = BatchUndelegationRecord {
            prorated_amount: Default::default(),
            undelegated_amount: Uint128::zero(),
            create_time: env.block.time,
            est_release_time: None,
            reconciled: false,
            last_updated_slashing_pointer: Decimal::one(),
            unbonding_slashing_ratio: Decimal::one(),
        };
        assert!(batch_undelegation_info.eq(&expected_batch_undelegation));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(100, "uluna")]),
            ExecuteMsg::AddPool {
                name: "Community Validator".to_string(),
                validator_contract: "pool0_val_addr".to_string(),
                reward_contract: "pool0_rew_addr".to_string(),
                protocol_fee_percent: Decimal::from_ratio(1_u128, 1000_u128),
                protocol_fee_contract: "protocol_fee_addr".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorContractInUse {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(100, "uluna")]),
            ExecuteMsg::AddPool {
                name: "Community Validator".to_string(),
                validator_contract: "new_pool0_val_addr".to_string(),
                reward_contract: "pool0_rew_addr".to_string(),
                protocol_fee_percent: Decimal::from_ratio(1_u128, 1000_u128),
                protocol_fee_contract: "protocol_fee_addr".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::RewardContractInUse {}));
    }

    #[test]
    fn test_add_validator_to_pool() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");

        instantiate_contract(&mut deps, &info, &env, None);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::AddValidator {
                val_addr: valid1.clone(),
                pool_id: 12,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                val_addr: valid2.clone(),
                pool_id: 12,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::PoolNotFound {}));

        assert!(VALIDATOR_META
            .may_load(deps.as_mut().storage, (valid2.clone(), U64Key::new(12)))
            .unwrap()
            .is_none());

        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &PoolRegistryInfo {
                    name: "Random name".to_string(),
                    active: false,
                    validator_contract: Addr::unchecked("pool_12_validator_addr"),
                    reward_contract: Addr::unchecked("reward_12_validator_addr"),
                    protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
                    protocol_fee_percent: Default::default(),
                    validators: vec![],
                    staked: Default::default(),
                    rewards_pointer: Default::default(),
                    airdrops_pointer: vec![],
                    slashing_pointer: Decimal::one(),
                    current_undelegation_batch_id: 0,
                    last_reconciled_batch_id: 0,
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                val_addr: valid2.clone(),
                pool_id: 12,
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::AddValidator {
                    val_addr: valid2.clone(),
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let vmeta = VALIDATOR_META
            .may_load(deps.as_mut().storage, (valid2.clone(), U64Key::new(12)))
            .unwrap();
        assert!(vmeta.is_some());
        assert_eq!(
            vmeta.unwrap(),
            VMeta {
                staked: Default::default(),
                slashed: Default::default(),
                filled: Default::default()
            }
        );

        let pool_meta = POOL_REGISTRY
            .may_load(deps.as_mut().storage, U64Key::new(12))
            .unwrap()
            .unwrap();
        let expected_pool_info = PoolRegistryInfo {
            name: "Random name".to_string(),
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("reward_12_validator_addr"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
            active: false,
            validators: vec![valid2.clone()],
            staked: Default::default(),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            slashing_pointer: Decimal::one(),
            current_undelegation_batch_id: 0,
            last_reconciled_batch_id: 0,
            protocol_fee_percent: Default::default(),
        };
        assert!(pool_meta.eq(&expected_pool_info));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                val_addr: valid2.clone(),
                pool_id: 12,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorAssociatedToPool {}));
    }

    #[test]
    fn test_remove_validator() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");

        instantiate_contract(&mut deps, &info, &env, None);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid1.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid2.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::PoolNotFound {}));

        let mut pool_info = PoolRegistryInfo {
            name: "Random name".to_string(),
            active: false,
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("reward_12_validator_addr"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
            protocol_fee_percent: Default::default(),
            validators: vec![],
            staked: Uint128::new(2000),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            slashing_pointer: Decimal::one(),
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
        };
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &pool_info)
            .unwrap();

        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(1)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(100_u128, 1_u128),
                    undelegated_amount: Uint128::new(0),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: false,
                    last_updated_slashing_pointer: Default::default(),
                    unbonding_slashing_ratio: Default::default(),
                },
            )
            .unwrap();

        let all_delegations = vec![
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"),
                validator: "valid0001".to_string(),
                amount: Coin::new(1000, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"),
                validator: "valid0002".to_string(),
                amount: Coin::new(800, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
        ];

        deps.querier
            .update_staking("test", &*get_validators(), &*all_delegations);

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid1.clone(),
                pool_id: 12,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorsCannotBeSame {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        pool_info.validators.push(valid1.clone());
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &pool_info)
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        // RESET FOR HAPPY PATH ====================================================================
        let pool_info = PoolRegistryInfo {
            name: "Random name".to_string(),
            active: false,
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("reward_12_validator_addr"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
            protocol_fee_percent: Default::default(),
            validators: vec![valid1.clone(), valid2.clone()],
            staked: Uint128::new(2000),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            slashing_pointer: Decimal::one(),
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
        };
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &pool_info)
            .unwrap();

        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(1)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(100_u128, 1_u128),
                    undelegated_amount: Uint128::new(0),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::one(),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();

        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &pool_info)
            .unwrap();

        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid1.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid2.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
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
                pool_id: 12,
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::RemoveValidator {
                    val_addr: valid1.clone(),
                    redelegate_addr: valid2.clone()
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let pool_meta = POOL_REGISTRY
            .may_load(deps.as_mut().storage, U64Key::new(12))
            .unwrap()
            .unwrap();
        let expected_pool_info = PoolRegistryInfo {
            name: "Random name".to_string(),
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("reward_12_validator_addr"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
            active: false,
            validators: vec![valid2.clone()],
            staked: Uint128::new(1800),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            slashing_pointer: Decimal::from_ratio(9_u128, 10_u128),
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
            protocol_fee_percent: Default::default(),
        };
        assert_eq!(pool_meta, expected_pool_info);

        let val2_meta = VALIDATOR_META
            .load(deps.as_mut().storage, (valid2.clone(), U64Key::new(12)))
            .unwrap();
        assert_eq!(
            val2_meta,
            VMeta {
                staked: Uint128::new(1800),
                slashed: Uint128::new(200),
                filled: Default::default()
            }
        );

        assert!(VALIDATOR_META
            .may_load(deps.as_mut().storage, (valid1.clone(), U64Key::new(12)))
            .unwrap()
            .is_none());
        let batch = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(1)))
            .unwrap();
        assert_eq!(
            batch.last_updated_slashing_pointer,
            Decimal::from_ratio(9_u128, 10_u128)
        );
        assert_eq!(batch.undelegated_amount, Uint128::new(0));
        assert_eq!(batch.prorated_amount, Decimal::from_ratio(90_u128, 1_u128));
    }

    #[test]
    fn test_rebalance_pool() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");

        instantiate_contract(&mut deps, &info, &env, None);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid1.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid2.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::PoolNotFound {}));

        let mut pool_info = PoolRegistryInfo {
            name: "Random name".to_string(),
            active: false,
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("reward_12_validator_addr"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
            protocol_fee_percent: Default::default(),
            validators: vec![],
            staked: Uint128::new(2000),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            slashing_pointer: Decimal::one(),
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
        };
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &pool_info)
            .unwrap();

        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(1)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(100_u128, 1_u128),
                    undelegated_amount: Uint128::new(100),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: false,
                    last_updated_slashing_pointer: Default::default(),
                    unbonding_slashing_ratio: Default::default(),
                },
            )
            .unwrap();

        let all_delegations = vec![
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"),
                validator: "valid0001".to_string(),
                amount: Coin::new(1000, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"),
                validator: "valid0002".to_string(),
                amount: Coin::new(800, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
        ];

        deps.querier
            .update_staking("test", &*get_validators(), &*all_delegations);

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RebalancePool {
                val_addr: valid1.clone(),
                redel_addr: valid1.clone(),
                pool_id: 12,
                amount: Uint128::new(300),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorsCannotBeSame {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RebalancePool {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12,
                amount: Uint128::new(300),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        pool_info.validators.push(valid1.clone());
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &pool_info)
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RebalancePool {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12,
                amount: Uint128::new(300),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        // RESET FOR HAPPY PATH ====================================================================
        let pool_info = PoolRegistryInfo {
            name: "Random name".to_string(),
            active: false,
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("reward_12_validator_addr"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
            protocol_fee_percent: Default::default(),
            validators: vec![valid1.clone(), valid2.clone()],
            staked: Uint128::new(2000),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            slashing_pointer: Decimal::one(),
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
        };
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &pool_info)
            .unwrap();

        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(1)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(100_u128, 1_u128),
                    undelegated_amount: Uint128::new(0),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::one(),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();

        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &pool_info)
            .unwrap();

        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid1.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid2.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RebalancePool {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12,
                amount: Uint128::new(300),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::Redelegate {
                    src: valid1.clone(),
                    dst: valid2.clone(),
                    amount: Uint128::new(300)
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let pool_meta = POOL_REGISTRY
            .may_load(deps.as_mut().storage, U64Key::new(12))
            .unwrap()
            .unwrap();
        let expected_pool_info = PoolRegistryInfo {
            name: "Random name".to_string(),
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("reward_12_validator_addr"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
            active: false,
            validators: vec![valid1.clone(), valid2.clone()],
            staked: Uint128::new(1800),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            slashing_pointer: Decimal::from_ratio(9_u128, 10_u128),
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
            protocol_fee_percent: Default::default(),
        };
        assert_eq!(pool_meta, expected_pool_info);

        let val2_meta = VALIDATOR_META
            .load(deps.as_mut().storage, (valid2.clone(), U64Key::new(12)))
            .unwrap();
        assert_eq!(
            val2_meta,
            VMeta {
                staked: Uint128::new(1100),
                slashed: Uint128::new(200),
                filled: Default::default()
            }
        );
        let val1_meta = VALIDATOR_META
            .load(deps.as_mut().storage, (valid1.clone(), U64Key::new(12)))
            .unwrap();
        assert_eq!(
            val1_meta,
            VMeta {
                staked: Uint128::new(700),
                slashed: Uint128::new(0),
                filled: Default::default()
            }
        );
        let batch = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(1)))
            .unwrap();
        assert_eq!(
            batch.last_updated_slashing_pointer,
            Decimal::from_ratio(9_u128, 10_u128)
        );
        assert_eq!(batch.prorated_amount, Decimal::from_ratio(90_u128, 1_u128));
        assert_eq!(
            batch.last_updated_slashing_pointer,
            Decimal::from_ratio(9_u128, 10_u128)
        );
        assert_eq!(batch.undelegated_amount, Uint128::new(0));
    }

    #[test]
    fn test_deposit_to_pool() {
        let user1 = Addr::unchecked("user0001");
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        let valid4 = Addr::unchecked("valid0004");
        let mut deps = mock_dependencies(&[]);
        let info = mock_info(&user1.clone().to_string(), &[]);
        let env = mock_env();

        instantiate_contract(&mut deps, &info, &env, None);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[]),
            ExecuteMsg::Deposit { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoFunds {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &user1.clone().to_string(),
                &[Coin::new(1_000_000_000_001, "uluna")],
            ),
            ExecuteMsg::Deposit { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::MaxDeposit {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(1, "uluna")]),
            ExecuteMsg::Deposit { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::MinDeposit {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(3000, "uluna")]),
            ExecuteMsg::Deposit { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::PoolNotFound {}));

        let mut pool_info = PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: false,
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("reward_12_validator_addr"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
            protocol_fee_percent: Default::default(),
            validators: vec![
                valid1.clone(),
                valid2.clone(),
                valid3.clone(),
                valid4.clone(),
                Addr::unchecked("valid0005").clone(),
            ],
            staked: Uint128::new(450),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![],
            slashing_pointer: Decimal::one(),
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
        };
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &pool_info)
            .unwrap();

        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(1)),
                &BatchUndelegationRecord {
                    prorated_amount: Default::default(),
                    undelegated_amount: Default::default(),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: false,
                    last_updated_slashing_pointer: Default::default(),
                    unbonding_slashing_ratio: Default::default(),
                },
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(3000, "uluna")]),
            ExecuteMsg::Deposit { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::PoolInactive {}));

        pool_info.active = true;
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &pool_info)
            .unwrap();

        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid1.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(100),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid2.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(100),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid3.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid4.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (Addr::unchecked("valid0005").clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();

        /* Validator 5 is not discoverable - essentially jailed. So don't deposit in any case */
        // CASE 1 - Val 1,2,3 are discoverable. Val 4 does not have a delegation. So val4 should get picked.

        let all_delegations = vec![
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0001".to_string(),
                amount: Coin::new(200, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0002".to_string(),
                amount: Coin::new(100, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0003".to_string(),
                amount: Coin::new(150, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
        ];

        let mut validators_discoverable = get_validators();
        validators_discoverable.push(Validator {
            address: "valid0004".to_string(),
            commission: Decimal::zero(),
            max_commission: Decimal::zero(),
            max_change_rate: Decimal::zero(),
        });

        deps.querier
            .update_staking("test", &validators_discoverable, &all_delegations);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(3000, "uluna")]),
            ExecuteMsg::Deposit { pool_id: 12 },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "delegator_addr".to_string(),
                msg: to_binary(&DelegatorMsg::Deposit {
                    user_addr: user1.clone(),
                    amount: Uint128::new(3000),
                    pool_id: 12,
                    pool_rewards_pointer: Decimal::zero(),
                    pool_airdrops_pointer: vec![],
                    pool_slashing_pointer: Decimal::one()
                })
                .unwrap(),
                funds: vec![]
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::Stake {
                    val_addr: valid4.clone(),
                })
                .unwrap(),
                funds: vec![Coin::new(3000, "uluna")]
            })
        );

        let pool_meta = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_meta.staked, Uint128::new(3450));

        // CASE 2 - Val 1,2,3 are discoverable. Val1, 2 each have 1000, val4 has 3000, val3 has 0 delegations. So pick val3

        let mut validators_discoverable = get_validators();
        validators_discoverable.push(Validator {
            address: "valid0004".to_string(),
            commission: Decimal::zero(),
            max_commission: Decimal::zero(),
            max_change_rate: Decimal::zero(),
        });

        let all_delegations = vec![
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0001".to_string(),
                amount: Coin::new(200, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0002".to_string(),
                amount: Coin::new(250, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"),
                validator: valid4.to_string(),
                amount: Coin::new(3000, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
        ];

        deps.querier
            .update_staking("test", &*validators_discoverable, &*all_delegations);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(1000, "uluna")]),
            ExecuteMsg::Deposit { pool_id: 12 },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "delegator_addr".to_string(),
                msg: to_binary(&DelegatorMsg::Deposit {
                    user_addr: user1.clone(),
                    amount: Uint128::new(1000),
                    pool_id: 12,
                    pool_rewards_pointer: Decimal::zero(),
                    pool_airdrops_pointer: vec![],
                    pool_slashing_pointer: Decimal::one()
                })
                .unwrap(),
                funds: vec![]
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::Stake {
                    val_addr: valid3.clone(),
                })
                .unwrap(),
                funds: vec![Coin::new(1000, "uluna")]
            })
        );

        let pool_meta = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_meta.staked, Uint128::new(4450));

        // CASE 3 - Val 1,2,3 are discoverable. Val1,2,3 each have 1000, val4 has 3000, val3 has 0 delegations. So pick val1 (as passed in order by pool vals)
        let mut validators_discoverable = get_validators();
        validators_discoverable.push(Validator {
            address: "valid0004".to_string(),
            commission: Decimal::zero(),
            max_commission: Decimal::zero(),
            max_change_rate: Decimal::zero(),
        });

        let all_delegations = vec![
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0001".to_string(),
                amount: Coin::new(500, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0002".to_string(),
                amount: Coin::new(500, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"),
                validator: valid3.to_string(),
                amount: Coin::new(500, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"),
                validator: valid4.to_string(),
                amount: Coin::new(2950, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
        ];

        deps.querier
            .update_staking("test", &*validators_discoverable, &*all_delegations);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(5000, "uluna")]),
            ExecuteMsg::Deposit { pool_id: 12 },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "delegator_addr".to_string(),
                msg: to_binary(&DelegatorMsg::Deposit {
                    user_addr: user1.clone(),
                    amount: Uint128::new(5000),
                    pool_id: 12,
                    pool_rewards_pointer: Decimal::zero(),
                    pool_airdrops_pointer: vec![],
                    pool_slashing_pointer: Decimal::one()
                })
                .unwrap(),
                funds: vec![]
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::Stake {
                    val_addr: valid1.clone(),
                })
                .unwrap(),
                funds: vec![Coin::new(5000, "uluna")]
            })
        );

        let pool_meta = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_meta.staked, Uint128::new(9450));

        // NOT CHECKING FOR VALIDATOR_META updates
    }

    #[test]
    fn test_redeem_rewards() {
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        instantiate_contract(&mut deps, &info, &env, None);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::RedeemRewards { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &PoolRegistryInfo {
                    name: "RandomName".to_string(),
                    active: true,
                    validator_contract: Addr::unchecked("pool_12_validator_addr"),
                    reward_contract: Addr::unchecked("reward_12_validator_addr"),
                    protocol_fee_contract: Addr::unchecked("pool_12_protocol_addr"),
                    protocol_fee_percent: Default::default(),
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(450),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![],
                    slashing_pointer: Default::default(),
                    current_undelegation_batch_id: 1,
                    last_reconciled_batch_id: 0,
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(1)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(40_u128, 1_u128),
                    undelegated_amount: Uint128::zero(),
                    create_time: env.block.time,
                    est_release_time: None,
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::one(),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemRewards { pool_id: 12 },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::RedeemRewards {
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()]
                })
                .unwrap(),
                funds: vec![]
            })
        );
    }

    #[test]
    fn test_swap() {
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        instantiate_contract(&mut deps, &info, &env, None);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::Swap { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Swap { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::PoolNotFound {}));

        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &PoolRegistryInfo {
                    name: "RandomName".to_string(),
                    active: true,
                    validator_contract: Addr::unchecked("pool_12_validator_addr"),
                    reward_contract: Addr::unchecked("pool_12_reward_addr"),
                    protocol_fee_contract: Addr::unchecked("pool_12_protocol_addr"),
                    protocol_fee_percent: Default::default(),
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(450),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![],
                    slashing_pointer: Default::default(),
                    current_undelegation_batch_id: 1,
                    last_reconciled_batch_id: 0,
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Swap { pool_id: 12 },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_reward_addr".to_string(),
                msg: to_binary(&RewardExecuteMsg::Swap {}).unwrap(),
                funds: vec![]
            },)
        );
    }

    #[test]
    fn test_transfer_rewards_to_scc() {
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());
        instantiate_contract(&mut deps, &info, &env, None);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::SendRewardsToScc { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::SendRewardsToScc { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::PoolNotFound {}));

        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &PoolRegistryInfo {
                    name: "RandomName".to_string(),
                    active: true,
                    validator_contract: Addr::unchecked("pool_12_validator_addr"),
                    reward_contract: Addr::unchecked("pool_12_reward_addr"),
                    protocol_fee_contract: Addr::unchecked("pool_12_protocol_addr"),
                    protocol_fee_percent: Decimal::from_ratio(2_u128, 100_u128), // 2 percent
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(450),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![],
                    slashing_pointer: Default::default(),
                    current_undelegation_batch_id: 1,
                    last_reconciled_batch_id: 0,
                },
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::SendRewardsToScc { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ZeroRewards {}));

        deps.querier.update_balance(
            Addr::unchecked("pool_12_reward_addr"),
            vec![Coin::new(1234, "uluna")],
        );
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::SendRewardsToScc { pool_id: 12 },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_reward_addr".to_string(),
                msg: to_binary(&RewardExecuteMsg::Transfer {
                    reward_amount: Uint128::new(1210),
                    reward_withdraw_contract: Addr::unchecked("scc_addr"),
                    protocol_fee_amount: Uint128::new(24),
                    protocol_fee_contract: Addr::unchecked("pool_12_protocol_addr"),
                })
                .unwrap(),
                funds: vec![]
            },)
        );
    }

    #[test]
    fn test_queue_user_undelegation() {
        let user1 = Addr::unchecked("user0001");
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        let mut deps = mock_dependencies(&[]);
        let info = mock_info(&user1.clone().to_string(), &[]);
        let env = mock_env();

        let all_delegations = vec![
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0001".to_string(),
                amount: Coin::new(150, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0002".to_string(),
                amount: Coin::new(150, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"),
                validator: valid3.to_string(),
                amount: Coin::new(105, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
        ];
        deps.querier
            .update_staking("test", &*get_validators(), &*all_delegations);

        instantiate_contract(&mut deps, &info, &env, None);

        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid1.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(150),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid2.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(150),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid3.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(150),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();

        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &PoolRegistryInfo {
                    name: "RandomName".to_string(),
                    active: false,
                    validator_contract: Addr::unchecked("pool_12_validator_addr"),
                    reward_contract: Addr::unchecked("pool_12_reward_addr"),
                    protocol_fee_contract: Addr::unchecked("pool_12_protocol_addr"),
                    protocol_fee_percent: Decimal::from_ratio(1_u128, 1000_u128),
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(450),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![],
                    slashing_pointer: Decimal::one(),
                    current_undelegation_batch_id: 1,
                    last_reconciled_batch_id: 0,
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(1)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(40_u128, 1_u128),
                    undelegated_amount: Uint128::zero(),
                    create_time: env.block.time,
                    est_release_time: None,
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::one(),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[]),
            ExecuteMsg::QueueUndelegate {
                pool_id: 12,
                amount: Uint128::new(20),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "delegator_addr".to_string(),
                msg: to_binary(&DelegatorMsg::Undelegate {
                    user_addr: user1.clone(),
                    batch_id: 1,
                    from_pool: 12,
                    amount: Uint128::new(20),
                    pool_rewards_pointer: Decimal::zero(),
                    pool_airdrops_pointer: vec![],
                    pool_slashing_pointer: Decimal::one() // Note that updated pool pointer slashing isn't propagated by design
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let pool_meta = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_meta.staked, Uint128::new(405)); // Slashing has reduced this.
        assert_eq!(
            pool_meta.slashing_pointer,
            Decimal::from_ratio(9_u128, 10_u128)
        ); // Slashing has reduced this.

        let batch_und = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(1)))
            .unwrap();
        assert_eq!(
            batch_und.prorated_amount,
            Decimal::from_ratio(54_u128, 1_u128)
        ); // because even the 20 newly deposited gets slashing adjusted to 18
    }

    #[test]
    fn test_undelegate_from_pool() {
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        let valid4 = Addr::unchecked("valid0004");
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let all_delegations = vec![
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0001".to_string(),
                amount: Coin::new(1000, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0002".to_string(),
                amount: Coin::new(1000, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"),
                validator: valid3.to_string(),
                amount: Coin::new(0, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
        ];
        deps.querier
            .update_staking("test", &*get_validators(), &*all_delegations);

        instantiate_contract(&mut deps, &info, &env, None);

        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid1.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid2.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(1000),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid3.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(0),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();

        instantiate_contract(&mut deps, &info, &env, None);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::Undelegate { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        assert!(BATCH_UNDELEGATION_REGISTRY
            .may_load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(2)))
            .unwrap()
            .is_none());

        // Val 4 will never be undelegated from because they are jailed.
        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &PoolRegistryInfo {
                    name: "RandomName".to_string(),
                    active: false,
                    validator_contract: Addr::unchecked("pool_12_validator_addr"),
                    reward_contract: Addr::unchecked("pool_12_reward_addr"),
                    protocol_fee_contract: Addr::unchecked("pool_12_protocol_addr"),
                    protocol_fee_percent: Decimal::from_ratio(1_u128, 1000_u128),
                    validators: vec![
                        valid1.clone(),
                        valid2.clone(),
                        valid3.clone(),
                        valid4.clone(),
                    ],
                    staked: Uint128::new(1450),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![],
                    slashing_pointer: Decimal::one(),
                    current_undelegation_batch_id: 1,
                    last_reconciled_batch_id: 0,
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(1)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::zero(),
                    undelegated_amount: Uint128::new(0),
                    create_time: env.block.time,
                    est_release_time: None,
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::one(),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Undelegate { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoOp {}));

        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(1)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(403_u128, 10_u128),
                    undelegated_amount: Uint128::new(0),
                    create_time: env.block.time,
                    est_release_time: None,
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::one(),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Undelegate { pool_id: 12 },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        // Val1 and 2 have 1000 each and val 3 has 0. Val 2 will be undelegated from owing to the reverse ordering in pool_meta.validators.
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::Undelegate {
                    val_addr: valid2.clone(),
                    amount: Uint128::new(40),
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let pool_meta = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_meta.staked, Uint128::new(1410)); // Staked amount is deducted on actual undelegation
        assert_eq!(pool_meta.current_undelegation_batch_id, 2);
        assert_eq!(pool_meta.last_reconciled_batch_id, 0);

        let batch_und = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(1)))
            .unwrap();
        assert_eq!(
            batch_und.prorated_amount,
            Decimal::from_ratio(403_u128, 10_u128)
        );
        assert_eq!(batch_und.undelegated_amount, Uint128::new(40));
        assert_eq!(
            batch_und.est_release_time.unwrap(),
            env.block.time.plus_seconds(21 * 24 * 3600)
        );
        assert!(!batch_und.reconciled);

        assert!(BATCH_UNDELEGATION_REGISTRY
            .may_load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(2)))
            .unwrap()
            .is_some());

        let all_delegations = vec![
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0001".to_string(),
                amount: Coin::new(1000, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0002".to_string(),
                amount: Coin::new(960, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
        ];
        deps.querier
            .update_staking("test", &*get_validators(), &*all_delegations);

        // Undelegate from multiple validators
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(2)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(120040_u128, 100_u128),
                    undelegated_amount: Uint128::new(0),
                    create_time: env.block.time,
                    est_release_time: None,
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::one(),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Undelegate { pool_id: 12 },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::Undelegate {
                    val_addr: valid1.clone(),
                    amount: Uint128::new(1000),
                })
                .unwrap(),
                funds: vec![]
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::Undelegate {
                    val_addr: valid2.clone(),
                    amount: Uint128::new(200),
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let pool_meta = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_meta.staked, Uint128::new(210));
        assert_eq!(pool_meta.current_undelegation_batch_id, 3_u64);

        assert_eq!(res.attributes[0], attr("Undelegation_pool_id", "12"));
        assert_eq!(res.attributes[1], attr("Undelegation_amount", "1200"));
    }

    #[test]
    fn test_reconcile_funds() {
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        let mut deps = mock_dependencies_for_validator_querier(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let instantiate_msg = InstantiateMsg {
            delegator_contract: Addr::unchecked("delegator_addr").to_string(),
            scc_contract: Addr::unchecked("scc_addr").to_string(),
            unbonding_period: None,
            unbonding_buffer: None,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
        };

        instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

        let timestamp_now = env.block.time;
        // let timestamp_before = env.block.time.minus_seconds(21 * 24 * 3600);

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::ReconcileFunds { pool_id: 12 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &PoolRegistryInfo {
                    name: "RandomName".to_string(),
                    active: true,
                    validator_contract: Addr::unchecked("pool_12_validator_addr"),
                    reward_contract: Addr::unchecked("pool_12_reward_addr"),
                    protocol_fee_contract: Addr::unchecked("pool_12_protocol_addr"),
                    protocol_fee_percent: Decimal::from_ratio(1_u128, 1000_u128),
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(450),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![],
                    slashing_pointer: Decimal::one(),
                    current_undelegation_batch_id: 9,
                    last_reconciled_batch_id: 5,
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(5)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(10_u128, 1_u128),
                    undelegated_amount: Uint128::new(10),
                    create_time: env.block.time,
                    est_release_time: Some(timestamp_now),
                    last_updated_slashing_pointer: Decimal::from_ratio(9_u128, 10_u128),
                    reconciled: true,
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(6)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(20_u128, 1_u128),
                    undelegated_amount: Uint128::new(20),
                    create_time: env.block.time,
                    est_release_time: Some(timestamp_now),
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::from_ratio(9_u128, 10_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(7)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(70_u128, 1_u128),
                    undelegated_amount: Uint128::new(63),
                    create_time: env.block.time,
                    est_release_time: Some(timestamp_now),
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::from_ratio(81_u128, 100_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(8)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(10_u128, 1_u128),
                    undelegated_amount: Uint128::new(10),
                    create_time: env.block.time,
                    est_release_time: Some(timestamp_now.plus_seconds(1)),
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::one(),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(9)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(10_u128, 1_u128),
                    undelegated_amount: Uint128::new(10),
                    create_time: env.block.time,
                    est_release_time: Some(timestamp_now.plus_seconds(1)),
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::one(),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ReconcileFunds { pool_id: 12 },
        )
        .unwrap();
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::TransferReconciledFunds {
                    amount: Uint128::new(81), // Querier has been hardcoded to return 81
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let pool_meta = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_meta.last_reconciled_batch_id, 7);

        let batch_6 = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(7)))
            .unwrap();
        assert_eq!(
            batch_6.unbonding_slashing_ratio,
            Decimal::from_ratio(975903614457831325_u128, 1_000_000_000_000_000_000_u128)
        );
        assert!(batch_6.reconciled);

        let batch_7 = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(7)))
            .unwrap();
        assert_eq!(
            batch_7.unbonding_slashing_ratio,
            Decimal::from_ratio(975903614457831325_u128, 1_000_000_000_000_000_000_u128)
        );
        assert!(batch_7.reconciled);
    }

    #[test]
    fn test_withdraw_funds_to_wallet() {
        let mut deps = mock_querier::mock_dependencies_for_delegator_querier(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let instantiate_msg = InstantiateMsg {
            delegator_contract: Addr::unchecked("delegator_addr").to_string(),
            scc_contract: Addr::unchecked("scc_addr").to_string(),
            unbonding_period: None,
            unbonding_buffer: None,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
        };

        instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[Coin::new(228, "uluna")]),
            ExecuteMsg::WithdrawFundsToWallet {
                pool_id: 12,
                batch_id: 9,
                undelegate_id: 21,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[]),
            ExecuteMsg::WithdrawFundsToWallet {
                pool_id: 12,
                batch_id: 9,
                undelegate_id: 21,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UndelegationBatchNotFound {}));

        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(9)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(500_u128, 1_u128),
                    undelegated_amount: Uint128::zero(),
                    create_time: env.block.time.minus_seconds(2),
                    est_release_time: Some(env.block.time.plus_seconds(1)),
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::one(),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[]),
            ExecuteMsg::WithdrawFundsToWallet {
                pool_id: 12,
                batch_id: 9,
                undelegate_id: 21,
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::UndelegationBatchNotReconciled {}
        ));

        let final_und_batch = BatchUndelegationRecord {
            prorated_amount: Decimal::from_ratio(500_u128, 1_u128),
            undelegated_amount: Uint128::zero(),
            create_time: env.block.time.minus_seconds(2),
            est_release_time: Some(env.block.time),
            reconciled: true,
            last_updated_slashing_pointer: Decimal::from_ratio(9_u128, 10_u128),
            unbonding_slashing_ratio: Decimal::from_ratio(8_u128, 10_u128),
        };
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(9)),
                &final_und_batch,
            )
            .unwrap();
        // Querier hardcoded to return an undelegated money of 100 and withdrawable money of 80
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[]),
            ExecuteMsg::WithdrawFundsToWallet {
                pool_id: 12,
                batch_id: 9,
                undelegate_id: 21,
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "delegator_addr".to_string(),
                msg: to_binary(&DelegatorMsg::WithdrawFunds {
                    user_addr: Addr::unchecked("any"),
                    pool_id: 12,
                    undelegate_id: 21,
                    undelegation_batch_slashing_pointer: Decimal::from_ratio(9_u128, 10_u128),
                    undelegation_batch_unbonding_slashing_ratio: Decimal::from_ratio(
                        8_u128, 10_u128
                    ),
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let batch_und_meta = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(9)))
            .unwrap();
        assert_eq!(batch_und_meta, final_und_batch);
    }

    #[test]
    fn test_claim_airdrops() {
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        instantiate_contract(&mut deps, &info, &env, None);

        let execute_msg = ExecuteMsg::ClaimAirdrops { rates: vec![] };
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[Coin::new(228, "uluna")]),
            execute_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        AIRDROP_REGISTRY
            .save(
                deps.as_mut().storage,
                "uair1".to_string(),
                &AirdropRegistryInfo {
                    airdrop_contract: Addr::unchecked("uair1_airdrop_contract"),
                    cw20_contract: Addr::unchecked("uair1_cw20_contract"),
                },
            )
            .unwrap();

        AIRDROP_REGISTRY
            .save(
                deps.as_mut().storage,
                "uair2".to_string(),
                &AirdropRegistryInfo {
                    airdrop_contract: Addr::unchecked("uair2_airdrop_contract"),
                    cw20_contract: Addr::unchecked("uair2_cw20_contract"),
                },
            )
            .unwrap();

        let pool_12_initial = PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("pool_12_reward_addr"),
            protocol_fee_contract: Addr::unchecked("pool_27_protocol_addr"),
            protocol_fee_percent: Default::default(),
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(280),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![DecCoin::new(
                Decimal::from_ratio(28_u128, 280_u128),
                "uair1",
            )],
            slashing_pointer: Default::default(),
            current_undelegation_batch_id: 9,
            last_reconciled_batch_id: 5,
        };
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &pool_12_initial)
            .unwrap();
        let pool_27_initial = PoolRegistryInfo {
            name: "RandomName".to_string(),
            validator_contract: Addr::unchecked("pool_27_validator_addr"),
            reward_contract: Addr::unchecked("pool_27_reward_addr"),
            protocol_fee_contract: Addr::unchecked("pool_27_protocol_addr"),
            active: true,
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(640),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![DecCoin::new(
                Decimal::from_ratio(64_u128, 640_u128),
                "uair2",
            )],
            slashing_pointer: Default::default(),
            current_undelegation_batch_id: 9,
            last_reconciled_batch_id: 5,
            protocol_fee_percent: Default::default(),
        };
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(27), &pool_27_initial)
            .unwrap();

        let execute_msg = ExecuteMsg::ClaimAirdrops {
            rates: vec![
                AirdropRate {
                    pool_id: 12,
                    denom: "uair1".to_string(),
                    amount: Uint128::new(14),
                    stage: 10,
                    proof: vec!["proof".to_string()],
                },
                AirdropRate {
                    pool_id: 27,
                    denom: "uair1".to_string(),
                    amount: Uint128::new(16),
                    stage: 10,
                    proof: vec!["proof".to_string()],
                },
                AirdropRate {
                    pool_id: 12,
                    denom: "uair2".to_string(),
                    amount: Uint128::new(14),
                    stage: 10,
                    proof: vec!["proof".to_string()],
                },
                AirdropRate {
                    pool_id: 27,
                    denom: "uair2".to_string(),
                    amount: Uint128::new(16),
                    stage: 10,
                    proof: vec!["proof".to_string()],
                },
            ],
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            execute_msg.clone(),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 4);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::ClaimAirdrop {
                    amount: Uint128::new(14),
                    claim_msg: to_binary(&MerkleAirdropMsg::Claim {
                        stage: 10,
                        amount: Uint128::new(14),
                        proof: vec!["proof".to_string()],
                    })
                    .unwrap(),
                    airdrop_contract: Addr::unchecked("uair1_airdrop_contract"),
                })
                .unwrap(),
                funds: vec![]
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_27_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::ClaimAirdrop {
                    amount: Uint128::new(16),
                    claim_msg: to_binary(&MerkleAirdropMsg::Claim {
                        stage: 10,
                        amount: Uint128::new(16),
                        proof: vec!["proof".to_string()],
                    })
                    .unwrap(),
                    airdrop_contract: Addr::unchecked("uair1_airdrop_contract"),
                })
                .unwrap(),
                funds: vec![]
            })
        );
        assert_eq!(
            res.messages[2],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::ClaimAirdrop {
                    amount: Uint128::new(14),
                    claim_msg: to_binary(&MerkleAirdropMsg::Claim {
                        stage: 10,
                        amount: Uint128::new(14),
                        proof: vec!["proof".to_string()],
                    })
                    .unwrap(),
                    airdrop_contract: Addr::unchecked("uair2_airdrop_contract"),
                })
                .unwrap(),
                funds: vec![]
            })
        );
        assert_eq!(
            res.messages[3],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_27_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::ClaimAirdrop {
                    amount: Uint128::new(16),
                    claim_msg: to_binary(&MerkleAirdropMsg::Claim {
                        stage: 10,
                        amount: Uint128::new(16),
                        proof: vec!["proof".to_string()],
                    })
                    .unwrap(),
                    airdrop_contract: Addr::unchecked("uair2_airdrop_contract"),
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let pool_12 = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert!(check_equal_deccoin_vector(
            &pool_12.airdrops_pointer,
            &pool_12_initial.airdrops_pointer
        ));
        let pool_27 = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(27))
            .unwrap();
        assert!(check_equal_deccoin_vector(
            &pool_27.airdrops_pointer,
            &pool_27_initial.airdrops_pointer
        ));
    }

    #[test]
    fn test_update_airdrop_pointers() {
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        let mut deps = mock_dependencies_for_validator_querier(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let instantiate_msg = InstantiateMsg {
            delegator_contract: Addr::unchecked("delegator_addr").to_string(),
            scc_contract: Addr::unchecked("scc_addr").to_string(),
            unbonding_period: None,
            unbonding_buffer: None,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
        };

        instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

        let execute_msg = ExecuteMsg::ClaimAirdrops { rates: vec![] };
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[Coin::new(228, "uluna")]),
            execute_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        AIRDROP_REGISTRY
            .save(
                deps.as_mut().storage,
                "uair1".to_string(),
                &AirdropRegistryInfo {
                    airdrop_contract: Addr::unchecked("uair1_airdrop_contract"),
                    cw20_contract: Addr::unchecked("uair1_cw20_contract"),
                },
            )
            .unwrap();

        AIRDROP_REGISTRY
            .save(
                deps.as_mut().storage,
                "uair2".to_string(),
                &AirdropRegistryInfo {
                    airdrop_contract: Addr::unchecked("uair2_airdrop_contract"),
                    cw20_contract: Addr::unchecked("uair2_cw20_contract"),
                },
            )
            .unwrap();

        let pool_12_initial = PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("pool_12_reward_addr"),
            protocol_fee_contract: Addr::unchecked("pool_27_protocol_addr"),
            protocol_fee_percent: Default::default(),
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(280),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![DecCoin::new(
                Decimal::from_ratio(28_u128, 280_u128),
                "uair1",
            )],
            slashing_pointer: Default::default(),
            current_undelegation_batch_id: 9,
            last_reconciled_batch_id: 5,
        };
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &pool_12_initial)
            .unwrap();
        let pool_27_initial = PoolRegistryInfo {
            name: "RandomName".to_string(),
            validator_contract: Addr::unchecked("pool_27_validator_addr"),
            reward_contract: Addr::unchecked("pool_27_reward_addr"),
            protocol_fee_contract: Addr::unchecked("pool_27_protocol_addr"),
            active: true,
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(640),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![DecCoin::new(
                Decimal::from_ratio(64_u128, 640_u128),
                "uair2",
            )],
            slashing_pointer: Default::default(),
            current_undelegation_batch_id: 9,
            last_reconciled_batch_id: 5,
            protocol_fee_percent: Default::default(),
        };
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(27), &pool_27_initial)
            .unwrap();

        let execute_msg = ExecuteMsg::UpdateAirdropPointers {
            transfers: vec![
                AirdropTransferRequest {
                    pool_id: 12,
                    denom: "uair1".to_string(),
                },
                AirdropTransferRequest {
                    pool_id: 27,
                    denom: "uair2".to_string(),
                },
            ],
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            execute_msg.clone(),
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_12_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::TransferAirdrop {
                    amount: Uint128::new(20),
                    cw20_contract: Addr::unchecked("uair1_cw20_contract"),
                })
                .unwrap(),
                funds: vec![]
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: "pool_27_validator_addr".to_string(),
                msg: to_binary(&ValidatorMsg::TransferAirdrop {
                    amount: Uint128::new(20),
                    cw20_contract: Addr::unchecked("uair2_cw20_contract"),
                })
                .unwrap(),
                funds: vec![]
            })
        );
        let pool_12 = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert!(check_equal_deccoin_vector(
            &pool_12.airdrops_pointer,
            &vec![DecCoin::new(
                Decimal::from_ratio(48_u128, 280_u128),
                "uair1"
            )]
        ));
        let pool_27 = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(27))
            .unwrap();
        assert!(check_equal_deccoin_vector(
            &pool_27.airdrops_pointer,
            &vec![DecCoin::new(
                Decimal::from_ratio(84_u128, 640_u128),
                "uair2"
            )]
        ));
    }

    #[test]
    fn test_update_pool_config() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        instantiate_contract(&mut deps, &info, &env, None);
        let valid1 = Addr::unchecked("0001");
        let initial_msg = ExecuteMsg::UpdatePoolMetadata {
            pool_id: 12,
            pool_config_update_request: PoolConfigUpdateRequest {
                active: None,
                reward_contract: None,
                protocol_fee_contract: None,
                protocol_fee_percent: None,
            },
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
            mock_info("creator", &[Coin::new(14, "uluna")]),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::PoolNotFound {}));

        let initial_pool_info = PoolRegistryInfo {
            name: "asdf".to_string(),
            active: true,
            validator_contract: Addr::unchecked("validator_addr"),
            reward_contract: Addr::unchecked("reward_addr"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
            protocol_fee_percent: Decimal::from_ratio(1_u128, 1000_u128),
            validators: vec![valid1.clone()],
            staked: Uint128::new(240),
            rewards_pointer: Decimal::from_ratio(12_u128, 240_u128),
            airdrops_pointer: vec![DecCoin::new(
                Decimal::from_ratio(10_u128, 240_u128),
                "uair1",
            )],
            slashing_pointer: Decimal::from_ratio(1_u128, 1000_u128),
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
        };

        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &initial_pool_info)
            .unwrap();

        let mut expected_pool_info = initial_pool_info.clone();
        let pool_info = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_info, expected_pool_info);

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            initial_msg.clone(),
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let pool_info = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_info, expected_pool_info);

        expected_pool_info = PoolRegistryInfo {
            name: "asdf".to_string(),
            active: false,
            validator_contract: Addr::unchecked("validator_addr"),
            reward_contract: Addr::unchecked("new_reward_addr"),
            protocol_fee_contract: Addr::unchecked("new_protocol_fee_addr"),
            protocol_fee_percent: Decimal::from_ratio(2_u128, 1000_u128),
            validators: vec![valid1.clone()],
            staked: Uint128::new(240),
            rewards_pointer: Decimal::from_ratio(12_u128, 240_u128),
            airdrops_pointer: vec![DecCoin::new(
                Decimal::from_ratio(10_u128, 240_u128),
                "uair1",
            )],
            slashing_pointer: Decimal::from_ratio(1_u128, 1000_u128),
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdatePoolMetadata {
                pool_id: 12,
                pool_config_update_request: PoolConfigUpdateRequest {
                    active: Some(false),
                    reward_contract: Some("new_reward_addr".to_string()),
                    protocol_fee_contract: Some("new_protocol_fee_addr".to_string()),
                    protocol_fee_percent: Some(Decimal::from_ratio(2_u128, 1000_u128)),
                },
            },
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let pool_info = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_info, expected_pool_info);
    }

    #[test]
    fn test_update_config() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_msg = ExecuteMsg::UpdateConfig {
            config_request: ConfigUpdateRequest {
                delegator_contract: None,
                scc_contract: None,
                min_deposit: None,
                max_deposit: None,
                unbonding_period: None,
                unbonding_buffer: None,
            },
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
            mock_info("creator", &[Coin::new(14, "uluna")]),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        let mut expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "uluna".to_string(),
            delegator_contract: Addr::unchecked("delegator_addr"),
            scc_contract: Addr::unchecked("scc_addr"),
            unbonding_period: 1814400,
            unbonding_buffer: 3600,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
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
            vault_denom: "uluna".to_string(),
            delegator_contract: Addr::unchecked("new_delegator_addr"),
            scc_contract: Addr::unchecked("new_scc_addr"),
            unbonding_period: 1814401,
            unbonding_buffer: 3601,
            min_deposit: Uint128::new(1001),
            max_deposit: Uint128::new(1_000_000_000_001),
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                config_request: ConfigUpdateRequest {
                    delegator_contract: Some(Addr::unchecked("new_delegator_addr")),
                    scc_contract: Some(Addr::unchecked("new_scc_addr")),
                    min_deposit: Some(Uint128::new(1001)),
                    max_deposit: Some(Uint128::new(1_000_000_000_001)),
                    unbonding_period: Some(1814401),
                    unbonding_buffer: Some(3601),
                },
            }
            .clone(),
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config, expected_config);
    }

    #[test]
    fn test_update_airdrop_registry() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        instantiate_contract(&mut deps, &info, &env, None);
        let other_info = mock_info(
            &Addr::unchecked("other").to_string(),
            &[Coin::new(1200, "uluna")],
        );
        let denom = "ABC".to_string();
        let airdrop_contract = Addr::unchecked("def".to_string());
        let token_contract = Addr::unchecked("efg".to_string());

        // Expects a manager to call
        let err = execute(
            deps.as_mut(),
            env.clone(),
            other_info.clone(),
            ExecuteMsg::UpdateAirdropRegistry {
                airdrop_token: denom.clone(),
                airdrop_contract: airdrop_contract.clone().to_string(),
                cw20_contract: token_contract.clone().to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        assert!(AIRDROP_REGISTRY
            .may_load(deps.as_mut().storage, denom.clone())
            .unwrap()
            .is_none());
        let res = execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::UpdateAirdropRegistry {
                airdrop_token: denom.clone(),
                airdrop_contract: airdrop_contract.clone().to_string(),
                cw20_contract: token_contract.clone().to_string(),
            },
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let airdrop_registry_info = AIRDROP_REGISTRY
            .may_load(deps.as_mut().storage, denom.clone())
            .unwrap();
        assert!(airdrop_registry_info.is_none());

        let airdrop_registry_info = AIRDROP_REGISTRY
            .may_load(deps.as_mut().storage, denom.to_lowercase().clone())
            .unwrap();
        assert!(airdrop_registry_info.is_some());

        let info = airdrop_registry_info.unwrap();
        assert_eq!(info.airdrop_contract, airdrop_contract.clone());
        assert_eq!(info.cw20_contract, token_contract.clone());
    }

    #[test]
    fn test_check_slashing() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");

        instantiate_contract(&mut deps, &info, &env, None);

        let all_delegations = vec![
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0001".to_string(),
                amount: Coin::new(150, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0002".to_string(),
                amount: Coin::new(60, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
            FullDelegation {
                delegator: Addr::unchecked("pool_12_validator_addr"), // Validator contract
                validator: "valid0003".to_string(),
                amount: Coin::new(33, "uluna"),
                can_redelegate: Default::default(),
                accumulated_rewards: vec![],
            },
        ];

        let mut validators_discoverable = get_validators();
        validators_discoverable.push(Validator {
            address: "valid0004".to_string(),
            commission: Decimal::zero(),
            max_commission: Decimal::zero(),
            max_change_rate: Decimal::zero(),
        });

        deps.querier
            .update_staking("test", &validators_discoverable, &all_delegations);

        let initial_pool_info = PoolRegistryInfo {
            name: "ASDF".to_string(),
            active: true,
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("reward_addr"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_addr"),
            protocol_fee_percent: Decimal::from_ratio(1_u128, 1000_u128),
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(270),
            rewards_pointer: Decimal::from_ratio(12_u128, 240_u128),
            airdrops_pointer: vec![DecCoin::new(
                Decimal::from_ratio(10_u128, 240_u128),
                "uair1",
            )],
            slashing_pointer: Decimal::from_ratio(9_u128, 10_u128),
            current_undelegation_batch_id: 3,
            last_reconciled_batch_id: 2,
        };
        POOL_REGISTRY
            .save(deps.as_mut().storage, U64Key::new(12), &initial_pool_info)
            .unwrap();

        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid1.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(150),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid2.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(60),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();
        VALIDATOR_META
            .save(
                deps.as_mut().storage,
                (valid3.clone(), U64Key::new(12)),
                &VMeta {
                    staked: Uint128::new(60),
                    slashed: Default::default(),
                    filled: Default::default(),
                },
            )
            .unwrap();

        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(3)),
                &BatchUndelegationRecord {
                    prorated_amount: Decimal::from_ratio(201_u128, 1_u128),
                    undelegated_amount: Uint128::zero(),
                    create_time: Default::default(),
                    est_release_time: None,
                    reconciled: false,
                    last_updated_slashing_pointer: Decimal::from_ratio(9_u128, 10_u128),
                    unbonding_slashing_ratio: Decimal::one(),
                },
            )
            .unwrap();

        check_slashing(&mut deps.as_mut(), env, 12).unwrap();

        let vmeta_1 = VALIDATOR_META
            .load(deps.as_mut().storage, (valid1.clone(), U64Key::new(12)))
            .unwrap();
        let vmeta_2 = VALIDATOR_META
            .load(deps.as_mut().storage, (valid2.clone(), U64Key::new(12)))
            .unwrap();
        let vmeta_3 = VALIDATOR_META
            .load(deps.as_mut().storage, (valid3.clone(), U64Key::new(12)))
            .unwrap();

        assert_eq!(
            vmeta_1,
            VMeta {
                staked: Uint128::new(150),
                slashed: Default::default(),
                filled: Default::default()
            }
        );
        assert_eq!(
            vmeta_2,
            VMeta {
                staked: Uint128::new(60),
                slashed: Default::default(),
                filled: Default::default()
            }
        );
        assert_eq!(
            vmeta_3,
            VMeta {
                staked: Uint128::new(33),
                slashed: Uint128::new(27),
                filled: Default::default()
            }
        );

        let pool_registry = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_registry.staked, Uint128::new(243));
        assert_eq!(
            pool_registry.slashing_pointer,
            Decimal::from_ratio(81_u128, 100_u128)
        );
        assert_eq!(
            pool_registry.rewards_pointer,
            initial_pool_info.rewards_pointer
        ); // No change
        assert_eq!(
            pool_registry.airdrops_pointer,
            initial_pool_info.airdrops_pointer
        ); // No change

        let batch = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(3)))
            .unwrap();
        assert_eq!(
            batch,
            BatchUndelegationRecord {
                prorated_amount: Decimal::from_ratio(1809_u128, 10_u128),
                undelegated_amount: Uint128::new(0),
                create_time: Default::default(),
                est_release_time: None,
                reconciled: false,
                last_updated_slashing_pointer: Decimal::from_ratio(81_u128, 100_u128),
                unbonding_slashing_ratio: Decimal::one()
            }
        );
    }
}
