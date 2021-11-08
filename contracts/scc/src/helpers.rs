#![allow(dead_code)]

use crate::state::{
    get_default_s_t_ratio, Config, StrategyInfo, UserRewardInfo, UserStrategyPortfolio,
    STRATEGY_MAP,
};
use crate::ContractError;
use cosmwasm_std::{Addr, Decimal, QuerierWrapper, StdResult, Storage, Uint128};
use cw_storage_plus::U64Key;
use sic_base::msg::{
    GetFulfillableUndelegatedFundsResponse, GetTotalTokensResponse, QueryMsg as sic_msg,
};
use stader_utils::coin_utils::{
    decimal_division_in_256, decimal_multiplication_in_256, get_decimal_from_uint128,
    uint128_from_decimal,
};
use std::collections::HashMap;
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

    let total_sic_tokens_res = get_sic_total_tokens(querier, sic_address)?;
    let total_sic_tokens = total_sic_tokens_res
        .total_tokens
        .unwrap_or_else(Uint128::zero);

    let total_strategy_shares = strategy_info.total_shares;

    if total_sic_tokens.is_zero() || total_strategy_shares.is_zero() {
        return Ok(get_default_s_t_ratio());
    }

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
    // if the default strategy has been deactivated then we have no choice but to fall back to retain rewards
    let fallback_strategy =
        get_expected_strategy_or_default(storage, config.fallback_strategy, 0u64)?;
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

                strategy_id =
                    get_expected_strategy_or_default(storage, strategy_id, fallback_strategy)?;

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

            // add the left out amount to the fallback strategy
            strategy_to_amount
                .entry(fallback_strategy)
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
    user_portfolio: &[UserStrategyPortfolio],
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
