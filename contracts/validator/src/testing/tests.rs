#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};
    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, GetConfigResponse, InstantiateMsg, QueryMsg};
    use crate::state::{Config, CONFIG, VALIDATOR_REGISTRY};
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
        MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{
        from_binary, to_binary, Addr, Attribute, BankMsg, Binary, Coin, Decimal, DistributionMsg,
        Env, FullDelegation, MessageInfo, OwnedDeps, Response, StakingMsg, SubMsg, Uint128,
        Validator, WasmMsg,
    };
    use cw20::Cw20ExecuteMsg;
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
            pools_contract: Addr::unchecked("pools_addr"),
            scc_contract: Addr::unchecked("scc_addr"),
            delegator_contract: Addr::unchecked("delegator_addr"),
            airdrop_withdraw_contract: Addr::unchecked("airdrop_withdraw_addr"),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            pools_contract: Addr::unchecked("pools_address"),
            scc_contract: Addr::unchecked("scc_addr"),
            delegator_contract: Addr::unchecked("delegator_addr"),
            airdrop_withdraw_contract: Addr::unchecked("airdrop_withdraw_addr"),
        };
        let expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "uluna".to_string(),
            pools_contract: Addr::unchecked("pools_address"),
            delegator_contract: Addr::unchecked("delegator_addr"),
            airdrop_withdraw_contract: Addr::unchecked("airdrop_withdraw_addr"),
        };
        let info = mock_info("creator", &[]);

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

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
    fn test_set_withdraw_address() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        instantiate_contract(&mut deps, &info, &env, None);

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::SetRewardWithdrawAddress {
                reward_contract: Addr::unchecked("reward_withdraw_addr"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[]), // Only pools contract can call
            ExecuteMsg::SetRewardWithdrawAddress {
                reward_contract: Addr::unchecked("reward_withdraw_addr"),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(DistributionMsg::SetWithdrawAddress {
                address: "reward_withdraw_addr".to_string()
            })
        );
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

        let pools_info = mock_info(&pools_addr.to_string(), &[Coin::new(1200, "uluna")]);

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &pools_addr.to_string(),
                &[Coin::new(1200, "uluna"), Coin::new(1000, "othercoin")],
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

        VALIDATOR_REGISTRY
            .save(deps.as_mut().storage, &valid1, &true)
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
                amount: Coin::new(1200, "uluna")
            })
        );

        let valid1_meta = VALIDATOR_REGISTRY
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert!(valid1_meta.is_some());

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

        let pools_info = mock_info(&pools_addr.to_string(), &[Coin::new(1200, "uluna")]);
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

        VALIDATOR_REGISTRY
            .save(deps.as_mut().storage, &valid1.clone(), &true)
            .unwrap();
        VALIDATOR_REGISTRY
            .save(deps.as_mut().storage, &valid2.clone(), &true)
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
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages[0],
            SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                validator: valid1.to_string()
            })
        );

        assert_eq!(
            res.messages[1],
            SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                validator: valid2.to_string()
            })
        );
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
        let pools_info = mock_info(&pools_addr.to_string(), &[Coin::new(1200, "uluna")]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(), // Pools contract as caller
            ExecuteMsg::Redelegate {
                src: valid1.clone(),
                dst: valid2.clone(),
                amount: Uint128::new(150),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]), // Manager as caller does not result in auth error.
            ExecuteMsg::Redelegate {
                src: valid1.clone(),
                dst: valid2.clone(),
                amount: Uint128::new(150),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {})); // Redundant check but to drive the point home.

        VALIDATOR_REGISTRY
            .save(deps.as_mut().storage, &valid1, &true)
            .unwrap();
        let pools_info = mock_info(&pools_addr.to_string(), &[]);
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
            .save(deps.as_mut().storage, &valid2, &true)
            .unwrap();
        let pools_info = mock_info(&pools_addr.to_string(), &[]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Redelegate {
                src: valid1.clone(),
                dst: valid2.clone(),
                amount: Uint128::new(1150),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InSufficientFunds {}));

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
                amount: Coin::new(15, "uluna")
            })
        );
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
            .save(deps.as_mut().storage, &valid1, &true)
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Undelegate {
                val_addr: valid1.clone(),
                amount: Uint128::new(0),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ZeroAmount {}));

        let pools_info = mock_info(&pools_addr.to_string(), &[]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::Undelegate {
                val_addr: valid1.clone(),
                amount: Uint128::new(1150),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InSufficientFunds {}));

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
                amount: Coin::new(15, "uluna")
            })
        );
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
            mock_info("creator", &[]), // Only pools-contract can call this.
            ExecuteMsg::RedeemAirdropAndTransfer {
                amount: Uint128::new(2000_u128),
                claim_msg: get_airdrop_claim_msg(),
                airdrop_contract: Addr::unchecked(""),
                cw20_contract: Addr::unchecked(""),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        /*
           Test - 2. First airdrops claim
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[]),
            ExecuteMsg::RedeemAirdropAndTransfer {
                amount: Uint128::new(2000_u128),
                claim_msg: get_airdrop_claim_msg(),
                airdrop_contract: anc_airdrop_contract.clone(),
                cw20_contract: anc_token_contract.clone(),
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
                    recipient: "airdrop_withdraw_addr".to_string(), // Set from config.
                    amount: Uint128::new(2000_u128),
                })
                .unwrap(),
                funds: vec![]
            })
        );

        /*
            Test - 3. MIR claim with ANC in pool
        */
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[]),
            ExecuteMsg::RedeemAirdropAndTransfer {
                amount: Uint128::new(1000_u128),
                claim_msg: get_airdrop_claim_msg(),
                airdrop_contract: mir_airdrop_contract.clone(),
                cw20_contract: mir_token_contract.clone(),
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
                    recipient: "airdrop_withdraw_addr".to_string(),
                    amount: Uint128::new(1000_u128),
                })
                .unwrap(),
                funds: vec![]
            })
        );
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
            mock_info("pools_addr", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redelegate_addr: valid2.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {})); // Expects manager to make the call

        VALIDATOR_REGISTRY
            .save(deps.as_mut().storage, &valid1, &true)
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redelegate_addr: valid2.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotAdded {})); // Expects manager to make the call

        VALIDATOR_REGISTRY
            .save(deps.as_mut().storage, &valid2, &true)
            .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redelegate_addr: valid2.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ZeroAmount {}));

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("pools_addr", &[]),
            ExecuteMsg::RemoveValidator {
                val_addr: valid1.clone(),
                redelegate_addr: valid2.clone(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(StakingMsg::Redelegate {
                src_validator: valid1.to_string(),
                dst_validator: valid2.to_string(),
                amount: Coin::new(1000, "uluna")
            })
        );

        assert!(VALIDATOR_REGISTRY
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap()
            .is_none());
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
        deps.querier
            .update_balance(env.contract.address.clone(), vec![Coin::new(4356, "uluna")]);

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
                amount: vec![Coin::new(400, "uluna")]
            })
        );

        // Remove 400 uluna as simulation from previous iteration withdraw
        deps.querier
            .update_balance(env.contract.address.clone(), vec![Coin::new(3956, "uluna")]);

        // Use slashing funds as well.
        let res = execute(
            deps.as_mut(),
            env.clone(),
            pools_info.clone(),
            ExecuteMsg::TransferReconciledFunds {
                amount: Uint128::new(3000),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(BankMsg::Send {
                to_address: Addr::unchecked("delegator_addr").to_string(),
                amount: vec![Coin::new(3000, "uluna")]
            })
        );
    }

    #[test]
    fn test_update_config() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        instantiate_contract(&mut deps, &info, &env, None);

        let initial_msg = ExecuteMsg::UpdateConfig {
            pools_contract: None,
            delegator_contract: None,
            airdrop_withdraw_contract: None,
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
            pools_contract: Addr::unchecked("pools_addr"),
            delegator_contract: Addr::unchecked("delegator_addr"),
            airdrop_withdraw_contract: Addr::unchecked("airdrop_withdraw_addr"),
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
            pools_contract: Addr::unchecked("new_pools_addr"),
            delegator_contract: Addr::unchecked("new_delegator_addr"),
            airdrop_withdraw_contract: Addr::unchecked("new_airdrop_withdraw_addr"),
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::UpdateConfig {
                pools_contract: Some(Addr::unchecked("new_pools_addr")),
                delegator_contract: Some(Addr::unchecked("new_delegator_addr")),
                airdrop_withdraw_contract: Some(Addr::unchecked("new_airdrop_withdraw_addr")),
            }
            .clone(),
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        assert_eq!(config, expected_config);
    }
}
