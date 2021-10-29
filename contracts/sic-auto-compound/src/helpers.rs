#![allow(dead_code)]

use crate::state::State;
use cosmwasm_std::{Addr, Coin, QuerierWrapper, StdResult, Uint128};
use std::collections::HashMap;

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

pub fn get_pool_stake_info(
    querier: QuerierWrapper,
    delegator: String,
    validator_pool: Vec<Addr>,
) -> StdResult<(HashMap<Addr, Uint128>, Uint128)> {
    let mut total_staked_tokens = Uint128::zero();
    let mut validator_stake_map: HashMap<Addr, Uint128> = HashMap::new();

    for validator in validator_pool.iter() {
        let stake_amount =
            if let Some(delegation) = querier.query_delegation(delegator.clone(), validator)? {
                delegation.amount.amount
            } else {
                Uint128::zero()
            };

        total_staked_tokens = total_staked_tokens.checked_add(stake_amount).unwrap();
        validator_stake_map
            .entry(validator.clone())
            .or_insert(stake_amount);
    }

    Ok((validator_stake_map, total_staked_tokens))
}
