#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{execute, instantiate, query};
    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, GetStateResponse, InstantiateMsg, QueryMsg};
    use crate::state::{StakeQuota, State, STATE, VALIDATORS_TO_STAKED_QUOTA};
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
        MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{
        coins, from_binary, to_binary, Addr, Attribute, BankMsg, Binary, Coin, Decimal,
        DistributionMsg, Empty, Env, FullDelegation, MessageInfo, OwnedDeps, Response, StakingMsg,
        StdResult, SubMsg, Uint128, Validator, WasmMsg,
    };
    use cw20::Cw20ExecuteMsg;
    use cw_storage_plus::U64Key;
    use stader_utils::test_helpers::{check_equal_bnk_send_msgs, check_equal_vec};

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
        ]
    }

    fn get_delegations() -> Vec<FullDelegation> {
        vec![
            FullDelegation {
                delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                validator: "valid0001".to_string(),
                amount: Coin::new(2000, "uluna".to_string()),
                can_redelegate: Coin::new(1000, "uluna".to_string()),
                accumulated_rewards: vec![
                    Coin::new(20, "uluna".to_string()),
                    Coin::new(30, "urew1"),
                ],
            },
            FullDelegation {
                delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                validator: "valid0002".to_string(),
                amount: Coin::new(2000, "uluna".to_string()),
                can_redelegate: Coin::new(0, "uluna".to_string()),
                accumulated_rewards: vec![
                    Coin::new(40, "uluna".to_string()),
                    Coin::new(60, "urew1"),
                ],
            },
        ]
    }

    pub fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
        info: &MessageInfo,
        env: &Env,
        validators: Option<Vec<Addr>>,
        strategy_denom: Option<String>,
    ) -> Response<Empty> {
        let default_validator1: Addr = Addr::unchecked("valid0001");
        let default_validator2: Addr = Addr::unchecked("valid0002");
        let scc_address: Addr = Addr::unchecked("scc-address");

        let instantiate_msg = InstantiateMsg {
            scc_address,
            strategy_denom: "uluna".to_string(),
            initial_validators: validators
                .unwrap_or_else(|| vec![default_validator1, default_validator2]),
            manager_seed_funds: Uint128::new(1000_u128),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    fn get_scc_contract_address() -> String {
        String::from("scc-address")
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let default_validator1: Addr = Addr::unchecked("valid0001");
        let default_validator2: Addr = Addr::unchecked("valid0002");
        let scc_address: Addr = Addr::unchecked("scc-address");

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None, None);
        assert_eq!(0, res.messages.len());

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        assert_eq!(
            state_response.state.unwrap(),
            State {
                manager: info.sender,
                scc_address,
                manager_seed_funds: Uint128::new(1000_u128),
                strategy_denom: "uluna".to_string(),
                contract_genesis_block_height: env.block.height,
                contract_genesis_timestamp: env.block.time,
                validator_pool: vec![default_validator1, default_validator2],
                unswapped_rewards: vec![],
                uninvested_rewards: Coin::new(0_u128, "uluna".to_string()),
                total_staked_tokens: Uint128::zero(),
                total_slashed_amount: Uint128::zero()
            }
        );
    }

    #[test]
    fn test__try_transfer_undelegated_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-scc", &[]),
            ExecuteMsg::TransferUndelegatedRewards {
                amount: Uint128::new(100_u128),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferUndelegatedRewards {
                amount: Uint128::zero(),
            },
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "undelegated_zero_funds".to_string(),
                value: "1".to_string()
            }]
        ));
    }

    #[test]
    fn test__try_transfer_undelegated_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. amount is equal to the unaccounted funds
        */
        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.manager_seed_funds = Uint128::new(1000_u128);
                Ok(state)
            },
        );

        deps.querier.update_balance(
            env.contract.address.clone(),
            vec![
                Coin::new(2800_u128, "uluna".to_string()),
                Coin::new(200_u128, "uusd".to_string()),
            ],
        );

        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.uninvested_rewards = Coin::new(300_u128, "uluna".to_string());
                state.unswapped_rewards = vec![
                    Coin::new(200_u128, "uluna".to_string()),
                    Coin::new(500_u128, "uusd".to_string()),
                    Coin::new(300_u128, "ukrt".to_string()),
                ];
                Ok(state)
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferUndelegatedRewards {
                amount: Uint128::new(1000_u128),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: get_scc_contract_address(),
                amount: vec![Coin::new(1000_u128, "uluna".to_string())]
            })]
        ));

        /*
            Test - 2. unaccounted_funds is less than the amount
        */
        deps.querier.update_balance(
            env.contract.address.clone(),
            vec![
                Coin::new(2500_u128, "uluna".to_string()),
                Coin::new(200_u128, "uusd".to_string()),
            ],
        );

        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.uninvested_rewards = Coin::new(300_u128, "uluna".to_string());
                state.unswapped_rewards = vec![
                    Coin::new(200_u128, "uluna".to_string()),
                    Coin::new(500_u128, "uusd".to_string()),
                    Coin::new(300_u128, "ukrt".to_string()),
                ];
                Ok(state)
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferUndelegatedRewards {
                amount: Uint128::new(1200_u128),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: get_scc_contract_address(),
                amount: vec![Coin::new(1000_u128, "uluna".to_string())]
            })]
        ));

        /*
            Test - 2. unaccounted_funds is more than the amount
        */
        deps.querier.update_balance(
            env.contract.address.clone(),
            vec![
                Coin::new(2500_u128, "uluna".to_string()),
                Coin::new(200_u128, "uusd".to_string()),
            ],
        );

        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.uninvested_rewards = Coin::new(300_u128, "uluna".to_string());
                state.unswapped_rewards = vec![
                    Coin::new(200_u128, "uluna".to_string()),
                    Coin::new(500_u128, "uusd".to_string()),
                    Coin::new(300_u128, "ukrt".to_string()),
                ];
                Ok(state)
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferUndelegatedRewards {
                amount: Uint128::new(700_u128),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: get_scc_contract_address(),
                amount: vec![Coin::new(700_u128, "uluna".to_string())]
            })]
        ));
    }

    #[test]
    fn test__try_claim_airdrops_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let airdrop_token_contract = Addr::unchecked("airdrop_token_contract");
        let cw20_token_contract = Addr::unchecked("cw20_token_contract");
        fn get_airdrop_claim_msg() -> Binary {
            Binary::from(vec![01, 02, 03, 04, 05, 06, 07, 08])
        }

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-scc", &[]),
            ExecuteMsg::ClaimAirdrops {
                airdrop_token_contract,
                cw20_token_contract,
                airdrop_token: "abc".to_string(),
                amount: Uint128::new(1000_u128),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test__try_claim_airdrops_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let airdrop_token_contract = Addr::unchecked("airdrop_token_contract");
        let cw20_token_contract = Addr::unchecked("cw20_token_contract");
        let scc_address: Addr = Addr::unchecked("scc-address");

        fn get_airdrop_claim_msg() -> Binary {
            Binary::from(vec![01, 02, 03, 04, 05, 06, 07, 08])
        }

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::ClaimAirdrops {
                airdrop_token_contract: airdrop_token_contract.clone(),
                cw20_token_contract: cw20_token_contract.clone(),
                airdrop_token: "abc".to_string(),
                amount: Uint128::new(1000_u128),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: airdrop_token_contract.to_string(),
                    msg: get_airdrop_claim_msg(),
                    funds: vec![],
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: cw20_token_contract.to_string(),
                    msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                        recipient: scc_address.to_string(),
                        amount: Uint128::new(1000_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                })
            ]
        ));
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::ClaimAirdrops {
                airdrop_token_contract: airdrop_token_contract.clone(),
                cw20_token_contract: cw20_token_contract.clone(),
                airdrop_token: "abc".to_string(),
                amount: Uint128::new(1000_u128),
                claim_msg: get_airdrop_claim_msg(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: airdrop_token_contract.to_string(),
                    msg: get_airdrop_claim_msg(),
                    funds: vec![],
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: cw20_token_contract.to_string(),
                    msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                        recipient: scc_address.to_string(),
                        amount: Uint128::new(1000_u128)
                    })
                    .unwrap(),
                    funds: vec![]
                })
            ]
        ));
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
    }

    #[test]
    fn test__try_undelegate_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-scc", &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(1000_u128),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::zero(),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "undelegated_zero_funds".to_string(),
                value: "1".to_string()
            }]
        ));

        /*
           requested amount is greater than total_staked_tokens
        */
        STATE.update(
            deps.as_mut().storage,
            |mut state| -> Result<_, ContractError> {
                state.total_staked_tokens = Uint128::new(1000_u128);
                Ok(state)
            },
        );

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(2000_u128),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "amount_greater_than_total_tokens".to_string(),
                value: "1".to_string()
            }]
        ));
    }

    #[test]
    fn test__try_undelegate_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let valid1: Addr = Addr::unchecked("valid0001");
        let valid2: Addr = Addr::unchecked("valid0002");

        /*
           Test - 1. Normal undelegation
        */
        VALIDATORS_TO_STAKED_QUOTA.save(
            deps.as_mut().storage,
            &valid1,
            &StakeQuota {
                amount: Coin::new(500_u128, "uluna"),
                stake_fraction: Decimal::from_ratio(1_u128, 2_u128),
            },
        );

        VALIDATORS_TO_STAKED_QUOTA.save(
            deps.as_mut().storage,
            &valid2,
            &StakeQuota {
                amount: Coin::new(500_u128, "uluna"),
                stake_fraction: Decimal::from_ratio(1_u128, 2_u128),
            },
        );

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.total_staked_tokens = Uint128::new(1000_u128);
            Ok(state)
        });
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::UndelegateRewards {
                amount: Uint128::new(500_u128),
            },
        )
        .unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_staked_tokens, Uint128::new(500_u128));
        assert_eq!(res.messages.len(), 3);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_binary(&ExecuteMsg::RedeemRewards {}).unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(StakingMsg::Undelegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(250_u128, "uluna")
                }),
                SubMsg::new(StakingMsg::Undelegate {
                    validator: valid2.to_string(),
                    amount: Coin::new(250_u128, "uluna")
                })
            ]
        ));
        let valid1_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        let valid2_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid2)
            .unwrap();
        assert_ne!(valid1_staked_quota_option, None);
        assert_ne!(valid2_staked_quota_option, None);
        let valid1_staked_quota = valid1_staked_quota_option.unwrap();
        let valid2_staked_quota = valid2_staked_quota_option.unwrap();
        assert_eq!(valid1_staked_quota.amount, Coin::new(250_u128, "uluna"));
        assert_eq!(valid2_staked_quota.amount, Coin::new(250_u128, "uluna"));
        assert_eq!(
            valid1_staked_quota.stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );
        assert_eq!(
            valid2_staked_quota.stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );
    }

    #[test]
    fn test__try_reinvest__fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "no_uninvested_rewards".to_string(),
                value: "1".to_string()
            }]
        ));
    }

    #[test]
    fn test__try_reinvest__success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        fn get_zero_delegations() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(0, "uluna".to_string()),
                    can_redelegate: Coin::new(1000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(00, "uluna".to_string()),
                        Coin::new(00, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(0, "uluna".to_string()),
                    can_redelegate: Coin::new(0, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(00, "uluna".to_string()),
                        Coin::new(00, "urew1"),
                    ],
                },
            ]
        }

        let deleg1 = Addr::unchecked("deleg0001".to_string());
        let deleg2 = Addr::unchecked("deleg0002".to_string());
        let deleg3 = Addr::unchecked("deleg0003".to_string());
        let valid1 = Addr::unchecked("valid0001".to_string());
        let valid2 = Addr::unchecked("valid0002".to_string());
        let valid3 = Addr::unchecked("valid0003".to_string());

        /*
           Test - 1. First reinvest
        */
        deps.querier
            .update_staking("test", &*get_validators(), &*get_zero_delegations());

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.uninvested_rewards = Coin::new(1000_u128, "uluna");
            Ok(state)
        });
        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(100_u128, "uluna")]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_staked_tokens, Uint128::new(1000_u128));
        assert_eq!(state.total_slashed_amount, Uint128::zero());
        assert_eq!(
            state.uninvested_rewards,
            Coin::new(0_u128, "uluna".to_string())
        );
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                }),
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid2.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                })
            ]
        ));
        let valid1_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert_ne!(valid1_staked_quota_option, None);
        let valid2_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid2)
            .unwrap();
        assert_ne!(valid2_staked_quota_option, None);
        let valid1_staked_quota = valid1_staked_quota_option.unwrap();
        assert_eq!(valid1_staked_quota.amount, Coin::new(500_u128, "uluna"));
        assert_eq!(
            valid1_staked_quota.stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );
        let valid2_staked_quota = valid2_staked_quota_option.unwrap();
        assert_eq!(valid2_staked_quota.amount, Coin::new(500_u128, "uluna"));
        assert_eq!(
            valid2_staked_quota.stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );

        /*
           Test - 2. Reinvesting after a few reinvests
        */
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.uninvested_rewards = Coin::new(1000_u128, "uluna");
            state.total_staked_tokens = Uint128::new(4000_u128);
            Ok(state)
        });

        VALIDATORS_TO_STAKED_QUOTA.save(
            deps.as_mut().storage,
            &valid1,
            &StakeQuota {
                amount: Coin::new(2000_u128, "uluna"),
                stake_fraction: Decimal::from_ratio(1_u128, 2_u128),
            },
        );
        VALIDATORS_TO_STAKED_QUOTA.save(
            deps.as_mut().storage,
            &valid1,
            &StakeQuota {
                amount: Coin::new(2000_u128, "uluna"),
                stake_fraction: Decimal::from_ratio(1_u128, 2_u128),
            },
        );

        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(100_u128, "uluna")]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_staked_tokens, Uint128::new(5000_u128));
        assert_eq!(state.total_slashed_amount, Uint128::zero());
        assert_eq!(
            state.uninvested_rewards,
            Coin::new(0_u128, "uluna".to_string())
        );
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                }),
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid2.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                })
            ]
        ));
        let valid1_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert_ne!(valid1_staked_quota_option, None);
        let valid2_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid2)
            .unwrap();
        assert_ne!(valid2_staked_quota_option, None);
        let valid1_staked_quota = valid1_staked_quota_option.unwrap();
        assert_eq!(valid1_staked_quota.amount, Coin::new(2500_u128, "uluna"));
        assert_eq!(
            valid1_staked_quota.stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );
        let valid2_staked_quota = valid2_staked_quota_option.unwrap();
        assert_eq!(valid2_staked_quota.amount, Coin::new(2500_u128, "uluna"));
        assert_eq!(
            valid2_staked_quota.stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );

        /*
           Test - 3. Slashing
        */
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.uninvested_rewards = Coin::new(1000_u128, "uluna");
            state.total_staked_tokens = Uint128::new(5000_u128);
            Ok(state)
        });
        VALIDATORS_TO_STAKED_QUOTA.save(
            deps.as_mut().storage,
            &valid1,
            &StakeQuota {
                amount: Coin::new(2500_u128, "uluna"),
                stake_fraction: Decimal::from_ratio(1_u128, 2_u128),
            },
        );
        VALIDATORS_TO_STAKED_QUOTA.save(
            deps.as_mut().storage,
            &valid1,
            &StakeQuota {
                amount: Coin::new(2500_u128, "uluna"),
                stake_fraction: Decimal::from_ratio(1_u128, 2_u128),
            },
        );
        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(100_u128, "uluna")]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap();
        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_staked_tokens, Uint128::new(5000_u128));
        assert_eq!(state.total_slashed_amount, Uint128::new(1000_u128));
        assert_eq!(
            state.uninvested_rewards,
            Coin::new(0_u128, "uluna".to_string())
        );
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                }),
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid2.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                })
            ]
        ));
        let valid1_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert_ne!(valid1_staked_quota_option, None);
        let valid2_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid2)
            .unwrap();
        assert_ne!(valid2_staked_quota_option, None);
        let valid1_staked_quota = valid1_staked_quota_option.unwrap();
        assert_eq!(valid1_staked_quota.amount, Coin::new(2500_u128, "uluna"));
        assert_eq!(
            valid1_staked_quota.stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );
        let valid2_staked_quota = valid2_staked_quota_option.unwrap();
        assert_eq!(valid2_staked_quota.amount, Coin::new(2500_u128, "uluna"));
        assert_eq!(
            valid2_staked_quota.stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );
    }

    #[test]
    fn test__try_transfer_rewards__fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-scc-contract", &[]),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "no_funds_sent".to_string(),
                value: "1".to_string()
            }]
        ));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[Coin::new(10_u128, "abc"), Coin::new(10_u128, "abc")],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "multiple_coins_passed".to_string(),
                value: "1".to_string()
            }]
        ));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[Coin::new(10_u128, "abc")]),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "transferred_denom_is_wrong".to_string(),
                value: "1".to_string()
            }]
        ));
    }

    #[test]
    fn test__try_transfer_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. First reinvest
        */
        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[Coin::new(100_u128, "uluna")],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.uninvested_rewards, Coin::new(100_u128, "uluna"));
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(env.contract.address.clone()),
                    msg: to_binary(&ExecuteMsg::RedeemRewards {}).unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(env.contract.address.clone()),
                    msg: to_binary(&ExecuteMsg::Reinvest {}).unwrap(),
                    funds: vec![]
                })
            ]
        ));

        /*
           Test - 2. Reinvest with existing uninvested_rewards
        */
        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.uninvested_rewards = Coin::new(1000_u128, "uluna");
            Ok(state)
        });

        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[Coin::new(100_u128, "uluna")],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.uninvested_rewards, Coin::new(1100_u128, "uluna"));
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(env.contract.address.clone()),
                    msg: to_binary(&ExecuteMsg::RedeemRewards {}).unwrap(),
                    funds: vec![]
                }),
                SubMsg::new(WasmMsg::Execute {
                    contract_addr: String::from(env.contract.address),
                    msg: to_binary(&ExecuteMsg::Reinvest {}).unwrap(),
                    funds: vec![]
                })
            ]
        ));
    }

    #[test]
    fn test__try_redeem_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemRewards {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages,
            vec![
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0001".to_string(),
                }),
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0002".to_string(),
                })
            ]
        );
        let mut state = STATE.load(deps.as_mut().storage).unwrap();
        assert!(check_equal_vec(
            state.unswapped_rewards,
            vec![Coin::new(90, "urew1"), Coin::new(60, "uluna")]
        ));
    }
}
