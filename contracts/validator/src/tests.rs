#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query, reply};
    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, GetConfigResponse, InstantiateMsg, QueryMsg};
    use crate::operations::{
        EVENT_REDELEGATE_ID, EVENT_REDELEGATE_KEY_DST_ADDR, EVENT_REDELEGATE_KEY_SRC_ADDR,
        EVENT_REDELEGATE_TYPE,
    };
    use crate::state::{
        AirdropRegistryInfo, Config, State, VMeta, AIRDROP_REGISTRY, CONFIG, STATE,
        VALIDATOR_REGISTRY,
    };
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
        MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{
        from_binary, to_binary, Addr, Attribute, BankMsg, Binary, Coin, ContractResult,
        Decimal, DistributionMsg, Env, Event, FullDelegation, MessageInfo, OwnedDeps, Reply,
        Response, StakingMsg, SubMsg, SubMsgExecutionResponse, Uint128, Validator, WasmMsg,
    };
    use cw20::Cw20ExecuteMsg;
    use stader_utils::coin_utils::check_equal_coin_vector;
    use terra_cosmwasm::TerraMsgWrapper;

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
            delegator_contract: Addr::unchecked("delegator_addr"),
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
            delegator_contract: Addr::unchecked("delegator_addr"),
        };
        let expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "utest".to_string(),
            pools_contract: Addr::unchecked("pools_address"),
            scc_contract: Addr::unchecked("scc_addr"),
            delegator_contract: Addr::unchecked("delegator_addr"),
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
    fn test_add_validator() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let valid1 = Addr::unchecked("valid0001");

        instantiate_contract(&mut deps, &info, &env, None);

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        assert!(VALIDATOR_REGISTRY
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap()
            .is_none());

        let err = execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddValidator {
                val_addr: valid1.clone(),
            },
        )
            .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let pools_info = mock_info("pools_addr", &[]);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::AddValidator {
                val_addr: valid1.clone(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);

        let valid1_meta = VALIDATOR_REGISTRY
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert!(valid1_meta.is_some());
        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::AddValidator {
                val_addr: valid1.clone(),
            },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::ValidatorAlreadyExists {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::AddValidator {
                val_addr: Addr::unchecked("valid0004").clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotDiscoverable {}));
    }

    #[test]
    fn test_stake() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let valid1 = Addr::unchecked("valid0001");
        let pools_addr = Addr::unchecked("pools_addr");

        instantiate_contract(&mut deps, &info, &env, None);

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::Stake {
                val_addr: valid1.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let pools_info = mock_info(&pools_addr.to_string(), &[Coin::new(1200, "utest")]);

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &pools_addr.to_string(),
                &[Coin::new(1200, "utest"), Coin::new(1000, "othercoin")],
            ),
            ExecuteMsg::Stake {
                val_addr: valid1.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::MultipleFunds {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Stake {
                val_addr: Addr::unchecked("valid0004").clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        let initial_accrued_rewards = vec![Coin::new(123, "utest")];
        VALIDATOR_REGISTRY
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Default::default(),
                    accrued_rewards: initial_accrued_rewards.clone(),
                },
            )
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Stake {
                val_addr: valid1.clone(),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(StakingMsg::Delegate {
                validator: valid1.to_string(),
                amount: Coin::new(1200, "utest")
            })
        );
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert!(check_equal_coin_vector(
            &state.unswapped_rewards,
            &vec![Coin::new(20, "utest"), Coin::new(30, "urew1")]
        ));

        let valid1_meta = VALIDATOR_REGISTRY
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert!(valid1_meta.is_some());
        let valid1_meta_unwrapped = valid1_meta.unwrap();
        assert!(check_equal_coin_vector(
            &valid1_meta_unwrapped.accrued_rewards,
            &vec![Coin::new(143, "utest"), Coin::new(30, "urew1")]
        ));
        assert_eq!(valid1_meta_unwrapped.staked, Uint128::new(1200));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Stake {
                val_addr: valid1.clone(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        let valid1_meta = VALIDATOR_REGISTRY
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert!(valid1_meta.is_some());
        let valid1_meta_unwrapped = valid1_meta.unwrap();
        assert!(check_equal_coin_vector(
            &valid1_meta_unwrapped.accrued_rewards,
            &vec![Coin::new(163, "utest"), Coin::new(60, "urew1")]
        )); // Accrued rewards remains unchanged
        assert_eq!(valid1_meta_unwrapped.staked, Uint128::new(2400)); // Adds to previous staked amount.
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert!(check_equal_coin_vector(
            &state.unswapped_rewards,
            &vec![Coin::new(40, "utest"), Coin::new(60, "urew1")]
        ));
    }

    #[test]
    fn test_redeem_rewards() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let pools_addr = Addr::unchecked("pools_addr");

        instantiate_contract(&mut deps, &info, &env, None);

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        STATE
            .save(
                deps.as_mut().storage,
                &State {
                    unswapped_rewards: vec![],
                    slashing_funds: Uint128::new(200),
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::Stake {
                val_addr: valid1.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        assert!(VALIDATOR_REGISTRY
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap()
            .is_none());

        let pools_info = mock_info(&pools_addr.to_string(), &[Coin::new(1200, "utest")]);
        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::RedeemRewards {
                validators: vec![valid1.clone()],
            },
        )
        .unwrap();
        assert!(res.messages.is_empty());
        assert_eq!(
            res.attributes,
            [Attribute {
                key: "failed_validators".to_string(),
                value: "valid0001".to_string()
            }]
        );

        let initial_accrued_rewards = vec![Coin::new(123, "utest")];
        VALIDATOR_REGISTRY
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(1100),
                    accrued_rewards: initial_accrued_rewards.clone(),
                },
            )
            .unwrap();
        VALIDATOR_REGISTRY
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(1050),
                    accrued_rewards: initial_accrued_rewards.clone(),
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::RedeemRewards {
                validators: vec![valid1.clone(), valid2.clone()],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 4);
        assert_eq!(
            res.messages[0],
            SubMsg::new(StakingMsg::Delegate {
                validator: valid1.to_string(),
                amount: Coin::new(100, "utest")
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                validator: valid1.to_string()
            })
        );
        assert_eq!(
            res.messages[2],
            SubMsg::new(StakingMsg::Delegate {
                validator: valid2.to_string(),
                amount: Coin::new(50, "utest")
            })
        );
        assert_eq!(
            res.messages[3],
            SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                validator: valid2.to_string()
            })
        );

        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.slashing_funds, Uint128::new(50));
        assert!(check_equal_coin_vector(
            &state.unswapped_rewards,
            &vec![Coin::new(60, "utest"), Coin::new(90, "urew1")]
        ));
    }

    #[test]
    fn test_redelegate() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        let pools_addr = Addr::unchecked("pools_addr");

        instantiate_contract(&mut deps, &info, &env, None);

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::Redelegate {
                src: valid1.clone(),
                dst: valid2.clone(),
                amount: Uint128::new(150),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        assert!(VALIDATOR_REGISTRY
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap()
            .is_none());
        assert!(VALIDATOR_REGISTRY
            .may_load(deps.as_mut().storage, &valid2)
            .unwrap()
            .is_none());
        let pools_info = mock_info(&pools_addr.to_string(), &[Coin::new(1200, "utest")]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Redelegate {
                src: valid1.clone(),
                dst: valid2.clone(),
                amount: Uint128::new(150),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        VALIDATOR_REGISTRY
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(100),
                    accrued_rewards: vec![Coin::new(123, "utest"), Coin::new(167, "urew1")],
                },
            )
            .unwrap();
        let pools_info = mock_info(&pools_addr.to_string(), &[Coin::new(1200, "utest")]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Redelegate {
                src: valid1.clone(),
                dst: valid2.clone(),
                amount: Uint128::new(150),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        VALIDATOR_REGISTRY
            .save(
                deps.as_mut().storage,
                &valid2,
                &VMeta {
                    staked: Uint128::new(800),
                    accrued_rewards: vec![Coin::new(10, "utest"), Coin::new(30, "urew1")],
                },
            )
            .unwrap();
        let pools_info = mock_info(&pools_addr.to_string(), &[Coin::new(1200, "utest")]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Redelegate {
                src: valid1.clone(),
                dst: valid2.clone(),
                amount: Uint128::new(150),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InSufficientFunds {}));

        STATE
            .save(
                deps.as_mut().storage,
                &State {
                    slashing_funds: Uint128::zero(),
                    unswapped_rewards: vec![Coin::new(10, "utest"), Coin::new(10, "urew1")],
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Redelegate {
                src: valid1.clone(),
                dst: valid2.clone(),
                amount: Uint128::new(15),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(StakingMsg::Redelegate {
                src_validator: valid1.to_string(),
                dst_validator: valid2.to_string(),
                amount: Coin::new(15, "utest")
            })
        );

        let valid1_meta = VALIDATOR_REGISTRY
            .load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert_eq!(valid1_meta.staked, Uint128::new(85));
        assert!(check_equal_coin_vector(
            &valid1_meta.accrued_rewards,
            &vec![Coin::new(143, "utest"), Coin::new(197, "urew1")]
        ));
        let valid2_meta = VALIDATOR_REGISTRY
            .load(deps.as_mut().storage, &valid2)
            .unwrap();
        assert_eq!(valid2_meta.staked, Uint128::new(815));
        assert!(check_equal_coin_vector(
            &valid2_meta.accrued_rewards,
            &vec![Coin::new(50, "utest"), Coin::new(90, "urew1")]
        ));

        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert!(check_equal_coin_vector(
            &state.unswapped_rewards,
            &vec![Coin::new(70, "utest"), Coin::new(100, "urew1")]
        ));
    }

    #[test]
    fn test_undelegate() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let valid1 = Addr::unchecked("valid0001");
        let pools_addr = Addr::unchecked("pools_addr");

        instantiate_contract(&mut deps, &info, &env, None);

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::Undelegate {
                val_addr: valid1.clone(),
                amount: Uint128::new(150),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        assert!(VALIDATOR_REGISTRY
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap()
            .is_none());
        let pools_info = mock_info(&pools_addr.to_string(), &[]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Undelegate {
                val_addr: valid1.clone(),
                amount: Uint128::new(150),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        VALIDATOR_REGISTRY
            .save(
                deps.as_mut().storage,
                &valid1,
                &VMeta {
                    staked: Uint128::new(100),
                    accrued_rewards: vec![Coin::new(123, "utest"), Coin::new(167, "urew1")],
                },
            )
            .unwrap();
        let pools_info = mock_info(&pools_addr.to_string(), &[]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Undelegate {
                val_addr: valid1.clone(),
                amount: Uint128::new(150),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InSufficientFunds {}));

        STATE
            .save(
                deps.as_mut().storage,
                &State {
                    slashing_funds: Uint128::new(127),
                    unswapped_rewards: vec![Coin::new(18, "utest"), Coin::new(26, "urew1")],
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Undelegate {
                val_addr: valid1.clone(),
                amount: Uint128::new(15),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(StakingMsg::Undelegate {
                validator: valid1.to_string(),
                amount: Coin::new(15, "utest")
            })
        );

        let valid1_meta = VALIDATOR_REGISTRY
            .load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert_eq!(valid1_meta.staked, Uint128::new(85));
        assert!(check_equal_coin_vector(
            &valid1_meta.accrued_rewards,
            &vec![Coin::new(143, "utest"), Coin::new(197, "urew1")]
        ));

        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.slashing_funds, Uint128::new(127));
        assert!(check_equal_coin_vector(
            &state.unswapped_rewards,
            &vec![Coin::new(38, "utest"), Coin::new(56, "urew1")]
        ));
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
                denom: denom.clone(),
                airdrop_contract: airdrop_contract.clone(),
                token_contract: token_contract.clone(),
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
                denom: denom.clone(),
                airdrop_contract: airdrop_contract.clone(),
                token_contract: token_contract.clone(),
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
        assert_eq!(info.token_contract, token_contract.clone());
    }

    #[test]
    fn test_redeem_airdrop() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        instantiate_contract(&mut deps, &info, &env, None);
        fn get_airdrop_claim_msg() -> Binary {
            Binary::from(vec![01, 02, 03, 04, 05, 06, 07, 08])
        }

        let anc_airdrop_contract = Addr::unchecked("anc_airdrop_contract".to_string());
        let mir_airdrop_contract = Addr::unchecked("mir_airdrop_contract".to_string());
        let anc_token_contract = Addr::unchecked("anc_token_contract".to_string());
        let mir_token_contract = Addr::unchecked("mir_token_contract".to_string());
        /*
           Test - 1. Only manager can update airdrops
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::RedeemAirdropAndTransfer {
                airdrop_token: "anc".to_string(),
                amount: Uint128::new(2000_u128),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. Airdrop not registered. Check failure.
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemAirdropAndTransfer {
                airdrop_token: "anc".to_string(),
                amount: Uint128::new(2000_u128),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::AirdropNotRegistered {}));

        // register airdrops
        AIRDROP_REGISTRY.save(
            deps.as_mut().storage,
            "anc".to_string(),
            &AirdropRegistryInfo {
                airdrop_contract: anc_airdrop_contract.clone(),
                token_contract: anc_token_contract.clone(),
            },
        ).unwrap();
        AIRDROP_REGISTRY.save(
            deps.as_mut().storage,
            "mir".to_string(),
            &AirdropRegistryInfo {
                airdrop_contract: mir_airdrop_contract.clone(),
                token_contract: mir_token_contract.clone(),
            },
        ).unwrap();

        /*
           Test - 3. First airdrops claim
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemAirdropAndTransfer {
                airdrop_token: "anc".to_string(),
                amount: Uint128::new(2000_u128),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: anc_airdrop_contract.clone().to_string(),
                msg: get_airdrop_claim_msg(),
                funds: vec![]
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: anc_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: Addr::unchecked("scc_addr").to_string(),
                    amount: Uint128::new(2000_u128),
                })
                .unwrap(),
                funds: vec![]
            })
        );

        /*
            Test - 4. MIR claim with ANC in pool
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemAirdropAndTransfer {
                airdrop_token: "mir".to_string(),
                amount: Uint128::new(1000_u128),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: mir_airdrop_contract.clone().to_string(),
                msg: get_airdrop_claim_msg(),
                funds: vec![]
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: mir_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: Addr::unchecked("scc_addr").to_string(),
                    amount: Uint128::new(1000_u128),
                })
                .unwrap(),
                funds: vec![]
            })
        );

        /*
           Test - 5. ANC claim with existing ANC
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemAirdropAndTransfer {
                airdrop_token: "anc".to_string(),
                amount: Uint128::new(2000_u128),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages[0],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: anc_airdrop_contract.clone().to_string(),
                msg: get_airdrop_claim_msg(),
                funds: vec![]
            })
        );
        assert_eq!(
            res.messages[1],
            SubMsg::new(WasmMsg::Execute {
                contract_addr: anc_token_contract.clone().to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: Addr::unchecked("scc_addr").to_string(),
                    amount: Uint128::new(2000_u128),
                })
                .unwrap(),
                funds: vec![]
            })
        );
    }

    #[test]
    fn test_add_slashing_funds() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        instantiate_contract(&mut deps, &info, &env, None);

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::AddSlashingFunds {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert!(state.slashing_funds.eq(&Uint128::zero()));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::AddSlashingFunds {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoFunds {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(1000, "utest")]),
            ExecuteMsg::AddSlashingFunds {},
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert!(state.slashing_funds.eq(&Uint128::new(1000_u128)));
        //
        // let res = execute(
        //     deps.as_mut(),
        //     env.clone(),
        //     mock_info("creator", &[]),
        //     ExecuteMsg::UpdateSlashingFunds { amount: -300 },
        // )
        // .unwrap();
        // let state = STATE.load(deps.as_mut().storage).unwrap();
        // assert!(state.slashing_funds.eq(&Uint128::new(700_u128)));
        //
        // let err = execute(
        //     deps.as_mut(),
        //     env.clone(),
        //     mock_info("creator", &[]),
        //     ExecuteMsg::UpdateSlashingFunds { amount: -701 },
        // )
        // .unwrap_err();
        // assert!(matches!(err, ContractError::InSufficientFunds {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(200, "utest")]),
            ExecuteMsg::AddSlashingFunds {},
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert!(state.slashing_funds.eq(&Uint128::new(1200_u128)));
    }

    #[test]
    fn test_remove_slashing_funds() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        instantiate_contract(&mut deps, &info, &env, None);

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::RemoveSlashingFunds {
                amount: Uint128::new(100),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(200, "utest")]),
            ExecuteMsg::RemoveSlashingFunds {
                amount: Uint128::new(100),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::FundsNotExpected {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveSlashingFunds {
                amount: Uint128::new(100),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InSufficientFunds {}));

        STATE
            .save(
                deps.as_mut().storage,
                &State {
                    slashing_funds: Uint128::new(500),
                    unswapped_rewards: vec![],
                },
            )
            .unwrap();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveSlashingFunds {
                amount: Uint128::new(100),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(BankMsg::Send {
                to_address: "creator".to_string(),
                amount: vec![Coin::new(100, "utest")]
            })
        );

        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert!(state.slashing_funds.eq(&Uint128::new(400_u128)));
    }

    #[test]
    fn test_remove_validator() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        instantiate_contract(&mut deps, &info, &env, None);

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redelegate_addr: valid2.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {})); // Expects manager to make the call

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redelegate_addr: valid2.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {})); // Expects manager to make the call

        VALIDATOR_REGISTRY.save(
            deps.as_mut().storage,
            &valid1,
            &VMeta {
                staked: Uint128::new(200),
                accrued_rewards: vec![Coin::new(100, "utest")],
            },
        ).unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redelegate_addr: valid2.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {})); // Expects manager to make the call

        VALIDATOR_REGISTRY.save(
            deps.as_mut().storage,
            &valid2,
            &VMeta {
                staked: Uint128::new(300),
                accrued_rewards: vec![Coin::new(50, "urew1")],
            },
        ).unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redelegate_addr: valid2.clone(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::reply_always(
                WasmMsg::Execute {
                    contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                    msg: to_binary(&ExecuteMsg::Redelegate {
                        src: valid1.clone(),
                        dst: valid2.clone(),
                        amount: Uint128::new(200),
                    })
                    .unwrap(),
                    funds: vec![]
                },
                0
            )
        );

        // assert!(VALIDATOR_REGISTRY.may_load(deps.as_mut().storage, &valid1).unwrap().is_none());
        let redel_val_meta = VALIDATOR_REGISTRY
            .load(deps.as_mut().storage, &valid2)
            .unwrap();
        assert_eq!(redel_val_meta.staked, Uint128::new(300)); // This will be updated when the actual redel message works.
        assert!(check_equal_coin_vector(
            &redel_val_meta.accrued_rewards,
            &vec![Coin::new(50, "urew1")]
        )); // no change in rewards as reply message hasn't run.
    }

    #[test]
    fn test_transfer_reconciled_funds() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        instantiate_contract(&mut deps, &info, &env, None);

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redelegate_addr: valid2.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {})); // Expects manager to make the call

        let pools_info = mock_info(&Addr::unchecked("pools_addr").to_string(), &[]);
        deps.querier.update_balance(
            env.contract.address.clone(),
            vec![
                Coin::new(4356, "utest"),
                Coin::new(885, "urew1"),
                Coin::new(228, "urew2"),
                Coin::new(88876, "air1"),
            ],
        );

        STATE.save(
            deps.as_mut().storage,
            &State {
                slashing_funds: Uint128::new(1000_u128),
                unswapped_rewards: vec![Coin::new(1500, "utest")],
            },
        ).unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::TransferReconciledFunds {
                amount: Uint128::new(400),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(BankMsg::Send {
                to_address: Addr::unchecked("delegator_addr").to_string(),
                amount: vec![Coin::new(400, "utest")]
            })
        );
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.slashing_funds, Uint128::new(1000));
        assert!(check_equal_coin_vector(
            &state.unswapped_rewards,
            &vec![Coin::new(1500, "utest")]
        ));

        // Remove 400 utest as simulation from previous iteration withdraw
        deps.querier.update_balance(
            env.contract.address.clone(),
            vec![
                Coin::new(3956, "utest"),
                Coin::new(885, "urew1"),
                Coin::new(228, "urew2"),
                Coin::new(88876, "air1"),
            ],
        );

        // Use slashing funds as well.
        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::TransferReconciledFunds {
                amount: Uint128::new(1500),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(BankMsg::Send {
                to_address: Addr::unchecked("delegator_addr").to_string(),
                amount: vec![Coin::new(1500, "utest")]
            })
        );
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.slashing_funds, Uint128::new(956));
        assert!(check_equal_coin_vector(
            &state.unswapped_rewards,
            &vec![Coin::new(1500, "utest")]
        ));

        deps.querier.update_balance(
            env.contract.address.clone(),
            vec![
                Coin::new(2456, "utest"),
                Coin::new(885, "urew1"),
                Coin::new(228, "urew2"),
                Coin::new(88876, "air1"),
            ],
        );
        // Use slashing funds as well.
        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::TransferReconciledFunds {
                amount: Uint128::new(1000),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NotEnoughSlashingFunds {}));
    }

    #[test]
    fn test_reply_remove_validator() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();
        instantiate_contract(&mut deps, &info, &env, None);

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");
        VALIDATOR_REGISTRY.save(
            deps.as_mut().storage,
            &valid1,
            &VMeta {
                staked: Uint128::new(0),
                accrued_rewards: vec![Coin::new(100, "utest"), Coin::new(50, "urew1")],
            },
        ).unwrap();

        VALIDATOR_REGISTRY.save(
            deps.as_mut().storage,
            &valid2,
            &VMeta {
                staked: Uint128::new(200),
                accrued_rewards: vec![Coin::new(100, "utest"), Coin::new(100, "urew1")],
            },
        ).unwrap();
        let res = reply(
            deps.as_mut(),
            env,
            Reply {
                id: EVENT_REDELEGATE_ID,
                result: ContractResult::Ok(SubMsgExecutionResponse {
                    events: vec![
                                        Event::new(format!("wasm-{}", EVENT_REDELEGATE_TYPE)) // Events are automatically prepended with a `wasm-`
                                            .add_attribute(
                                                EVENT_REDELEGATE_KEY_SRC_ADDR,
                                                valid1.to_string(),
                                            )
                                            .add_attribute(
                                                EVENT_REDELEGATE_KEY_DST_ADDR,
                                                valid2.to_string(),
                                            ),
                                    ],
                    data: None,
                }),
            },
        )
        .unwrap();
        assert!(res.messages.is_empty());
        assert!(VALIDATOR_REGISTRY
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap()
            .is_none());
        let valid2_meta = VALIDATOR_REGISTRY
            .load(deps.as_mut().storage, &valid2)
            .unwrap();
        assert!(check_equal_coin_vector(
            &valid2_meta.accrued_rewards,
            &vec![Coin::new(200, "utest"), Coin::new(150, "urew1")]
        ));
        assert_eq!(valid2_meta.staked, Uint128::new(200));
    }

    #[test]
    fn test_update_config() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_msg = ExecuteMsg::UpdateConfig {
            pools_contract: None,
            scc_contract: None,
            delegator_contract: None
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
            pools_contract: Addr::unchecked("pools_addr"),
            scc_contract: Addr::unchecked("scc_addr"),
            delegator_contract: Addr::unchecked("delegator_addr"),
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
            pools_contract: Addr::unchecked("new_pools_addr"),
            scc_contract: Addr::unchecked("new_scc_addr"),
            delegator_contract: Addr::unchecked("new_delegator_addr"),
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                pools_contract: Some(Addr::unchecked("new_pools_addr")),
                scc_contract: Some(Addr::unchecked("new_scc_addr")),
                delegator_contract: Some(Addr::unchecked("new_delegator_addr"))
            }.clone(),
        ).unwrap();
        assert!(res.messages.is_empty());
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config, expected_config);
    }
}
