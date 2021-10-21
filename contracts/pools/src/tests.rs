#[cfg(test)]
mod tests {
    // use crate::contract::{execute, instantiate, query, reply, MESSAGE_REPLY_SWAP_ID};
    use crate::error::ContractError;
    use crate::msg::{
        ExecuteMsg, InstantiateMsg, QueryConfigResponse, QueryMsg, QueryStateResponse,
    };
    use crate::state::{AirdropRate, BatchUndelegationRecord, Config, ConfigUpdateRequest, PoolRegistryInfo, State, BATCH_UNDELEGATION_REGISTRY, CONFIG, POOL_REGISTRY, STATE, AIRDROP_REGISTRY, AirdropRegistryInfo};
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
        MOCK_CONTRACT_ADDR,
    };
    use crate::request_validation::{create_new_undelegation_batch, get_validator_for_deposit};
    use cosmwasm_std::{attr, from_binary, to_binary, Addr, Coin, ContractResult, Decimal, Env, Event, FullDelegation, MessageInfo, OwnedDeps, Reply, Response, StdResult, SubMsg, SubMsgExecutionResponse, Uint128, Validator, WasmMsg, QueryRequest, Binary};
    use cw_storage_plus::U64Key;
    use delegator::msg::ExecuteMsg as DelegatorMsg;
    use stader_utils::coin_utils::{check_equal_deccoin_vector, DecCoin};
    use stader_utils::event_constants::{
        EVENT_KEY_IDENTIFIER, EVENT_SWAP_KEY_AMOUNT, EVENT_SWAP_TYPE,
    };
    use terra_cosmwasm::TerraMsgWrapper;
    use validator::msg::ExecuteMsg as ValidatorMsg;
    use reward::msg::ExecuteMsg as RewardExecuteMsg;
    use crate::contract::{instantiate, query, execute};

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
            delegator_contract: Addr::unchecked("delegator_addr"),
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
            vault_denom: "utest".to_string(),
            delegator_contract: Addr::unchecked("delegator_addr"),
            unbonding_period: None,
            unbonding_buffer: None,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
        };
        let expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "utest".to_string(),
            delegator_contract: Addr::unchecked("delegator_addr"),
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
                validator_contract: Addr::unchecked("pool0_val_addr"),
                reward_contract:  Addr::unchecked("pool0_rew_addr")
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
                name: "Community Pro".to_string(),
                validator_contract: Addr::unchecked("pool0_val_addr"),
                reward_contract:  Addr::unchecked("pool0_rew_addr")
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0], SubMsg::new(WasmMsg::Execute {
            contract_addr: "pool0_val_addr".to_string(),
            msg: to_binary(&ValidatorMsg::SetRewardWithdrawAddress { reward_contract: Addr::unchecked("pool0_rew_addr") }).unwrap(),
            funds: vec![]
        }));

        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.next_pool_id, 1_u64);

        let pool_meta = POOL_REGISTRY
            .may_load(deps.as_mut().storage, U64Key::new(0))
            .unwrap()
            .unwrap();
        let expected_pool_info = PoolRegistryInfo {
            name: "Community Pro".to_string(),
            validator_contract: Addr::unchecked("pool0_val_addr"),
            reward_contract:  Addr::unchecked("pool0_rew_addr"),
            active: true,
            validators: vec![],
            staked: Default::default(),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
        };
        assert!(pool_meta.eq(&expected_pool_info));

        let batch_undelegation_info = BATCH_UNDELEGATION_REGISTRY
            .may_load(deps.as_mut().storage, (U64Key::new(0), U64Key::new(1)))
            .unwrap()
            .unwrap();
        let expected_batch_undelegation = BatchUndelegationRecord {
            amount: Uint128::zero(),
            create_time: env.block.time,
            est_release_time: None,
            withdrawable_time: None,
        };
        assert!(batch_undelegation_info.eq(&expected_batch_undelegation));


        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(100, "utest")]),
            ExecuteMsg::AddPool {
                name: "Community Validator".to_string(),
                validator_contract: Addr::unchecked("pool0_val_addr"),
                reward_contract:  Addr::unchecked("pool0_rew_addr")
            },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorContractInUse {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(100, "utest")]),
            ExecuteMsg::AddPool {
                name: "Community Validator".to_string(),
                validator_contract: Addr::unchecked("new_pool0_val_addr"),
                reward_contract:  Addr::unchecked("pool0_rew_addr")
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

        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &PoolRegistryInfo {
                    name: "Random name".to_string(),
                    active: false,
                    validator_contract: Addr::unchecked("pool_12_validator_addr"),
                    reward_contract: Addr::unchecked("reward_12_validator_addr"),
                    validators: vec![],
                    staked: Default::default(),
                    rewards_pointer: Default::default(),
                    airdrops_pointer: vec![],
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

        let pool_meta = POOL_REGISTRY
            .may_load(deps.as_mut().storage, U64Key::new(12))
            .unwrap()
            .unwrap();
        let expected_pool_info = PoolRegistryInfo {
            name: "Random name".to_string(),
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("reward_12_validator_addr"),
            active: false,
            validators: vec![valid2.clone()],
            staked: Default::default(),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 0,
            last_reconciled_batch_id: 0,
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
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());
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
                pool_id: 12
            },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12
            },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::PoolNotFound {}));

        let mut pool_info = PoolRegistryInfo {
            name: "Random name".to_string(),
            active: false,
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("reward_12_validator_addr"),
            validators: vec![],
            staked: Default::default(),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 0,
            last_reconciled_batch_id: 0,
        };
        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &pool_info,
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid1.clone(),
                pool_id: 12
            },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::RemoveValidatorsCannotBeSame {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12
            },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        pool_info.validators.push(valid1.clone());
        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(12), &pool_info).unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12
            },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        pool_info.validators.push(valid2.clone());
        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(12), &pool_info).unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redel_addr: valid2.clone(),
                pool_id: 12
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
            active: false,
            validators: vec![valid2.clone()],
            staked: Default::default(),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 0,
            last_reconciled_batch_id: 0,
        };
        assert!(pool_meta.eq(&expected_pool_info));
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
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

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
            mock_info(&user1.clone().to_string(), &[Coin::new(1_000_000_000_001, "utest")]),
            ExecuteMsg::Deposit { pool_id: 12 },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::MaxDeposit {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(1, "utest")]),
            ExecuteMsg::Deposit { pool_id: 12 },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::MinDeposit {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(3000, "utest")]),
            ExecuteMsg::Deposit { pool_id: 12 },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::PoolNotFound {}));

        let mut pool_info = PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: false,
            validator_contract: Addr::unchecked("pool_12_validator_addr"),
            reward_contract: Addr::unchecked("reward_12_validator_addr"),
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone(), valid4.clone(), Addr::unchecked("valid0005").clone()],
            staked: Uint128::new(450),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
        };
        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &pool_info,
            )
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(3000, "utest")]),
            ExecuteMsg::Deposit { pool_id: 12 },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::PoolInactive {}));

        pool_info.active = true;
        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &pool_info,
            )
            .unwrap();

        /* Validator 5 is not discoverable - essentially jailed. So don't deposit in any case */
        // CASE 1 - Val 1,2,3 are discoverable. Val 4 does not have a delegation. So val4 should get picked.
        let mut validators_discoverable = get_validators();
        validators_discoverable.push(Validator {
            address: "valid0004".to_string(),
            commission: Decimal::zero(),
            max_commission: Decimal::zero(),
            max_change_rate: Decimal::zero(),
        });

        deps.querier
            .update_staking("test", &*validators_discoverable, &*get_delegations());
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(3000, "utest")]),
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
                    pool_airdrops_pointer: vec![]
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
                funds: vec![Coin::new(3000, "utest")]
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

        let mut validator_delegations = get_delegations();
        validator_delegations.push(FullDelegation {
            delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
            validator: valid4.to_string(),
            amount: Coin::new(3000, "utest"),
            can_redelegate: Default::default(),
            accumulated_rewards: vec![]
        });

        deps.querier
            .update_staking("test", &*validators_discoverable, &*validator_delegations);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(1000, "utest")]),
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
                    pool_airdrops_pointer: vec![]
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
                funds: vec![Coin::new(1000, "utest")]
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

        let mut validator_delegations = get_delegations();
        validator_delegations.pop();
        validator_delegations.push(FullDelegation {
            delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
            validator: valid3.to_string(),
            amount: Coin::new(1000, "utest"),
            can_redelegate: Default::default(),
            accumulated_rewards: vec![]
        });
        validator_delegations.push(FullDelegation {
            delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
            validator: valid4.to_string(),
            amount: Coin::new(3000, "utest"),
            can_redelegate: Default::default(),
            accumulated_rewards: vec![]
        });

        deps.querier
            .update_staking("test", &*validators_discoverable, &*validator_delegations);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(5000, "utest")]),
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
                    pool_airdrops_pointer: vec![]
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
                funds: vec![Coin::new(5000, "utest")]
            })
        );

        let pool_meta = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_meta.staked, Uint128::new(9450));
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
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(450),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![],
                    current_undelegation_batch_id: 1,
                    last_reconciled_batch_id: 0,
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
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(450),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![],
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
            SubMsg::new(
                WasmMsg::Execute {
                    contract_addr: "pool_12_reward_addr".to_string(),
                    msg: to_binary(&RewardExecuteMsg::Swap {})
                    .unwrap(),
                    funds: vec![]
                },
            )
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
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(450),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![],
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

        deps.querier.update_balance(Addr::unchecked("pool_12_reward_addr"), vec![Coin::new(1234, "utest")]);
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
            SubMsg::new(
                WasmMsg::Execute {
                    contract_addr: "pool_12_reward_addr".to_string(),
                    msg: to_binary(&RewardExecuteMsg::Transfer { amount: Uint128::new(1234) })
                        .unwrap(),
                    funds: vec![]
                },
            )
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
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        instantiate_contract(&mut deps, &info, &env, None);

        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &PoolRegistryInfo {
                    name: "RandomName".to_string(),
                    active: false,
                    validator_contract: Addr::unchecked("pool_12_validator_addr"),
                    reward_contract: Addr::unchecked("pool_12_reward_addr"),
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(450),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![],
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
                    amount: Uint128::new(40),
                    create_time: env.block.time,
                    est_release_time: None,
                    withdrawable_time: None,
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
                    pool_airdrops_pointer: vec![]
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let pool_meta = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_meta.staked, Uint128::new(450));

        let batch_und = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(1)))
            .unwrap();
        assert_eq!(batch_und.amount, Uint128::new(60));
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
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

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
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone(), valid4.clone()],
                    staked: Uint128::new(1450),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![],
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
                    amount: Uint128::new(0),
                    create_time: env.block.time,
                    est_release_time: None,
                    withdrawable_time: None,
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
                    amount: Uint128::new(40),
                    create_time: env.block.time,
                    est_release_time: None,
                    withdrawable_time: None,
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
        assert_eq!(pool_meta.staked, Uint128::new(1410)); // Staked amount is deducted when user undelegates in the first place
        assert_eq!(pool_meta.current_undelegation_batch_id, 2);
        assert_eq!(pool_meta.last_reconciled_batch_id, 0);

        let batch_und = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(1)))
            .unwrap();
        assert_eq!(batch_und.amount, Uint128::new(40));
        assert_eq!(
            batch_und.est_release_time.unwrap(),
            env.block.time.plus_seconds(21 * 24 * 3600)
        );
        assert_eq!(
            batch_und.withdrawable_time.unwrap(),
            env.block.time.plus_seconds(21 * 24 * 3600 + 3600)
        );

        assert!(BATCH_UNDELEGATION_REGISTRY
            .may_load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(2)))
            .unwrap()
            .is_some());

        let mut validator_delegations = get_delegations();
        validator_delegations[1] = FullDelegation {
            delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
            validator: valid2.to_string(),
            amount: Coin::new(960, "utest"),
            can_redelegate: Default::default(),
            accumulated_rewards: vec![]
        };
        deps.querier
            .update_staking("test", &*get_validators(), &*validator_delegations);

        // Undelegate from multiple validators
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(2)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(1200),
                    create_time: env.block.time,
                    est_release_time: None,
                    withdrawable_time: None,
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
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        instantiate_contract(&mut deps, &info, &env, None);

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
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(450),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![],
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
                    amount: Uint128::new(10),
                    create_time: env.block.time,
                    est_release_time: Some(timestamp_now),
                    withdrawable_time: Some(timestamp_now),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(6)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(20),
                    create_time: env.block.time,
                    est_release_time: Some(timestamp_now),
                    withdrawable_time: Some(timestamp_now),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(7)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(70),
                    create_time: env.block.time,
                    est_release_time: Some(timestamp_now),
                    withdrawable_time: Some(timestamp_now),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(8)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(120),
                    create_time: env.block.time,
                    est_release_time: Some(timestamp_now.plus_seconds(1)),
                    withdrawable_time: Some(timestamp_now.plus_seconds(1)),
                },
            )
            .unwrap();
        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(9)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(50),
                    create_time: env.block.time,
                    est_release_time: Some(timestamp_now.plus_seconds(1)),
                    withdrawable_time: Some(timestamp_now.plus_seconds(1)),
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
                    amount: Uint128::new(90),
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let pool_meta = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        assert_eq!(pool_meta.last_reconciled_batch_id, 7);
    }

    #[test]
    fn test_withdraw_funds_to_wallet() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        instantiate_contract(&mut deps, &info, &env, None);

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[Coin::new(228, "utest")]),
            ExecuteMsg::WithdrawFundsToWallet {
                pool_id: 12,
                batch_id: 9,
                undelegate_id: 21,
                amount: Uint128::new(45),
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
                amount: Uint128::new(45),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UndelegationBatchNotFound {}));

        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(9)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(500),
                    create_time: env.block.time.minus_seconds(2),
                    est_release_time: Some(env.block.time.plus_seconds(1)),
                    withdrawable_time: Some(env.block.time.plus_seconds(2)),
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
                amount: Uint128::new(45),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UndelegationNotWithdrawable {}));

        BATCH_UNDELEGATION_REGISTRY
            .save(
                deps.as_mut().storage,
                (U64Key::new(12), U64Key::new(9)),
                &BatchUndelegationRecord {
                    amount: Uint128::new(500),
                    create_time: env.block.time.minus_seconds(2),
                    est_release_time: Some(env.block.time),
                    withdrawable_time: Some(env.block.time),
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[]),
            ExecuteMsg::WithdrawFundsToWallet {
                pool_id: 12,
                batch_id: 9,
                undelegate_id: 21,
                amount: Uint128::new(45),
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
                    amount: Uint128::new(45),
                })
                .unwrap(),
                funds: vec![]
            })
        );

        let batch_und_meta = BATCH_UNDELEGATION_REGISTRY
            .load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(9)))
            .unwrap();
        assert_eq!(batch_und_meta.amount, Uint128::new(455));
    }

    #[test]
    fn test_claim_airdrops() {
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        fn get_airdrop_claim_msg() -> Binary {
            Binary::from(vec![01, 02, 03, 04, 05, 06, 07, 08])
        }

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        instantiate_contract(&mut deps, &info, &env, None);

        let execute_msg = ExecuteMsg::ClaimAirdrops {
            rates: vec![],
        };
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[Coin::new(228, "utest")]),
            execute_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &PoolRegistryInfo {
                    name: "RandomName".to_string(),
                    validator_contract: Addr::unchecked("pool_12_validator_addr"),
                    reward_contract: Addr::unchecked("pool_12_reward_addr"),
                    active: false,
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(280),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![DecCoin::new(
                        Decimal::from_ratio(28_u128, 280_u128),
                        "uair1",
                    )],
                    current_undelegation_batch_id: 9,
                    last_reconciled_batch_id: 5,
                },
            )
            .unwrap();
        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(27),
                &PoolRegistryInfo {
                    name: "RandomName".to_string(),
                    validator_contract: Addr::unchecked("pool_27_validator_addr"),
                    reward_contract: Addr::unchecked("pool_27_reward_addr"),
                    active: false,
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(640),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![DecCoin::new(
                        Decimal::from_ratio(28_u128, 280_u128),
                        "uair2",
                    )],
                    current_undelegation_batch_id: 9,
                    last_reconciled_batch_id: 5,
                },
            )
            .unwrap();

        AIRDROP_REGISTRY.save(deps.as_mut().storage, "uair1".to_string(), &AirdropRegistryInfo {
            airdrop_contract: Addr::unchecked("uair1_airdrop_contract"),
            cw20_contract: Addr::unchecked("uair1_cw20_contract"),
        }).unwrap();

        AIRDROP_REGISTRY.save(deps.as_mut().storage, "uair2".to_string(), &AirdropRegistryInfo {
            airdrop_contract: Addr::unchecked("uair2_airdrop_contract"),
            cw20_contract: Addr::unchecked("uair2_cw20_contract"),
        }).unwrap();

        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(12),
                &PoolRegistryInfo {
                    name: "RandomName".to_string(),
                    active: true,
                    validator_contract: Addr::unchecked("pool_12_validator_addr"),
                    reward_contract: Addr::unchecked("pool_12_reward_addr"),
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(280),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![DecCoin::new(
                        Decimal::from_ratio(28_u128, 280_u128),
                        "uair1",
                    )],
                    current_undelegation_batch_id: 9,
                    last_reconciled_batch_id: 5,
                },
            )
            .unwrap();
        POOL_REGISTRY
            .save(
                deps.as_mut().storage,
                U64Key::new(27),
                &PoolRegistryInfo {
                    name: "RandomName".to_string(),
                    validator_contract: Addr::unchecked("pool_27_validator_addr"),
                    reward_contract: Addr::unchecked("pool_27_reward_addr"),
                    active: true,
                    validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                    staked: Uint128::new(640),
                    rewards_pointer: Decimal::zero(),
                    airdrops_pointer: vec![DecCoin::new(
                        Decimal::from_ratio(64_u128, 640_u128),
                        "uair2",
                    )],
                    current_undelegation_batch_id: 9,
                    last_reconciled_batch_id: 5,
                },
            )
            .unwrap();

        let execute_msg = ExecuteMsg::ClaimAirdrops {
            rates: vec![
                AirdropRate {
                    pool_id: 12,
                    denom: "uair1".to_string(),
                    amount: Uint128::new(14),
                    claim_msg: get_airdrop_claim_msg()
                },
                AirdropRate {
                    pool_id: 27,
                    denom: "uair1".to_string(),
                    amount: Uint128::new(16),
                    claim_msg: get_airdrop_claim_msg()
                },
                AirdropRate {
                    pool_id: 12,
                    denom: "uair2".to_string(),
                    amount: Uint128::new(14),
                    claim_msg: get_airdrop_claim_msg()
                },
                AirdropRate {
                    pool_id: 27,
                    denom: "uair2".to_string(),
                    amount: Uint128::new(16),
                    claim_msg: get_airdrop_claim_msg()
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
        assert_eq!(res.messages[0], SubMsg::new(WasmMsg::Execute {
            contract_addr: "pool_12_validator_addr".to_string(),
            msg: to_binary(&ValidatorMsg::RedeemAirdropAndTransfer {
                amount: Uint128::new(14),
                claim_msg: get_airdrop_claim_msg(),
                airdrop_contract: Addr::unchecked("uair1_airdrop_contract"),
                cw20_contract: Addr::unchecked("uair1_cw20_contract")
            }).unwrap(),
            funds: vec![]
        }));
        assert_eq!(res.messages[1], SubMsg::new(WasmMsg::Execute {
            contract_addr: "pool_27_validator_addr".to_string(),
            msg: to_binary(&ValidatorMsg::RedeemAirdropAndTransfer {
                amount: Uint128::new(16),
                claim_msg: get_airdrop_claim_msg(),
                airdrop_contract: Addr::unchecked("uair1_airdrop_contract"),
                cw20_contract: Addr::unchecked("uair1_cw20_contract")
            }).unwrap(),
            funds: vec![]
        }));
        assert_eq!(res.messages[2], SubMsg::new(WasmMsg::Execute {
            contract_addr: "pool_12_validator_addr".to_string(),
            msg: to_binary(&ValidatorMsg::RedeemAirdropAndTransfer {
                amount: Uint128::new(14),
                claim_msg: get_airdrop_claim_msg(),
                airdrop_contract: Addr::unchecked("uair2_airdrop_contract"),
                cw20_contract: Addr::unchecked("uair2_cw20_contract")
            }).unwrap(),
            funds: vec![]
        }));
        assert_eq!(res.messages[3], SubMsg::new(WasmMsg::Execute {
            contract_addr: "pool_27_validator_addr".to_string(),
            msg: to_binary(&ValidatorMsg::RedeemAirdropAndTransfer {
                amount: Uint128::new(16),
                claim_msg: get_airdrop_claim_msg(),
                airdrop_contract: Addr::unchecked("uair2_airdrop_contract"),
                cw20_contract: Addr::unchecked("uair2_cw20_contract")
            }).unwrap(),
            funds: vec![]
        }));

        let pool_12 = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(12))
            .unwrap();
        let expected_pool12_pointers = vec![
            DecCoin::new(
                Decimal::from_ratio(15_u128, 100_u128),
                "uair1"
            ),
            DecCoin::new(
                Decimal::from_ratio(14_u128, 280_u128),
                "uair2"
            ),
        ];
        assert!(check_equal_deccoin_vector(&pool_12.airdrops_pointer, &expected_pool12_pointers));
        let pool_27 = POOL_REGISTRY
            .load(deps.as_mut().storage, U64Key::new(27))
            .unwrap();
        let expected_pool27_pointers = vec![
            DecCoin::new(
                Decimal::from_ratio(16_u128, 640_u128),
                "uair1"
            ),
            DecCoin::new(
                Decimal::from_ratio(80_u128, 640_u128),
                "uair2"
            ),
        ];
        assert!(check_equal_deccoin_vector(&pool_27.airdrops_pointer, &expected_pool27_pointers));
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
            mock_info("creator", &[Coin::new(14, "utest")]),
            initial_msg.clone(),
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        let mut expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "utest".to_string(),
            delegator_contract: Addr::unchecked("delegator_addr"),
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
            vault_denom: "utest".to_string(),
            delegator_contract: Addr::unchecked("new_delegator_addr"),
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
            &[Coin::new(1200, "utest")],
        );
        let denom = "abc".to_string();
        let airdrop_contract = Addr::unchecked("def".to_string());
        let token_contract = Addr::unchecked("efg".to_string());

        // Expects a manager to call
        let err = execute(
            deps.as_mut(),
            env.clone(),
            other_info.clone(),
            ExecuteMsg::UpdateAirdropRegistry {
                airdrop_token: denom.clone(),
                airdrop_contract: airdrop_contract.clone(),
                cw20_contract: token_contract.clone(),
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
                airdrop_contract: airdrop_contract.clone(),
                cw20_contract: token_contract.clone(),
            },
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let airdrop_registry_info = AIRDROP_REGISTRY
            .may_load(deps.as_mut().storage, denom.clone())
            .unwrap();
        assert!(airdrop_registry_info.is_some());

        let info = airdrop_registry_info.unwrap();
        assert_eq!(info.airdrop_contract, airdrop_contract.clone());
        assert_eq!(info.cw20_contract, token_contract.clone());
    }
}
