#[cfg(test)]
mod tests {
    use crate::contract::{check_slashing, execute, instantiate, query};
    use crate::error::ContractError;
    use crate::msg::{
        ExecuteMsg, InstantiateMsg, MerkleAirdropMsg, QueryConfigResponse, QueryMsg,
        QueryStateResponse,
    };
    use crate::state::{Config, State, CONFIG};
    use crate::testing::mock_querier;
    use crate::testing::test_helpers::check_equal_vec;
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
        MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{
        attr, from_binary, to_binary, Addr, Coin, Decimal, DistributionMsg, Env, FullDelegation,
        MessageInfo, OwnedDeps, StdResult, SubMsg, Timestamp, Uint128, Validator, WasmMsg,
    };
    use cw_storage_plus::U64Key;
    use stader_utils::coin_utils::{check_equal_deccoin_vector, DecCoin};

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
    ) {
        let instantiate_msg = InstantiateMsg {
            unbonding_period: 1000,
            undelegation_cooldown: 100,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
            reward_contract: "".to_string(),
            airdrops_registry_contract: "".to_string(),
            airdrop_withdrawal_contract: "".to_string(),
            protocol_fee_contract: "".to_string(),
            protocol_reward_fee: Default::default(),
            protocol_deposit_fee: Default::default(),
            protocol_withdraw_fee: Default::default(),
        };

        instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let msg = InstantiateMsg {
            unbonding_period: 3600 * 24 * 21,
            undelegation_cooldown: 10,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
            reward_contract: "reward_contract".to_string(),
            airdrops_registry_contract: "airdrop_registry_contract".to_string(),
            airdrop_withdrawal_contract: "airdrop_withdrawal_contract".to_string(),
            protocol_fee_contract: "protocol_fee_contract".to_string(),
            protocol_reward_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            protocol_deposit_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            protocol_withdraw_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
        };
        let expected_config = Config {
            manager: Addr::unchecked("creator"),
            vault_denom: "uluna".to_string(),
            unbonding_period: 3600 * 24 * 21,
            undelegation_cooldown: 10,
            min_deposit: Uint128::new(1000),
            max_deposit: Uint128::new(1_000_000_000_000),
            active: true,
            reward_contract: Addr::unchecked("reward_contract"),
            cw20_token_contract: Addr::unchecked("0"),
            airdrop_registry_contract: Addr::unchecked("airdrop_registry_contract"),
            airdrop_withdrawal_contract: Addr::unchecked("airdrop_withdrawal_contract"),
            protocol_fee_contract: Addr::unchecked("protocol_fee_contract"),
            protocol_reward_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            protocol_deposit_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
            protocol_withdraw_fee: Decimal::from_ratio(1_u128, 100_u128), // 1%
        };
        let info = mock_info("creator", &[]);

        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(check_equal_vec(
            res.messages,
            vec![SubMsg::new(DistributionMsg::SetWithdrawAddress {
                address: "reward_contract".to_string()
            })]
        ));

        let config_res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let config: QueryConfigResponse = from_binary(&config_res).unwrap();
        assert_eq!(config.config, expected_config);

        let state_res = query(deps.as_ref(), mock_env(), QueryMsg::State {}).unwrap();
        let state: QueryStateResponse = from_binary(&state_res).unwrap();
        assert_eq!(
            state.state,
            State {
                total_staked: Uint128::zero(),
                exchange_rate: Decimal::one(),
                last_reconciled_batch_id: 0,
                current_undelegation_batch_id: 2,
                last_undelegation_time: env
                    .block
                    .time
                    .minus_seconds(config.config.undelegation_cooldown), // Gives flexibility for first undelegaion run.
                validators: vec![]
            }
        );
    }
}
