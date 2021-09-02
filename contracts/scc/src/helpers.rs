use crate::state::{StrategyInfo, UserRewardInfo, UserStrategyInfo, USER_REWARD_INFO_MAP};
use cosmwasm_std::{
    Addr, Decimal, Fraction, QuerierWrapper, Response, Storage, Timestamp, Uint128,
};
use sic_base::msg::{GetTotalTokensResponse, QueryMsg as sic_msg};
use stader_utils::coin_utils::{
    decimal_division_in_256, decimal_multiplication_in_256, decimal_subtraction_in_256,
    get_decimal_from_uint128,
};
use std::fmt::Error;
use std::ops::Div;

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
    querier
        .query_wasm_smart(sic_address, &sic_msg::GetTotalTokens {})
        .unwrap()
}

pub fn get_strategy_shares_per_token_ratio(
    querier: QuerierWrapper,
    strategy_info: &StrategyInfo,
) -> Decimal {
    let sic_address = &strategy_info.sic_contract_address;
    let default_s_t_ratio = Decimal::from_ratio(10_u128, 1_u128);

    let total_sic_tokens = get_sic_total_tokens(querier, sic_address)
        .total_tokens
        .unwrap_or(Uint128::zero());
    if total_sic_tokens.is_zero() {
        return default_s_t_ratio;
    }

    let total_strategy_shares = strategy_info.total_shares;

    decimal_division_in_256(
        total_strategy_shares,
        get_decimal_from_uint128(total_sic_tokens),
    )
}

#[cfg(test)]
mod tests {
    use crate::contract::instantiate;
    use crate::helpers::get_vault_apr;
    use crate::msg::InstantiateMsg;
    use crate::state::{UserRewardInfo, UserStrategyInfo, STATE, USER_REWARD_INFO_MAP};
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
        MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{
        Addr, Coin, Decimal, Empty, Env, Fraction, MessageInfo, OwnedDeps, Response, StdResult,
        Uint128,
    };
    use stader_utils::coin_utils::decimal_division_in_256;
    use std::ops::Div;

    pub fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
        info: &MessageInfo,
        env: &Env,
        vault_denom: Option<String>,
    ) -> Response<Empty> {
        let instantiate_msg = InstantiateMsg {
            strategy_denom: vault_denom.unwrap_or_else(|| "uluna".to_string()),
            pools_contract: Addr::unchecked("abc"),
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
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
