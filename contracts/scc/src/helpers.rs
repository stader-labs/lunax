use crate::state::{StrategyInfo, UserRewardInfo, UserStrategyInfo, USER_REWARD_INFO_MAP};
use crate::user::get_user_airdrops;
use cosmwasm_std::{
    Addr, Coin, Decimal, Fraction, QuerierWrapper, Response, Storage, Timestamp, Uint128,
};
use sic_base::msg::{
    GetFulfillableUndelegatedFundsResponse, GetTotalTokensResponse, QueryMsg as sic_msg,
};
use stader_utils::coin_utils::{
    decimal_division_in_256, decimal_multiplication_in_256, decimal_subtraction_in_256,
    get_decimal_from_uint128, merge_coin_vector, uint128_from_decimal,
};

pub fn get_vault_apr(
    current_shares_per_token_ratio: Decimal,
    contract_genesis_shares_per_token_ratio: Decimal,
    contract_genesis_timestamp: Timestamp,
    current_block_time: Timestamp,
) -> Decimal {
    let shares_token_ratio_reduction: Decimal = decimal_subtraction_in_256(
        contract_genesis_shares_per_token_ratio,
        current_shares_per_token_ratio,
    );
    let shares_token_ratio_reduction_ratio: Decimal = decimal_multiplication_in_256(
        shares_token_ratio_reduction,
        contract_genesis_shares_per_token_ratio.inv().unwrap(),
    );
    let time_since_genesis = Uint128::new(
        current_block_time
            .minus_seconds(contract_genesis_timestamp.seconds())
            .seconds() as u128,
    );
    let year_in_secs = Uint128::new(Timestamp::from_seconds(365 * 24 * 3600).seconds() as u128);
    let year_extrapolation = Decimal::from_ratio(year_in_secs, time_since_genesis);

    let decimal_apr =
        decimal_multiplication_in_256(shares_token_ratio_reduction_ratio, year_extrapolation);
    decimal_multiplication_in_256(decimal_apr, Decimal::from_ratio(100_u128, 1_u128))
}

pub fn get_sic_total_tokens(querier: QuerierWrapper, sic_address: &Addr) -> GetTotalTokensResponse {
    // TODO: bchain99 - we should handle this gracefully. cannot assume anything about external SICs
    querier
        .query_wasm_smart(sic_address, &sic_msg::GetTotalTokens {})
        .unwrap()
}

// tells us how much the sic contract can gave back for an undelegation of "amount". Ideally it should be equal to "amount"
// but if there is an undelegation slashing event or any other such event, then SCC can account for such events.
pub fn get_sic_fulfillable_undelegated_funds(
    querier: QuerierWrapper,
    amount: Uint128,
    sic_address: &Addr,
) -> Uint128 {
    let res: GetFulfillableUndelegatedFundsResponse = querier
        .query_wasm_smart(
            sic_address,
            &sic_msg::GetFulfillableUndelegatedFunds { amount },
        )
        .unwrap();

    res.undelegated_funds.unwrap()
}

pub fn get_strategy_shares_per_token_ratio(
    querier: QuerierWrapper,
    strategy_info: &StrategyInfo,
) -> Decimal {
    let sic_address = &strategy_info.sic_contract_address;
    let default_s_t_ratio = Decimal::from_ratio(10_u128, 1_u128);

    let total_sic_tokens = get_sic_total_tokens(querier, sic_address)
        .total_tokens
        .unwrap_or_else(Uint128::zero);
    if total_sic_tokens.is_zero() {
        return default_s_t_ratio;
    }

    let total_strategy_shares = strategy_info.total_shares;

    decimal_division_in_256(
        total_strategy_shares,
        get_decimal_from_uint128(total_sic_tokens),
    )
}

pub fn get_user_staked_amount(shares_per_token_ratio: Decimal, total_shares: Decimal) -> Uint128 {
    uint128_from_decimal(decimal_division_in_256(
        total_shares,
        shares_per_token_ratio,
    ))
}

pub fn get_strategy_split(
    user_reward_info: &UserRewardInfo,
    amount: Uint128,
) -> (Vec<(String, Uint128)>, Uint128) {
    let user_portfolio = &user_reward_info.user_portfolio;

    let mut strategy_split: Vec<(String, Uint128)> = vec![];
    let mut surplus = amount;
    for u in user_portfolio {
        let strategy_name = u.strategy_name.clone();
        let deposit_fraction = u.deposit_fraction;

        let deposit_amount = uint128_from_decimal(decimal_multiplication_in_256(
            deposit_fraction,
            get_decimal_from_uint128(amount),
        ));

        strategy_split.push((strategy_name, deposit_amount));

        surplus = surplus.checked_sub(deposit_amount).unwrap();
    }

    (strategy_split, surplus)
}

#[cfg(test)]
mod tests {
    use crate::contract::instantiate;
    use crate::helpers::{get_strategy_shares_per_token_ratio, get_strategy_split, get_vault_apr};
    use crate::msg::InstantiateMsg;
    use crate::state::{
        StrategyInfo, UserRewardInfo, UserStrategyInfo, UserStrategyPortfolio, STATE, STRATEGY_MAP,
        USER_REWARD_INFO_MAP,
    };
    use cosmwasm_std::{
        Addr, Coin, Decimal, Empty, Env, Fraction, MessageInfo, OwnedDeps, Response, StdResult,
        Uint128,
    };
    use stader_utils::coin_utils::{decimal_division_in_256, decimal_subtraction_in_256};
    use stader_utils::mock::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use stader_utils::test_helpers::check_equal_vec;
    use std::collections::HashMap;
    use std::ops::Div;

    pub fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
        info: &MessageInfo,
        env: &Env,
        strategy_denom: Option<String>,
    ) -> Response<Empty> {
        let instantiate_msg = InstantiateMsg {
            strategy_denom: strategy_denom.unwrap_or_else(|| "uluna".to_string()),
            pools_contract: Addr::unchecked("abc"),
            default_user_portfolio: None,
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    #[test]
    fn test__get_strategy_shares_per_token_ratio() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(&mut deps, &info, &env, None);

        let sic1_address = Addr::unchecked("sic1_address");

        /*
           Test - 1. S_T ratio is less than 10
        */
        let mut contracts_to_tokens: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_tokens.insert(sic1_address.clone(), Uint128::new(500_u128));
        deps.querier.update_wasm(Some(contracts_to_tokens), None);

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: false,
                total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );

        let strategy_info = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap()
            .unwrap();
        let s_t_ratio = get_strategy_shares_per_token_ratio(deps.as_ref().querier, &strategy_info);

        assert_eq!(s_t_ratio, Decimal::from_ratio(2_u128, 1_u128));

        /*
           Test - 2. S_T ratio is greater than 10
        */
        let mut contracts_to_tokens: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_tokens.insert(sic1_address.clone(), Uint128::new(50_u128));
        deps.querier.update_wasm(Some(contracts_to_tokens), None);

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            "sid1",
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: sic1_address.clone(),
                unbonding_period: 3600,
                unbonding_buffer: 3600,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: false,
                total_shares: Decimal::from_ratio(1000_u128, 1_u128),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
            },
        );

        let strategy_info = STRATEGY_MAP
            .may_load(deps.as_mut().storage, "sid1")
            .unwrap()
            .unwrap();
        let s_t_ratio = get_strategy_shares_per_token_ratio(deps.as_ref().querier, &strategy_info);

        assert_eq!(s_t_ratio, Decimal::from_ratio(20_u128, 1_u128));
    }

    #[test]
    fn test__get_strategy_split() {
        /*
           Test - 1. 100% split in the portfolio. No surplus
        */
        let user_reward_info = UserRewardInfo {
            user_portfolio: vec![
                UserStrategyPortfolio {
                    strategy_name: "sid1".to_string(),
                    deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                },
                UserStrategyPortfolio {
                    strategy_name: "sid2".to_string(),
                    deposit_fraction: Decimal::from_ratio(1_u128, 2_u128),
                },
                UserStrategyPortfolio {
                    strategy_name: "sid3".to_string(),
                    deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                },
            ],
            strategies: vec![],
            pending_airdrops: vec![],
            undelegation_records: vec![],
            pending_rewards: Uint128::zero(),
        };
        let amount = Uint128::new(100_u128);

        let res = get_strategy_split(&user_reward_info, amount);
        let strategy_split = res.0;
        let surplus = res.1;
        assert_eq!(surplus, Uint128::zero());
        assert!(check_equal_vec(
            strategy_split,
            vec![
                ("sid1".to_string(), Uint128::new(25_u128)),
                ("sid2".to_string(), Uint128::new(50_u128)),
                ("sid3".to_string(), Uint128::new(25_u128))
            ]
        ));

        /*
           Test - 2. There is some surplus
        */

        let user_reward_info = UserRewardInfo {
            user_portfolio: vec![
                UserStrategyPortfolio {
                    strategy_name: "sid1".to_string(),
                    deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                },
                UserStrategyPortfolio {
                    strategy_name: "sid3".to_string(),
                    deposit_fraction: Decimal::from_ratio(1_u128, 4_u128),
                },
            ],
            strategies: vec![],
            pending_airdrops: vec![],
            undelegation_records: vec![],
            pending_rewards: Uint128::zero(),
        };
        let amount = Uint128::new(100_u128);

        let res = get_strategy_split(&user_reward_info, amount);
        let strategy_split = res.0;
        let surplus = res.1;
        assert_eq!(surplus, Uint128::new(50_u128));
        assert!(check_equal_vec(
            strategy_split,
            vec![
                ("sid1".to_string(), Uint128::new(25_u128)),
                ("sid3".to_string(), Uint128::new(25_u128))
            ]
        ));

        /*
           Test - 3. There is no portfolio
        */

        let user_reward_info = UserRewardInfo {
            user_portfolio: vec![],
            strategies: vec![],
            pending_airdrops: vec![],
            undelegation_records: vec![],
            pending_rewards: Uint128::zero(),
        };
        let amount = Uint128::new(100_u128);

        let res = get_strategy_split(&user_reward_info, amount);
        let strategy_split = res.0;
        let surplus = res.1;
        assert_eq!(surplus, Uint128::new(100_u128));
        assert!(check_equal_vec(strategy_split, vec![]));
    }

    #[test]
    fn test__get_vault_apr() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(&mut deps, &info, &env, None);

        let deleg1 = Addr::unchecked("deleg0001".to_string());
        let initial_shares_per_token_ratio = Decimal::from_ratio(1_000_000_00_u128, 1_u128);

        /*
           Test - 1. Vault apr when shares_per_token_ratio is still 1
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_vault_apr(
            Decimal::from_ratio(1_000_000_00_u128, 1_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state.contract_genesis_timestamp.plus_seconds(500),
        );
        assert_eq!(apr, Decimal::zero());

        /*
           Test - 2. Vault apr when shares_per_token_ratio becomes 0 ( will never happen )
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_vault_apr(
            Decimal::zero(),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state.contract_genesis_timestamp.plus_seconds(500),
        );
        assert_eq!(apr, Decimal::from_ratio(6307200_u128, 1_u128));

        /*
            Test - 3. Vault apr when shares_per_token_ratio becomes 0.9 after 1000s
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_vault_apr(
            Decimal::from_ratio(9_000_000_00_u128, 10_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state.contract_genesis_timestamp.plus_seconds(1000),
        );
        assert_eq!(apr, Decimal::from_ratio(3153600_u128, 10_u128));

        /*
            Test - 4. Vault apr when shares_per_token_ratio becomes 0.8 after 5000s
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_vault_apr(
            Decimal::from_ratio(8_000_000_00_u128, 10_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state.contract_genesis_timestamp.plus_seconds(5000),
        );
        assert_eq!(apr, Decimal::from_ratio(126144_u128, 1_u128));

        /*
           Test - 5. Vault apr when shares_per_token_ratio becomes 0.009 after 10000s.
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_vault_apr(
            Decimal::from_ratio(9_000_000_00_u128, 1000_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state.contract_genesis_timestamp.plus_seconds(10000),
        );
        assert_eq!(apr, Decimal::from_ratio(31252176_u128, 100_u128));

        /*
           Test - 6. Vault apr when shares_per_token_ratio becomes 0.94 after 1 year
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_vault_apr(
            Decimal::from_ratio(94_000_000_00_u128, 100_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state
                .contract_genesis_timestamp
                .plus_seconds(365 * 24 * 3600),
        );
        assert_eq!(apr, Decimal::from_ratio(6_u128, 1_u128));

        /*
            Test - 7. Vault apr when shares_per_token_ratio becomes 0.90 after 2 year
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_vault_apr(
            Decimal::from_ratio(9_000_000_000_u128, 100_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state
                .contract_genesis_timestamp
                .plus_seconds(365 * 24 * 3600 * 2),
        );
        assert_eq!(apr, Decimal::from_ratio(5_u128, 1_u128));

        /*
            Test - 8. Vault apr when shares_per_token_ratio becomes 0.90 after half a year
        */
        let state = STATE.load(deps.as_mut().storage).unwrap();
        let apr = get_vault_apr(
            Decimal::from_ratio(9_000_000_000_u128, 100_u128),
            initial_shares_per_token_ratio,
            state.contract_genesis_timestamp,
            state
                .contract_genesis_timestamp
                .plus_seconds(365 * 12 * 3600),
        );
        assert_eq!(apr, Decimal::from_ratio(20_u128, 1_u128));
    }
}
