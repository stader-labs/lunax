#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};
    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, GetStateResponse, InstantiateMsg, MerkleAirdropMsg, QueryMsg};
    use crate::state::{State, STATE};
    use crate::test_helpers::check_equal_vec;
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use cosmwasm_std::{
        coin, from_binary, to_binary, Addr, BankMsg, Coin, Empty, Env, MessageInfo, OwnedDeps,
        Response, SubMsg, Uint128, WasmMsg,
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
    fn test_transfer_rewards_fail() {
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

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoFundsSent {}));

        let err = execute(
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
        .unwrap_err();
        assert!(matches!(err, ContractError::MultipleCoinsSent {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[Coin::new(100_u128, "abc".to_string())],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::WrongDenom(String { .. })));
    }

    #[test]
    fn test_transfer_rewards_success() {
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
    fn test_claim_airdrops_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None);
        assert_eq!(0, res.messages.len());

        let airdrop_token_contract = Addr::unchecked("airdrop_token_contract");

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-scc", &[]),
            ExecuteMsg::ClaimAirdrops {
                airdrop_token_contract: airdrop_token_contract.to_string(),
                airdrop_token: "abc".to_string(),
                amount: Uint128::new(1000_u128),
                stage: 0,
                proof: vec![],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test_claim_airdrops_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None);
        assert_eq!(0, res.messages.len());

        let airdrop_token_contract = Addr::unchecked("airdrop_token_contract");

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::ClaimAirdrops {
                airdrop_token_contract: airdrop_token_contract.to_string(),
                airdrop_token: "abc".to_string(),
                amount: Uint128::new(1000_u128),
                stage: 3,
                proof: vec![
                    "proof1".to_string(),
                    "proof2".to_string(),
                    "proof3".to_string(),
                ],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: airdrop_token_contract.to_string(),
                msg: to_binary(&MerkleAirdropMsg::Claim {
                    stage: 3,
                    amount: Uint128::new(1000_u128),
                    proof: vec![
                        "proof1".to_string(),
                        "proof2".to_string(),
                        "proof3".to_string()
                    ]
                })
                .unwrap(),
                funds: vec![],
            }),]
        ));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::ClaimAirdrops {
                airdrop_token_contract: airdrop_token_contract.to_string(),
                airdrop_token: "abc".to_string(),
                amount: Uint128::new(1000_u128),
                stage: 4,
                proof: vec![
                    "proof1".to_string(),
                    "proof2".to_string(),
                    "proof3".to_string(),
                ],
            },
        )
        .unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: airdrop_token_contract.to_string(),
                msg: to_binary(&MerkleAirdropMsg::Claim {
                    stage: 4,
                    amount: Uint128::new(1000_u128),
                    proof: vec![
                        "proof1".to_string(),
                        "proof2".to_string(),
                        "proof3".to_string()
                    ]
                })
                .unwrap(),
                funds: vec![],
            }),]
        ));
    }

    #[test]
    fn test_transfer_airdrops_to_scc_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None);
        assert_eq!(0, res.messages.len());

        let cw20_token_contract = Addr::unchecked("airdrop_token_contract");
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-scc", &[]),
            ExecuteMsg::TransferAirdropsToScc {
                cw20_token_contract: cw20_token_contract.to_string(),
                airdrop_token: "anc".to_string(),
                amount: Uint128::new(100),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test_transfer_airdrops_to_scc_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None);
        assert_eq!(0, res.messages.len());

        let cw20_token_contract = Addr::unchecked("airdrop_token_contract");
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("scc-address", &[]),
            ExecuteMsg::TransferAirdropsToScc {
                cw20_token_contract: cw20_token_contract.to_string(),
                airdrop_token: "anc".to_string(),
                amount: Uint128::new(100),
            },
        )
        .unwrap();
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: cw20_token_contract.to_string(),
                msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                    recipient: state.scc_address.to_string(),
                    amount: Uint128::new(100)
                })
                .unwrap(),
                funds: vec![]
            })]
        ));
    }

    #[test]
    fn test_transfer_undelegated_rewards_fail() {
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

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferUndelegatedRewards {
                amount: Uint128::zero(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::ZeroWithdrawal {}));

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.total_rewards_accumulated = Uint128::new(100_u128);
                    Ok(state)
                },
            )
            .unwrap();
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferUndelegatedRewards {
                amount: Uint128::new(150_u128),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InSufficientFunds {}));
    }

    #[test]
    fn test_transfer_undelegated_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None);
        assert_eq!(0, res.messages.len());

        STATE
            .update(
                deps.as_mut().storage,
                |mut state| -> Result<_, ContractError> {
                    state.total_rewards_accumulated = Uint128::new(200_u128);
                    Ok(state)
                },
            )
            .unwrap();
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
        ));
        let state = STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(state.total_rewards_accumulated, Uint128::new(100_u128));
    }
}
