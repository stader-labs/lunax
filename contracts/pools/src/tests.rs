#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query, reply, MESSAGE_REPLY_SWAP_ID};
    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, QueryConfigResponse, InstantiateMsg, QueryMsg, QueryStateResponse};
    use crate::state::{Config, State, CONFIG, STATE, POOL_REGISTRY, BATCH_UNDELEGATION_REGISTRY, PoolRegistryInfo, BatchUndelegationRecord, ValInfo, VALIDATOR_REGISTRY, AirdropRate, ConfigUpdateRequest};
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
        MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{from_binary, to_binary, Addr, Coin, ContractResult, Decimal, Env, Event, FullDelegation, MessageInfo, OwnedDeps, Reply, Response, SubMsg, SubMsgExecutionResponse, Uint128, Validator, WasmMsg, StdResult, attr};
    use stader_utils::coin_utils::{DecCoin, check_equal_deccoin_vector};
    use terra_cosmwasm::TerraMsgWrapper;
    use cw_storage_plus::U64Key;
    use delegator::msg::ExecuteMsg as DelegatorMsg;
    use validator::msg::ExecuteMsg as ValidatorMsg;
    use stader_utils::event_constants::{EVENT_SWAP_KEY_AMOUNT, EVENT_KEY_IDENTIFIER, EVENT_SWAP_TYPE};

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
            validator_contract: Addr::unchecked("validator_addr"),
            delegator_contract: Addr::unchecked("delegator_addr"),
            unbonding_period: None,
            unbonding_buffer: None,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000)
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            vault_denom: "utest".to_string(),
            validator_contract: Addr::unchecked("validator_addr"),
            delegator_contract: Addr::unchecked("delegator_addr"),
            unbonding_period: None,
            unbonding_buffer: None,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000)
        };
        let expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "utest".to_string(),
            validator_contract: Addr::unchecked("validator_addr"),
            delegator_contract: Addr::unchecked("delegator_addr"),
            unbonding_period: 3600 * 24 * 21,
            unbonding_buffer: 3600,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000)
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
                name: "Community Validator".to_string()
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(100, "utest")]),
            ExecuteMsg::AddPool {
                name: "Community Validator".to_string()
            },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        assert!(POOL_REGISTRY.may_load(deps.as_mut().storage, U64Key::new(0)).unwrap().is_none());
        assert!(BATCH_UNDELEGATION_REGISTRY.may_load(deps.as_mut().storage,
                                                     (U64Key::new(0), U64Key::new(1))).unwrap().is_none());

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddPool {
                name: "Community Pro".to_string()
            },
        ).unwrap();

        assert!(res.messages.is_empty());
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.next_pool_id, 1_u64);

        let pool_meta = POOL_REGISTRY.may_load(deps.as_mut().storage, U64Key::new(0)).unwrap().unwrap();
        let expected_pool_info = PoolRegistryInfo {
            name: "Community Pro".to_string(),
            active: true,
            validators: vec![],
            staked: Default::default(),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0,
        };
        assert!(pool_meta.eq(&expected_pool_info));

        let batch_undelegation_info = BATCH_UNDELEGATION_REGISTRY.may_load(deps.as_mut().storage,
                                                                           (U64Key::new(0), U64Key::new(1))).unwrap().unwrap();
        let expected_batch_undelegation = BatchUndelegationRecord {
            amount: Uint128::zero(),
            create_time: env.block.time,
            est_release_time: None,
            withdrawable_time: None
        };
        assert!(batch_undelegation_info.eq(&expected_batch_undelegation));
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
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(100, "utest")]),
            ExecuteMsg::AddValidator {
                val_addr: valid1.clone(),
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        // let err = execute(
        //     deps.as_mut(),
        //     env.clone(),
        //     mock_info("creator", &[]),
        //     ExecuteMsg::AddValidator {
        //         val_addr: Addr::unchecked("valid0004"),
        //         pool_id: 12
        //     },
        // ).unwrap_err();
        // assert!(matches!(err, ContractError::ValidatorNotDiscoverable {}));

        VALIDATOR_REGISTRY.save(deps.as_mut().storage, &valid1.clone(), &ValInfo {
            pool_id: 19,
            staked: Uint128::zero()
        }).unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                val_addr: valid1.clone(),
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::ValidatorAssociatedToPool {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                val_addr: valid2.clone(),
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::PoolNotFound {}));

        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(12), &PoolRegistryInfo {
            name: "Random name".to_string(),
            active: false,
            validators: vec![],
            staked: Default::default(),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 0,
            last_reconciled_batch_id: 0
        }).unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                val_addr: valid2.clone(),
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::PoolInactive {}));

        POOL_REGISTRY.update(deps.as_mut().storage, U64Key::new(12), |x| -> StdResult<_> {
            let mut y = x.unwrap();
            y.active = true;
            Ok(y)
        }).unwrap();
        assert!(VALIDATOR_REGISTRY.may_load(deps.as_mut().storage, &valid2.clone()).unwrap().is_none());
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddValidator {
                val_addr: valid2.clone(),
                pool_id: 12
            },
        ).unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0], SubMsg::new(WasmMsg::Execute {
            contract_addr: "validator_addr".to_string(),
            msg: to_binary(&ValidatorMsg::AddValidator {
                val_addr: valid2.clone(),
            }).unwrap(),
            funds: vec![]
        }));

        let pool_meta = POOL_REGISTRY.may_load(deps.as_mut().storage, U64Key::new(12)).unwrap().unwrap();
        let expected_pool_info = PoolRegistryInfo {
            name: "Random name".to_string(),
            active: true,
            validators: vec![valid2.clone()],
            staked: Default::default(),
            rewards_pointer: Default::default(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 0,
            last_reconciled_batch_id: 0,
        };
        assert!(pool_meta.eq(&expected_pool_info));

        let val_info = VALIDATOR_REGISTRY.may_load(deps.as_mut().storage, &valid2.clone()).unwrap().unwrap();
        let expected_val_info = ValInfo {
            pool_id: 12,
            staked: Default::default()
        };
        assert!(val_info.eq(&expected_val_info));
    }

    #[test]
    fn test_deposit_to_pool() {
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
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[]),
            ExecuteMsg::Deposit {
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::NoFunds {}));

        VALIDATOR_REGISTRY.save(deps.as_mut().storage, &valid1.clone(), &ValInfo {
            pool_id: 12,
            staked: Uint128::new(100)
        }).unwrap();
        VALIDATOR_REGISTRY.save(deps.as_mut().storage, &valid2.clone(), &ValInfo {
            pool_id: 12,
            staked: Uint128::new(150)
        }).unwrap();
        VALIDATOR_REGISTRY.save(deps.as_mut().storage, &valid3.clone(), &ValInfo {
            pool_id: 12,
            staked: Uint128::new(200)
        }).unwrap();
        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(12), &PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(450),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0
        }).unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(30, "utest")]),
            ExecuteMsg::Deposit {
                pool_id: 12
            },
        ).unwrap();

        assert_eq!(res.messages.len(), 2);
        assert_eq!(res.messages[0], SubMsg::new(WasmMsg::Execute {
            contract_addr: "delegator_addr".to_string(),
            msg: to_binary(&DelegatorMsg::Deposit {
                user_addr: user1.clone(),
                amount: Uint128::new(30),
                pool_id: 12,
                pool_rewards_pointer: Decimal::zero(),
                pool_airdrops_pointer: vec![]
            }).unwrap(),
            funds: vec![]
        }));
        assert_eq!(res.messages[1], SubMsg::new(WasmMsg::Execute {
            contract_addr: "validator_addr".to_string(),
            msg: to_binary(&ValidatorMsg::Stake {
                val_addr: valid1.clone(),
            }).unwrap(),
            funds: vec![Coin::new(30, "utest")]
        }));

        let pool_meta = POOL_REGISTRY.load(deps.as_mut().storage, U64Key::new(12)).unwrap();
        assert_eq!(pool_meta.staked, Uint128::new(480));

        let val_meta = VALIDATOR_REGISTRY.load(deps.as_mut().storage, &valid1.clone()).unwrap();
        assert_eq!(val_meta.staked, Uint128::new(130));
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
            ExecuteMsg::RedeemRewards {
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(228, "utest")]),
            ExecuteMsg::RedeemRewards {
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(12), &PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(450),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0
        }).unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemRewards {
                pool_id: 12
            },
        ).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0], SubMsg::new(WasmMsg::Execute {
            contract_addr: "validator_addr".to_string(),
            msg: to_binary(&ValidatorMsg::RedeemRewards {
                validators: vec![valid1.clone(), valid2.clone(), valid3.clone()]
            }).unwrap(),
            funds: vec![]
        }));
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
            ExecuteMsg::Swap {
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(228, "utest")]),
            ExecuteMsg::Swap {
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(12), &PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(450),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0
        }).unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Swap {
                pool_id: 12
            },
        ).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0], SubMsg::reply_always(WasmMsg::Execute {
            contract_addr: "validator_addr".to_string(),
            msg: to_binary(&ValidatorMsg::SwapAndTransfer {
                validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
                identifier: "12".to_string()
            }).unwrap(),
            funds: vec![]
        }, 0));
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
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[Coin::new(10, "utest")]),
            ExecuteMsg::QueueUndelegate {
                pool_id: 12,
                amount: Uint128::new(20)
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        VALIDATOR_REGISTRY.save(deps.as_mut().storage, &valid1.clone(), &ValInfo {
            pool_id: 12,
            staked: Uint128::new(100)
        }).unwrap();
        VALIDATOR_REGISTRY.save(deps.as_mut().storage, &valid2.clone(), &ValInfo {
            pool_id: 12,
            staked: Uint128::new(150)
        }).unwrap();
        VALIDATOR_REGISTRY.save(deps.as_mut().storage, &valid3.clone(), &ValInfo {
            pool_id: 12,
            staked: Uint128::new(200)
        }).unwrap();
        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(12), &PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(450),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0
        }).unwrap();
        BATCH_UNDELEGATION_REGISTRY.save(deps.as_mut().storage, (U64Key::new(12), U64Key::new(1)), &BatchUndelegationRecord {
            amount: Uint128::new(40),
            create_time: env.block.time,
            est_release_time: None,
            withdrawable_time: None
        }).unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&user1.clone().to_string(), &[]),
            ExecuteMsg::QueueUndelegate {
                pool_id: 12,
                amount: Uint128::new(20)
            },
        ).unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0], SubMsg::new(WasmMsg::Execute {
            contract_addr: "delegator_addr".to_string(),
            msg: to_binary(&DelegatorMsg::Undelegate {
                user_addr: user1.clone(),
                batch_id: 1,
                from_pool: 12,
                amount: Uint128::new(20),
                pool_rewards_pointer: Decimal::zero(),
                pool_airdrops_pointer: vec![]
            }).unwrap(),
            funds: vec![]
        }));

        let pool_meta = POOL_REGISTRY.load(deps.as_mut().storage, U64Key::new(12)).unwrap();
        assert_eq!(pool_meta.staked, Uint128::new(430));

        let batch_und = BATCH_UNDELEGATION_REGISTRY.load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(1))).unwrap();
        assert_eq!(batch_und.amount, Uint128::new(60));
    }

    #[test]
    fn test_undelegate_from_pool() {
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
            ExecuteMsg::Undelegate {
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(228, "utest")]),
            ExecuteMsg::Swap {
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        assert!(BATCH_UNDELEGATION_REGISTRY.may_load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(2))).unwrap().is_none());
        VALIDATOR_REGISTRY.save(deps.as_mut().storage, &valid1.clone(), &ValInfo {
            pool_id: 12,
            staked: Uint128::new(100)
        }).unwrap();
        VALIDATOR_REGISTRY.save(deps.as_mut().storage, &valid2.clone(), &ValInfo {
            pool_id: 12,
            staked: Uint128::new(150)
        }).unwrap();
        VALIDATOR_REGISTRY.save(deps.as_mut().storage, &valid3.clone(), &ValInfo {
            pool_id: 12,
            staked: Uint128::new(200)
        }).unwrap();
        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(12), &PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(450),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 1,
            last_reconciled_batch_id: 0
        }).unwrap();
        BATCH_UNDELEGATION_REGISTRY.save(deps.as_mut().storage, (U64Key::new(12), U64Key::new(1)), &BatchUndelegationRecord {
            amount: Uint128::new(0),
            create_time: env.block.time,
            est_release_time: None,
            withdrawable_time: None
        }).unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Undelegate {
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::NoOp {}));

        BATCH_UNDELEGATION_REGISTRY.save(deps.as_mut().storage, (U64Key::new(12), U64Key::new(1)), &BatchUndelegationRecord {
            amount: Uint128::new(40),
            create_time: env.block.time,
            est_release_time: None,
            withdrawable_time: None
        }).unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Undelegate {
                pool_id: 12
            },
        ).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0], SubMsg::new(WasmMsg::Execute {
            contract_addr: "validator_addr".to_string(),
            msg: to_binary(&ValidatorMsg::Undelegate {
                val_addr: valid3.clone(),
                amount: Uint128::new(40),
            }).unwrap(),
            funds: vec![]
        }));

        let pool_meta = POOL_REGISTRY.load(deps.as_mut().storage, U64Key::new(12)).unwrap();
        assert_eq!(pool_meta.staked, Uint128::new(450)); // Staked amount is deducted when user undelegates in the first place
        assert_eq!(pool_meta.current_undelegation_batch_id, 2);

        let batch_und = BATCH_UNDELEGATION_REGISTRY.load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(1))).unwrap();
        assert_eq!(batch_und.amount, Uint128::new(40));
        assert_eq!(batch_und.est_release_time.unwrap(), env.block.time.plus_seconds(21 * 24 * 3600));
        assert_eq!(batch_und.withdrawable_time.unwrap(), env.block.time.plus_seconds(21 * 24 * 3600 + 3600));

        assert!(BATCH_UNDELEGATION_REGISTRY.may_load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(2))).unwrap().is_some());

        let val_meta = VALIDATOR_REGISTRY.load(deps.as_mut().storage, &valid3.clone()).unwrap();
        assert_eq!(val_meta.staked, Uint128::new(160));

        // Undelegate from multiple validators
        BATCH_UNDELEGATION_REGISTRY.save(deps.as_mut().storage, (U64Key::new(12), U64Key::new(2)), &BatchUndelegationRecord {
            amount: Uint128::new(200),
            create_time: env.block.time,
            est_release_time: None,
            withdrawable_time: None
        }).unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Undelegate {
                pool_id: 12
            },
        ).unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(res.messages[0], SubMsg::new(WasmMsg::Execute {
            contract_addr: "validator_addr".to_string(),
            msg: to_binary(&ValidatorMsg::Undelegate {
                val_addr: valid3.clone(),
                amount: Uint128::new(160),
            }).unwrap(),
            funds: vec![]
        }));
        assert_eq!(res.messages[1], SubMsg::new(WasmMsg::Execute {
            contract_addr: "validator_addr".to_string(),
            msg: to_binary(&ValidatorMsg::Undelegate {
                val_addr: valid2.clone(),
                amount: Uint128::new(40),
            }).unwrap(),
            funds: vec![]
        }));
        let val_meta = VALIDATOR_REGISTRY.load(deps.as_mut().storage, &valid3.clone()).unwrap();
        assert_eq!(val_meta.staked, Uint128::new(0));

        let val_meta = VALIDATOR_REGISTRY.load(deps.as_mut().storage, &valid2.clone()).unwrap();
        assert_eq!(val_meta.staked, Uint128::new(110));

        let pool_meta = POOL_REGISTRY.load(deps.as_mut().storage, U64Key::new(12)).unwrap();
        assert_eq!(pool_meta.staked, Uint128::new(450));

        assert_eq!(res.attributes[0], attr("Undelegation_pool_id", "12"));
        assert_eq!(res.attributes[1], attr("Undelegation_amount", "200"));
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
            ExecuteMsg::ReconcileFunds {
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(228, "utest")]),
            ExecuteMsg::ReconcileFunds {
                pool_id: 12
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(12), &PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(450),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![],
            current_undelegation_batch_id: 9,
            last_reconciled_batch_id: 5
        }).unwrap();
        BATCH_UNDELEGATION_REGISTRY.save(deps.as_mut().storage, (U64Key::new(12), U64Key::new(6)), &BatchUndelegationRecord {
            amount: Uint128::new(10),
            create_time: env.block.time,
            est_release_time: Some(timestamp_now),
            withdrawable_time: Some(timestamp_now)
        }).unwrap();
        BATCH_UNDELEGATION_REGISTRY.save(deps.as_mut().storage, (U64Key::new(12), U64Key::new(6)), &BatchUndelegationRecord {
            amount: Uint128::new(20),
            create_time: env.block.time,
            est_release_time: Some(timestamp_now),
            withdrawable_time: Some(timestamp_now)
        }).unwrap();
        BATCH_UNDELEGATION_REGISTRY.save(deps.as_mut().storage, (U64Key::new(12), U64Key::new(7)), &BatchUndelegationRecord {
            amount: Uint128::new(70),
            create_time: env.block.time,
            est_release_time: Some(timestamp_now),
            withdrawable_time: Some(timestamp_now)
        }).unwrap();
        BATCH_UNDELEGATION_REGISTRY.save(deps.as_mut().storage, (U64Key::new(12), U64Key::new(8)), &BatchUndelegationRecord {
            amount: Uint128::new(120),
            create_time: env.block.time,
            est_release_time: Some(timestamp_now.plus_seconds(1)),
            withdrawable_time: Some(timestamp_now.plus_seconds(1))
        }).unwrap();
        BATCH_UNDELEGATION_REGISTRY.save(deps.as_mut().storage, (U64Key::new(12), U64Key::new(9)), &BatchUndelegationRecord {
            amount: Uint128::new(50),
            create_time: env.block.time,
            est_release_time: Some(timestamp_now.plus_seconds(1)),
            withdrawable_time: Some(timestamp_now.plus_seconds(1))
        }).unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::ReconcileFunds {
                pool_id: 12
            },
        ).unwrap();
        assert_eq!(res.messages[0], SubMsg::new(WasmMsg::Execute {
            contract_addr: "validator_addr".to_string(),
            msg: to_binary(&ValidatorMsg::TransferReconciledFunds {
                amount: Uint128::new(90),
            }).unwrap(),
            funds: vec![]
        }));

        let pool_meta = POOL_REGISTRY.load(deps.as_mut().storage, U64Key::new(12)).unwrap();
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
                amount: Uint128::new(45)
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[]),
            ExecuteMsg::WithdrawFundsToWallet {
                pool_id: 12,
                batch_id: 9,
                undelegate_id: 21,
                amount: Uint128::new(45)
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::UndelegationBatchNotFound {}));

        BATCH_UNDELEGATION_REGISTRY.save(deps.as_mut().storage, (U64Key::new(12), U64Key::new(9)), &BatchUndelegationRecord {
            amount: Uint128::new(500),
            create_time: env.block.time.minus_seconds(2),
            est_release_time: Some(env.block.time.plus_seconds(1)),
            withdrawable_time: Some(env.block.time.plus_seconds(2))
        }).unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[]),
            ExecuteMsg::WithdrawFundsToWallet {
                pool_id: 12,
                batch_id: 9,
                undelegate_id: 21,
                amount: Uint128::new(45)
            },
        ).unwrap_err();
        assert!(matches!(err, ContractError::UndelegationNotWithdrawable {}));

        BATCH_UNDELEGATION_REGISTRY.save(deps.as_mut().storage, (U64Key::new(12), U64Key::new(9)), &BatchUndelegationRecord {
            amount: Uint128::new(500),
            create_time: env.block.time.minus_seconds(2),
            est_release_time: Some(env.block.time),
            withdrawable_time: Some(env.block.time)
        }).unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[]),
            ExecuteMsg::WithdrawFundsToWallet {
                pool_id: 12,
                batch_id: 9,
                undelegate_id: 21,
                amount: Uint128::new(45)
            },
        ).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(res.messages[0], SubMsg::new(WasmMsg::Execute {
            contract_addr: "delegator_addr".to_string(),
            msg: to_binary(&DelegatorMsg::WithdrawFunds {
                user_addr: Addr::unchecked("any"),
                pool_id: 12,
                undelegate_id: 21,
                amount: Uint128::new(45),
            }).unwrap(),
            funds: vec![]
        }));

        let batch_und_meta = BATCH_UNDELEGATION_REGISTRY.load(deps.as_mut().storage, (U64Key::new(12), U64Key::new(9))).unwrap();
        assert_eq!(batch_und_meta.amount, Uint128::new(455));
    }

    #[test]
    fn test_update_airdrop_pointers() {
        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let valid3 = Addr::unchecked("valid0003");
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        instantiate_contract(&mut deps, &info, &env, None);

        let execute_msg = ExecuteMsg::UpdateAirdropPointers {
            airdrop_amount: Default::default(),
            rates: vec![]
        };
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("any", &[Coin::new(228, "utest")]),
            execute_msg.clone()
        ).unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(228, "utest")]),
            execute_msg.clone()
        ).unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            execute_msg.clone()
        ).unwrap_err();
        assert!(matches!(err, ContractError::ZeroAmount {}));


        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(12), &PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(280),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![DecCoin::new(Decimal::from_ratio(28_u128, 280_u128), "uair1")],
            current_undelegation_batch_id: 9,
            last_reconciled_batch_id: 5
        }).unwrap();
        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(27), &PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(640),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![DecCoin::new(Decimal::from_ratio(28_u128, 280_u128), "uair2")],
            current_undelegation_batch_id: 9,
            last_reconciled_batch_id: 5
        }).unwrap();
        let execute_msg = ExecuteMsg::UpdateAirdropPointers {
            airdrop_amount: Uint128::new(20),
            rates: vec![AirdropRate {
                pool_id: 12,
                denom: "uair1".to_string(),
                amount: Uint128::new(14)
            }, AirdropRate {
                pool_id: 27,
                denom: "uair1".to_string(),
                amount: Uint128::new(16)
            }]
        };
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            execute_msg.clone()
        ).unwrap_err();
        println!("ErrMsg {:?}", err);
        assert!(matches!(err, ContractError::MismatchingAmounts {}));

        // IN Tests the updates are persistent because there's no rollback.
        let pool_12 = POOL_REGISTRY.load(deps.as_mut().storage, U64Key::new(12)).unwrap();
        assert_eq!(pool_12.airdrops_pointer, vec![DecCoin::new(Decimal::from_ratio(15_u128, 100_u128), "uair1")]);

        let pool_27 = POOL_REGISTRY.load(deps.as_mut().storage, U64Key::new(27)).unwrap();
        assert!(check_equal_deccoin_vector(&pool_27.airdrops_pointer, &vec![
            DecCoin::new(Decimal::from_ratio(1_u128, 40_u128), "uair1"),
            DecCoin::new(Decimal::from_ratio(28_u128, 280_u128), "uair2")
        ]));

        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(12), &PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(280),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![DecCoin::new(Decimal::from_ratio(28_u128, 280_u128), "uair1")],
            current_undelegation_batch_id: 9,
            last_reconciled_batch_id: 5
        }).unwrap();
        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(27), &PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validators: vec![valid1.clone(), valid2.clone(), valid3.clone()],
            staked: Uint128::new(640),
            rewards_pointer: Decimal::zero(),
            airdrops_pointer: vec![DecCoin::new(Decimal::from_ratio(28_u128, 280_u128), "uair2")],
            current_undelegation_batch_id: 9,
            last_reconciled_batch_id: 5
        }).unwrap();

        let execute_msg = ExecuteMsg::UpdateAirdropPointers {
            airdrop_amount: Uint128::new(30),
            rates: vec![AirdropRate {
                pool_id: 12,
                denom: "uair1".to_string(),
                amount: Uint128::new(14)
            }, AirdropRate {
                pool_id: 27,
                denom: "uair1".to_string(),
                amount: Uint128::new(16)
            }]
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            execute_msg.clone()
        ).unwrap();
        assert!(res.messages.is_empty());

        let pool_12 = POOL_REGISTRY.load(deps.as_mut().storage, U64Key::new(12)).unwrap();
        assert_eq!(pool_12.airdrops_pointer, vec![DecCoin::new(Decimal::from_ratio(15_u128, 100_u128), "uair1")]);

        let pool_27 = POOL_REGISTRY.load(deps.as_mut().storage, U64Key::new(27)).unwrap();
        assert!(check_equal_deccoin_vector(&pool_27.airdrops_pointer, &vec![
            DecCoin::new(Decimal::from_ratio(1_u128, 40_u128), "uair1"),
            DecCoin::new(Decimal::from_ratio(28_u128, 280_u128), "uair2")
        ]));
    }

    #[test]
    fn test_reply_swap() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        instantiate_contract(&mut deps, &info, &env, None);

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");

        POOL_REGISTRY.save(deps.as_mut().storage, U64Key::new(12), &PoolRegistryInfo {
            name: "RandomName".to_string(),
            active: true,
            validators: vec![valid1.clone(), valid2.clone()],
            staked: Uint128::new(2800),
            rewards_pointer: Decimal::from_ratio(18_u128, 100_u128),
            airdrops_pointer: vec![DecCoin::new(Decimal::from_ratio(28_u128, 280_u128), "uair1")],
            current_undelegation_batch_id: 9,
            last_reconciled_batch_id: 5
        }).unwrap();

        let res =
            reply(
                deps.as_mut(),
                env,
                Reply {
                    id: MESSAGE_REPLY_SWAP_ID,
                    result:
                    ContractResult::Ok(
                        SubMsgExecutionResponse {
                            events:
                            vec![
                                Event::new(format!("wasm-{}", EVENT_SWAP_TYPE)) // Events are automatically prepended with a `wasm-`
                                    .add_attribute(
                                        EVENT_SWAP_KEY_AMOUNT,
                                        "1400",
                                    )
                                    .add_attribute(
                                        EVENT_KEY_IDENTIFIER,
                                        "12",
                                    ),
                            ],
                            data: None,
                        },
                    ),
                },
            ).unwrap();
        assert!(res.messages.is_empty());
        let pool_12 = POOL_REGISTRY.load(deps.as_mut().storage, U64Key::new(12)).unwrap();
        assert_eq!(pool_12.rewards_pointer, Decimal::from_ratio(68_u128, 100_u128));
    }

    #[test]
    fn test_update_config() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_msg = ExecuteMsg::UpdateConfig {
            config_request: ConfigUpdateRequest {
                validator_contract: None,
                delegator_contract: None,
                min_deposit: None,
                max_deposit: None,
                unbonding_period: None,
                unbonding_buffer: None
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
            validator_contract: Addr::unchecked("validator_addr"),
            delegator_contract: Addr::unchecked("delegator_addr"),
            unbonding_period: 1814400,
            unbonding_buffer: 3600,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000)
        };
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config, expected_config);

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            initial_msg.clone(),
        ).unwrap();
        assert!(res.messages.is_empty());
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config, expected_config);

        expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "utest".to_string(),
            validator_contract: Addr::unchecked("new_validator_addr"),
            delegator_contract: Addr::unchecked("new_delegator_addr"),
            unbonding_period: 1814401,
            unbonding_buffer: 3601,
            min_deposit: Uint128::new(1001),
            max_deposit: Uint128::new(1_000_000_000_001)
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                config_request: ConfigUpdateRequest {
                    validator_contract: Some(Addr::unchecked("new_validator_addr")),
                    delegator_contract: Some(Addr::unchecked("new_delegator_addr")),
                    min_deposit: Some(Uint128::new(1001)),
                    max_deposit: Some(Uint128::new(1_000_000_000_001)),
                    unbonding_period: Some(1814401),
                    unbonding_buffer: Some(3601)
                },
            }.clone(),
        ).unwrap();
        assert!(res.messages.is_empty());
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config, expected_config);
    }
}
