#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};

    use crate::msg::{
        ExecuteMsg, GetConfigResponse, InstantiateMsg, QueryMsg, UpdateUserRewardsRequest,
    };
    use crate::state::{Config, USER_REWARDS};
    use crate::ContractError;
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use cosmwasm_std::{
        coins, from_binary, Addr, BankMsg, Coin, Empty, Env, MessageInfo, OwnedDeps, Response,
        SubMsg, Uint128,
    };

    fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
        info: &MessageInfo,
        env: &Env,
        delegator_contract: Option<String>,
    ) -> Response<Empty> {
        let msg = InstantiateMsg {
            delegator_contract: Addr::unchecked(
                delegator_contract.unwrap_or("delegator_contract".to_string()),
            ),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("delegator_contract")),
        );

        // query the config
        let config_response: GetConfigResponse =
            from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::GetConfig {}).unwrap())
                .unwrap();
        let config = config_response.config;
        assert_eq!(
            config,
            Config {
                manager: Addr::unchecked("creator"),
                delegator_contract: Addr::unchecked("delegator_contract")
            }
        );
    }

    #[test]
    fn test_update_user_rewards() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("delegator_contract")),
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-delegator", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("delegator_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![],
            },
        )
        .unwrap();
        assert!(res.messages.is_empty());
        assert_eq!(res.attributes.len(), 1);

        let user1 = Addr::unchecked("user1");
        let user2 = Addr::unchecked("user2");

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("delegator_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![
                    UpdateUserRewardsRequest {
                        user: user1.clone(),
                        funds: Uint128::new(200),
                    },
                    UpdateUserRewardsRequest {
                        user: user2.clone(),
                        funds: Uint128::new(300),
                    },
                ],
            },
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let user1_rewards = USER_REWARDS
            .load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        let user2_rewards = USER_REWARDS
            .load(deps.as_mut().storage, &user2.clone())
            .unwrap();
        assert_eq!(user1_rewards, Uint128::new(200));
        assert_eq!(user2_rewards, Uint128::new(300));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("delegator_contract", &[]),
            ExecuteMsg::UpdateUserRewards {
                update_user_rewards_requests: vec![UpdateUserRewardsRequest {
                    user: user1.clone(),
                    funds: Uint128::new(300),
                }],
            },
        )
        .unwrap();
        assert!(res.messages.is_empty());
        let user1_rewards = USER_REWARDS
            .load(deps.as_mut().storage, &user1.clone())
            .unwrap();
        let user2_rewards = USER_REWARDS
            .load(deps.as_mut().storage, &user2.clone())
            .unwrap();
        assert_eq!(user1_rewards, Uint128::new(500));
        assert_eq!(user2_rewards, Uint128::new(300));
    }

    #[test]
    fn test_withdraw_funds() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("manager", &coins(1000, "earth"));
        let env = mock_env();

        let _res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(String::from("delegator_contract")),
        );

        /*
           Test - 1. Unauthorized
        */
        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-manager", &[]),
            ExecuteMsg::WithdrawFunds {
                withdraw_address: Addr::unchecked("randomAddr"),
                amount: Default::default(),
                denom: "utest".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("manager", &[]),
            ExecuteMsg::WithdrawFunds {
                withdraw_address: Addr::unchecked("randomAddr"),
                amount: Default::default(),
                denom: "utest".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::AmountZero {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("manager", &[]),
            ExecuteMsg::WithdrawFunds {
                withdraw_address: Addr::unchecked("randomAddr"),
                amount: Uint128::new(800),
                denom: "utest".to_string(),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0],
            SubMsg::new(BankMsg::Send {
                to_address: "randomAddr".to_string(),
                amount: vec![Coin::new(800, "utest")]
            })
        );
    }
}
