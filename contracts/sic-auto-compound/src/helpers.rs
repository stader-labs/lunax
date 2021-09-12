#![allow(dead_code)]

use crate::state::State;
use cosmwasm_std::{Addr, Coin, QuerierWrapper, Uint128};

pub fn get_unaccounted_funds(
    querier: QuerierWrapper,
    contract_address: Addr,
    state: &State,
) -> Uint128 {
    let strategy_denom: String = state.strategy_denom.clone();

    let total_base_funds_in_strategy = querier
        .query_balance(contract_address, strategy_denom.clone())
        .unwrap()
        .amount;
    let current_uninvested_rewards = state.uninvested_rewards.amount;
    let base_funds_from_rewards = state
        .unswapped_rewards
        .iter()
        .find(|&x| x.denom.eq(&strategy_denom))
        .cloned()
        .unwrap_or_else(|| Coin::new(0, strategy_denom))
        .amount;
    let manager_seed_funds = state.manager_seed_funds;

    total_base_funds_in_strategy
        .checked_sub(current_uninvested_rewards)
        .unwrap()
        .checked_sub(manager_seed_funds)
        .unwrap()
        .checked_sub(base_funds_from_rewards)
        .unwrap()
}
