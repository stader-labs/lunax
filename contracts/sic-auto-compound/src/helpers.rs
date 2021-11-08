#![allow(dead_code)]

use crate::error::ContractError;
use crate::state::State;
use cosmwasm_std::{Addr, Coin, MessageInfo, QuerierWrapper, StdResult, Uint128};
use reward::msg::{QueryMsg as reward_query, SwappedAmountResponse};
use std::collections::HashMap;

pub fn get_unaccounted_funds(
    querier: QuerierWrapper,
    contract_address: Addr,
    state: &State,
) -> Uint128 {
    let strategy_denom: String = state.strategy_denom.clone();

    let total_base_funds_in_strategy = querier
        .query_balance(contract_address, strategy_denom)
        .unwrap()
        .amount;
    let manager_seed_funds = state.manager_seed_funds;

    total_base_funds_in_strategy
        .checked_sub(manager_seed_funds)
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
                continue;
            };

        total_staked_tokens = total_staked_tokens.checked_add(stake_amount).unwrap();
        validator_stake_map
            .entry(validator.clone())
            .or_insert(stake_amount);
    }

    Ok((validator_stake_map, total_staked_tokens))
}

pub fn get_reward_tokens(querier: QuerierWrapper, reward_contract: Addr) -> StdResult<Uint128> {
    let res: SwappedAmountResponse =
        querier.query_wasm_smart(reward_contract.to_string(), &reward_query::SwappedAmount {})?;
    Ok(res.amount)
}

pub fn get_validator_for_deposit(
    querier: QuerierWrapper,
    delegator: String,
    validators: Vec<Addr>,
) -> Result<Addr, ContractError> {
    if validators.is_empty() {
        return Err(ContractError::NoValidatorsInPool {});
    }

    let mut stake_tuples = vec![];
    for val_addr in validators {
        if querier.query_validator(val_addr.clone())?.is_none() {
            // Don't deposit to a jailed validator
            continue;
        }
        let delegation_opt = querier.query_delegation(delegator.clone(), val_addr.clone())?;

        if delegation_opt.is_none() {
            // No delegation. So use the validator
            return Ok(val_addr);
        }
        stake_tuples.push((
            delegation_opt.unwrap().amount.amount.u128(),
            val_addr.to_string(),
        ))
    }
    if stake_tuples.is_empty() {
        return Err(ContractError::AllValidatorsJailed {});
    }
    stake_tuples.sort();
    Ok(Addr::unchecked(stake_tuples.first().unwrap().clone().1))
}

pub fn get_validated_coin(
    info: &MessageInfo,
    staking_denom: String,
) -> Result<Coin, ContractError> {
    // check if any money is being sent
    if info.funds.is_empty() {
        return Err(ContractError::NoFundsSent {});
    }

    // accept only one coin
    if info.funds.len() > 1 {
        return Err(ContractError::MultipleCoins {});
    }

    let transferred_coin = info.funds[0].clone();
    if transferred_coin.denom.ne(&staking_denom) {
        return Err(ContractError::WrongDenom(transferred_coin.denom));
    }

    Ok(transferred_coin)
}
