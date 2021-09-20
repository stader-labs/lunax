#![allow(dead_code)]

use crate::state::{Config, StrategyInfo, UserRewardInfo, UserStrategyPortfolio, STRATEGY_MAP};
use crate::ContractError;
use cosmwasm_std::{
    Addr, Decimal, Fraction, QuerierWrapper, StdResult, Storage, Timestamp, Uint128,
};
use cw_storage_plus::U64Key;
use sic_base::msg::{
    GetFulfillableUndelegatedFundsResponse, GetTotalTokensResponse, QueryMsg as sic_msg,
};
use stader_utils::coin_utils::{
    decimal_division_in_256, decimal_multiplication_in_256, decimal_subtraction_in_256,
    get_decimal_from_uint128, uint128_from_decimal,
};
use std::collections::HashMap;

pub fn get_strategy_apr(
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

// TODO: bchain99 - we can probably make these generic
pub fn get_sic_total_tokens(
    querier: QuerierWrapper,
    sic_address: &Addr,
) -> StdResult<GetTotalTokensResponse> {
    querier.query_wasm_smart(sic_address, &sic_msg::GetTotalTokens {})
}

// tells us how much the sic contract can gave back for an undelegation of "amount". Ideally it should be equal to "amount"
// but if there is an undelegation slashing event or any other such event, then SCC can account for such events.
pub fn get_sic_fulfillable_undelegated_funds(
    querier: QuerierWrapper,
    amount: Uint128,
    sic_address: &Addr,
) -> StdResult<GetFulfillableUndelegatedFundsResponse> {
    querier.query_wasm_smart(
        sic_address,
        &sic_msg::GetFulfillableUndelegatedFunds { amount },
    )
}

pub fn get_strategy_shares_per_token_ratio(
    querier: QuerierWrapper,
    strategy_info: &StrategyInfo,
) -> StdResult<Decimal> {
    let sic_address = &strategy_info.sic_contract_address;
    let default_s_t_ratio = Decimal::from_ratio(10_u128, 1_u128);

    let total_sic_tokens_res = get_sic_total_tokens(querier, sic_address)?;
    let total_sic_tokens = total_sic_tokens_res
        .total_tokens
        .unwrap_or_else(Uint128::zero);

    if total_sic_tokens.is_zero() {
        return Ok(default_s_t_ratio);
    }

    let total_strategy_shares = strategy_info.total_shares;

    Ok(decimal_division_in_256(
        total_strategy_shares,
        get_decimal_from_uint128(total_sic_tokens),
    ))
}

pub fn get_staked_amount(shares_per_token_ratio: Decimal, total_shares: Decimal) -> Uint128 {
    uint128_from_decimal(decimal_division_in_256(
        total_shares,
        shares_per_token_ratio,
    ))
}

pub fn get_expected_strategy_or_default(
    storage: &mut dyn Storage,
    strategy_id: u64,
    default_strategy: u64,
) -> StdResult<u64> {
    match STRATEGY_MAP.may_load(storage, U64Key::new(strategy_id))? {
        None => Ok(default_strategy),
        Some(strategy_info) => {
            if !strategy_info.is_active {
                return Ok(default_strategy);
            }

            Ok(strategy_id)
        }
    }
}

// Gets the split for the user portfolio
// if a strategy in the user portfolio is non-existent, removed or is inactive, we fall back to
// the default strategy in config.
// the return value is a map of strategy id to the amount to invest in the strategy
// checks for amount.is_zero() should be done outside the function.
pub fn get_strategy_split(
    storage: &mut dyn Storage,
    config: &Config,
    strategy_override: Option<u64>,
    user_reward_info: &UserRewardInfo,
    amount: Uint128,
) -> StdResult<HashMap<u64, Uint128>> {
    let mut strategy_to_amount: HashMap<u64, Uint128> = HashMap::new();
    let user_portfolio = &user_reward_info.user_portfolio;

    match strategy_override {
        None => {
            let mut surplus_amount = amount;
            for u in user_portfolio {
                let mut strategy_id = u.strategy_id;
                let deposit_fraction = u.deposit_fraction;

                let deposit_amount = uint128_from_decimal(decimal_multiplication_in_256(
                    Decimal::from_ratio(deposit_fraction, 100_u128),
                    get_decimal_from_uint128(amount),
                ));

                strategy_id = get_expected_strategy_or_default(
                    storage,
                    strategy_id,
                    config.fallback_strategy,
                )?;

                strategy_to_amount
                    .entry(strategy_id)
                    .and_modify(|x| {
                        *x = x.checked_add(deposit_amount).unwrap();
                    })
                    .or_insert(deposit_amount);

                // we will ideally never underflow here as the underflow can happen
                // if the user portfolio fraction is somehow greater than 1 which we validate
                // for before creating the portfolio.
                surplus_amount = surplus_amount.checked_sub(deposit_amount).unwrap();
            }

            // add the left out amount to retain rewards strategy.(strategy_id 0)
            strategy_to_amount
                .entry(0)
                .and_modify(|x| {
                    *x = x.checked_add(surplus_amount).unwrap();
                })
                .or_insert(surplus_amount);
        }
        Some(strategy_override) => {
            let strategy_id = get_expected_strategy_or_default(
                storage,
                strategy_override,
                config.fallback_strategy,
            )?;
            strategy_to_amount.insert(strategy_id, amount);
        }
    }

    Ok(strategy_to_amount)
}

pub fn validate_user_portfolio(
    storage: &mut dyn Storage,
    user_portfolio: &Vec<UserStrategyPortfolio>,
) -> Result<bool, ContractError> {
    let mut total_deposit_fraction = Uint128::zero();
    for u in user_portfolio {
        let strategy_id = u.strategy_id;
        if STRATEGY_MAP
            .may_load(storage, U64Key::new(strategy_id))?
            .is_none()
        {
            return Err(ContractError::StrategyInfoDoesNotExist {});
        }

        total_deposit_fraction = total_deposit_fraction
            .checked_add(u.deposit_fraction)
            .unwrap();
    }

    if total_deposit_fraction > Uint128::new(100_u128) {
        return Err(ContractError::InvalidPortfolioDepositFraction {});
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use crate::contract::instantiate;
    use crate::helpers::{
        get_expected_strategy_or_default, get_strategy_apr, get_strategy_shares_per_token_ratio,
        get_strategy_split, validate_user_portfolio,
    };
    use crate::msg::InstantiateMsg;
    use crate::state::{
        Config, StrategyInfo, UserRewardInfo, UserStrategyInfo, UserStrategyPortfolio, CONFIG,
        STATE, STRATEGY_MAP, USER_REWARD_INFO_MAP,
    };
    use crate::ContractError;
    use cosmwasm_std::{
        Addr, Coin, Decimal, Empty, Env, Fraction, MessageInfo, OwnedDeps, Response, StdResult,
        Uint128,
    };
    use cw_storage_plus::U64Key;
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
            delegator_contract: Addr::unchecked("abc"),
            default_user_portfolio: None,
            default_fallback_strategy: None,
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    macro_rules! hashmap {
        ($( $key: expr => $val: expr ),*) => {{
             let mut map = ::std::collections::HashMap::new();
             $( map.insert($key, $val); )*
             map
        }}
    }

    #[test]
    fn test__validate_user_portfolio() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(&mut deps, &info, &env, None);

        /*
           Test - 1. Non-existent strategy
        */
        let err = validate_user_portfolio(
            deps.as_mut().storage,
            &vec![UserStrategyPortfolio {
                strategy_id: 1,
                deposit_fraction: Uint128::new(50_u128),
            }],
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::StrategyInfoDoesNotExist {}));

        /*
            Test - 2. Invalid deposit fraction
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(1),
            &StrategyInfo::default("sid1".to_string()),
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(2),
            &StrategyInfo::default("sid2".to_string()),
        );

        let err = validate_user_portfolio(
            deps.as_mut().storage,
            &vec![
                UserStrategyPortfolio {
                    strategy_id: 1,
                    deposit_fraction: Uint128::new(50_u128),
                },
                UserStrategyPortfolio {
                    strategy_id: 2,
                    deposit_fraction: Uint128::new(75_u128),
                },
            ],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::InvalidPortfolioDepositFraction {}
        ));

        /*
            Test - 3. Valid portfolio
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(1),
            &StrategyInfo::default("sid1".to_string()),
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(2),
            &StrategyInfo::default("sid2".to_string()),
        );

        let res = validate_user_portfolio(
            deps.as_mut().storage,
            &vec![
                UserStrategyPortfolio {
                    strategy_id: 1,
                    deposit_fraction: Uint128::new(50_u128),
                },
                UserStrategyPortfolio {
                    strategy_id: 2,
                    deposit_fraction: Uint128::new(50_u128),
                },
            ],
        )
        .unwrap();
        assert!(res);
    }

    #[test]
    fn test__get_expected_strategy_or_default() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(&mut deps, &info, &env, None);

        /*
           Test - 1. Strategy does not exist or is removed
        */
        let strategy_id = get_expected_strategy_or_default(deps.as_mut().storage, 1, 0).unwrap();
        assert_eq!(strategy_id, 0);

        /*
           Test - 2. Strategy is not active
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(1),
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 0,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: false,
                total_shares: Default::default(),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Default::default(),
            },
        );
        let strategy_id = get_expected_strategy_or_default(deps.as_mut().storage, 1, 0).unwrap();
        assert_eq!(strategy_id, 0);

        /*
           Test - 3. Strategy is good
        */
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(1),
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 0,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: true,
                total_shares: Default::default(),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Default::default(),
            },
        );
        let strategy_id = get_expected_strategy_or_default(deps.as_mut().storage, 1, 0).unwrap();
        assert_eq!(strategy_id, 1);
    }

    #[test]
    fn test__get_strategy_split() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(&mut deps, &info, &env, None);

        /*
           Test - 1. There is a strategy override and the strategy is not active
        */
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(1),
            &StrategyInfo::default("sid1".to_string()),
        );

        let split = get_strategy_split(
            deps.as_mut().storage,
            &config,
            Some(1),
            &UserRewardInfo::default(),
            Uint128::new(100_u128),
        )
        .unwrap();

        assert_eq!(split, hashmap![0 => Uint128::new(100_u128)]);

        /*
            Test - 2. There is a strategy override and the strategy is active
        */
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(1),
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 0,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: true,
                total_shares: Default::default(),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Default::default(),
            },
        );

        let split = get_strategy_split(
            deps.as_mut().storage,
            &config,
            Some(1),
            &UserRewardInfo::default(),
            Uint128::new(100_u128),
        )
        .unwrap();

        assert_eq!(split, hashmap![1 => Uint128::new(100_u128)]);

        /*
            Test - 3. There is no strategy override and all strategies are active
        */
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(1),
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 0,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: true,
                total_shares: Default::default(),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Default::default(),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(2),
            &StrategyInfo {
                name: "sid2".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 0,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: true,
                total_shares: Default::default(),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Default::default(),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(3),
            &StrategyInfo {
                name: "sid3".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 0,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: true,
                total_shares: Default::default(),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Default::default(),
            },
        );

        let mut user_reward_info = UserRewardInfo::default();
        user_reward_info.user_portfolio = vec![
            UserStrategyPortfolio {
                strategy_id: 1,
                deposit_fraction: Uint128::new(25_u128),
            },
            UserStrategyPortfolio {
                strategy_id: 2,
                deposit_fraction: Uint128::new(25_u128),
            },
            UserStrategyPortfolio {
                strategy_id: 3,
                deposit_fraction: Uint128::new(25_u128),
            },
        ];

        let split = get_strategy_split(
            deps.as_mut().storage,
            &config,
            None,
            &user_reward_info,
            Uint128::new(100_u128),
        )
        .unwrap();

        assert_eq!(
            split,
            hashmap![0 => Uint128::new(25_u128), 1 => Uint128::new(25_u128), 2 => Uint128::new(25_u128), 3 => Uint128::new(25_u128)]
        );

        /*
            Test - 4. There is no strategy override and 2/3 strategies are inactive
        */
        let config = CONFIG.load(deps.as_mut().storage).unwrap();
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(1),
            &StrategyInfo {
                name: "sid1".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 0,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: true,
                total_shares: Default::default(),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Default::default(),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(2),
            &StrategyInfo {
                name: "sid2".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 0,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: false,
                total_shares: Default::default(),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Default::default(),
            },
        );
        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(3),
            &StrategyInfo {
                name: "sid3".to_string(),
                sic_contract_address: Addr::unchecked("abc"),
                unbonding_period: 0,
                unbonding_buffer: 0,
                undelegation_batch_id_pointer: 0,
                reconciled_batch_id_pointer: 0,
                is_active: false,
                total_shares: Default::default(),
                current_undelegated_shares: Default::default(),
                global_airdrop_pointer: vec![],
                total_airdrops_accumulated: vec![],
                shares_per_token_ratio: Default::default(),
            },
        );

        let mut user_reward_info = UserRewardInfo::default();
        user_reward_info.user_portfolio = vec![
            UserStrategyPortfolio {
                strategy_id: 1,
                deposit_fraction: Uint128::new(25_u128),
            },
            UserStrategyPortfolio {
                strategy_id: 2,
                deposit_fraction: Uint128::new(25_u128),
            },
            UserStrategyPortfolio {
                strategy_id: 3,
                deposit_fraction: Uint128::new(25_u128),
            },
        ];

        let split = get_strategy_split(
            deps.as_mut().storage,
            &config,
            None,
            &user_reward_info,
            Uint128::new(100_u128),
        )
        .unwrap();

        assert_eq!(
            split,
            hashmap![0 => Uint128::new(75_u128), 1 => Uint128::new(25_u128)]
        );
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
            U64Key::new(1),
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
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap()
            .unwrap();
        let s_t_ratio =
            get_strategy_shares_per_token_ratio(deps.as_ref().querier, &strategy_info).unwrap();

        assert_eq!(s_t_ratio, Decimal::from_ratio(2_u128, 1_u128));

        /*
           Test - 2. S_T ratio is greater than 10
        */
        let mut contracts_to_tokens: HashMap<Addr, Uint128> = HashMap::new();
        contracts_to_tokens.insert(sic1_address.clone(), Uint128::new(50_u128));
        deps.querier.update_wasm(Some(contracts_to_tokens), None);

        STRATEGY_MAP.save(
            deps.as_mut().storage,
            U64Key::new(1),
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
            .may_load(deps.as_mut().storage, U64Key::new(1))
            .unwrap()
            .unwrap();
        let s_t_ratio =
            get_strategy_shares_per_token_ratio(deps.as_ref().querier, &strategy_info).unwrap();

        assert_eq!(s_t_ratio, Decimal::from_ratio(20_u128, 1_u128));
    }

    #[test]
    fn test__get_strategy_apr() {
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
        let apr = get_strategy_apr(
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
        let apr = get_strategy_apr(
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
        let apr = get_strategy_apr(
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
        let apr = get_strategy_apr(
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
        let apr = get_strategy_apr(
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
        let apr = get_strategy_apr(
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
        let apr = get_strategy_apr(
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
        let apr = get_strategy_apr(
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
