#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{execute, instantiate, query};
    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, GetConfigResponse, InstantiateMsg, QueryMsg};
    use crate::state::{Config, VMeta, VALIDATOR_META};
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
        MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{
        coins, from_binary, Addr, Attribute, Coin, Decimal, DistributionMsg, Empty, Env,
        FullDelegation, MessageInfo, OwnedDeps, Response, StakingMsg, SubMsg, Uint128, Validator,
    };
    use stader_utils::coin_utils::check_equal_coin_vector;

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
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>, info: &MessageInfo, env: &Env,
        vault_denom: Option<String>,
    ) -> Response<Empty> {
        let instantiate_msg = InstantiateMsg {
            vault_denom: vault_denom.unwrap_or_else(|| "utest".to_string()),
            pools_contract_addr: Addr::unchecked("pools_addr"),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            vault_denom: "utest".to_string(),
            pools_contract_addr: Addr::unchecked("pools_address"),
        };
        let expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "utest".to_string(),
            pools_contract_addr: Addr::unchecked("pools_address"),
        };
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetConfig {}).unwrap();
        let value: GetConfigResponse = from_binary(&res).unwrap();
        assert_eq!(value.config, expected_config);
    }

    #[test]
    fn test_add_validator() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let valid1 = Addr::unchecked("valid0001");
        let valid2 = Addr::unchecked("valid0002");

        instantiate_contract(&mut deps, &info, &env, None);

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        assert!(VALIDATOR_META
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap()
            .is_none());
        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddValidator {
                val_addr: valid1.clone(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        let valid1_meta = VALIDATOR_META
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert!(valid1_meta.is_some());
        let valid1_meta_unwrapped = valid1_meta.unwrap();
        assert!(valid1_meta_unwrapped.accrued_rewards.is_empty());
        assert!(valid1_meta_unwrapped.staked.is_zero());
        // assert!(valid1_meta_unwrapped.reward_pointer.is_zero());

        let err = execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddValidator {
                val_addr: valid1.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorAlreadyExists {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddValidator {
                val_addr: Addr::unchecked("valid0004").clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ValidatorNotDiscoverable {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("other", &[]),
            ExecuteMsg::AddValidator {
                val_addr: valid1.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test_stake() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[Coin::new(1500, "utest")]);
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
        VALIDATOR_META
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

        let valid1_meta = VALIDATOR_META
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert!(valid1_meta.is_some());
        let valid1_meta_unwrapped = valid1_meta.unwrap();
        assert!(check_equal_coin_vector(
            &valid1_meta_unwrapped.accrued_rewards,
            &initial_accrued_rewards.clone()
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
        let valid1_meta = VALIDATOR_META
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert!(valid1_meta.is_some());
        let valid1_meta_unwrapped = valid1_meta.unwrap();
        assert!(check_equal_coin_vector(
            &valid1_meta_unwrapped.accrued_rewards,
            &initial_accrued_rewards.clone()
        )); // Accrued rewards remains unchanged
        assert_eq!(valid1_meta_unwrapped.staked, Uint128::new(2400)); // Adds to previous staked amount.
    }

    #[test]
    fn test_redeem_rewards() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[Coin::new(1500, "utest")]);
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

        assert!(VALIDATOR_META
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
        VALIDATOR_META
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
            ExecuteMsg::RedeemRewards {
                validators: vec![valid1.clone()],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                validator: valid1.to_string()
            })
        );

        // assert!(res.attributes[0])
        // let initial_accrued_rewards = vec![Coin::new(123, "utest")];
        // VALIDATOR_META.save(deps.as_mut().storage, &valid1, &VMeta {
        //     staked: Default::default(),
        //     accrued_rewards: initial_accrued_rewards.clone()
        // }).unwrap();
        //
        // let res = execute(deps.as_mut(), env.clone(), pools_info.clone(),
        //                   ExecuteMsg::Stake { val_addr: valid1.clone() },
        // ).unwrap();
        // assert_eq!(res.messages.len(), 1);
        // assert_eq!(res.messages[0], SubMsg::new(StakingMsg::Delegate {
        //     validator: valid1.to_string(),
        //     amount: Coin::new(1200, "utest")
        // }));
        //
        // let valid1_meta = VALIDATOR_META.may_load(deps.as_mut().storage, &valid1).unwrap();
        // assert!(valid1_meta.is_some());
        // let valid1_meta_unwrapped = valid1_meta.unwrap();
        // assert!(check_equal_coin_vector(&valid1_meta_unwrapped.accrued_rewards, &initial_accrued_rewards.clone()));
        // assert_eq!(valid1_meta_unwrapped.staked, Uint128::new(1200));
        //
        // let res = execute(deps.as_mut(), env.clone(), pools_info.clone(),
        //                   ExecuteMsg::Stake { val_addr: valid1.clone() },
        // ).unwrap();
        // let valid1_meta = VALIDATOR_META.may_load(deps.as_mut().storage, &valid1).unwrap();
        // assert!(valid1_meta.is_some());
        // let valid1_meta_unwrapped = valid1_meta.unwrap();
        // assert!(check_equal_coin_vector(&valid1_meta_unwrapped.accrued_rewards, &initial_accrued_rewards.clone()));// Accrued rewards remains unchanged
        // assert_eq!(valid1_meta_unwrapped.staked, Uint128::new(2400)); // Adds to previous staked amount.
    }
}
