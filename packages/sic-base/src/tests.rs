#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};
    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, GetStateResponse, InstantiateMsg, QueryMsg};
    use crate::state::State;
    use crate::test_helpers::check_equal_vec;
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use cosmwasm_std::{
        coin, from_binary, to_binary, Addr, Attribute, BankMsg, Binary, Coin, Empty, Env,
        MessageInfo, OwnedDeps, Response, SubMsg, Uint128, WasmMsg,
    };

    pub fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
        info: &MessageInfo,
        env: &Env,
        vault_denom: Option<String>,
    ) -> Response<Empty> {
        let instantiate_msg = InstantiateMsg {
            scc_address: Addr::unchecked(get_scc_contract_address()),
            strategy_denom: vault_denom.unwrap_or("uluna".to_string()),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    fn get_scc_contract_address() -> String {
        String::from("scc-address")
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None);
        assert_eq!(0, res.messages.len());

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        assert_eq!(
            state_response.state.unwrap(),
            State {
                manager: info.sender,
                scc_address: Addr::unchecked(get_scc_contract_address()),
                strategy_denom: "uluna".to_string(),
                contract_genesis_block_height: env.block.height,
                contract_genesis_timestamp: env.block.time,
                total_rewards_accumulated: Uint128::zero(),
            }
        );
    }

    #[test]
    fn test_try_transfer_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None);
        assert_eq!(0, res.messages.len());

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-scc", &[]),
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
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "empty_funds".to_string(),
                value: "1".to_string()
            }]
        ));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[
                    Coin::new(100_u128, "abc".to_string()),
                    Coin::new(200_u128, "def".to_string()),
                ],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "multiple_coins_sent".to_string(),
                value: "1".to_string()
            }]
        ));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[Coin::new(100_u128, "abc".to_string())],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();
        assert_eq!(res.attributes.len(), 1);
        assert!(check_equal_vec(
            res.attributes,
            vec![Attribute {
                key: "wrong_denom_sent".to_string(),
                value: "1".to_string()
            }]
        ));
    }

    #[test]
    fn test_try_transfer_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None);
        assert_eq!(0, res.messages.len());

        let _res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[Coin::new(100_u128, "uluna".to_string())],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();

        let state_response: GetStateResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetState {}).unwrap())
                .unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_rewards_accumulated, Uint128::new(100_u128));
    }

    #[test]
    fn test_try_claim_airdrops_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None);
        assert_eq!(0, res.messages.len());

        let airdrop_token_contract = Addr::unchecked("airdrop_token_contract");
        let cw20_token_contract = Addr::unchecked("cw20_token_contract");
        fn get_airdrop_claim_msg() -> Binary {
            Binary::from(vec![01, 02, 03, 04, 05, 06, 07, 08])
        }

        let err = execute(
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
    fn test_try_claim_airdrops_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None);
        assert_eq!(0, res.messages.len());

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
    }

    #[test]
    fn test_try_transfer_undelegated_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None);
        assert_eq!(0, res.messages.len());

        let err = execute(
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
                key: "transferring_zero_rewards".to_string(),
                value: "1".to_string()
            }]
        ));
    }

    #[test]
    fn test_try_transfer_undelegated_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None);
        assert_eq!(0, res.messages.len());

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferUndelegatedRewards {
                amount: Uint128::new(100_u128),
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(BankMsg::Send {
                to_address: get_scc_contract_address(),
                amount: vec![coin(100_u128, "uluna".to_string())]
            })]
        ))
    }
}
