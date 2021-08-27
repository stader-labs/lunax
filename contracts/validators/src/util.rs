use cosmwasm_std::{Coin, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Operation {
    Add,
    Sub,
    Replace,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CoinOp {
    pub(crate) fund: Coin,
    pub(crate) operation: Operation,
}

// Supports vector of coins only
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CoinVecOp {
    pub(crate) fund: Vec<Coin>,
    pub(crate) operation: Operation,
}

pub fn merge_coin(coin1: &Coin, coin_op: CoinOp) -> Coin {
    let fund = coin_op.fund;
    let operation = coin_op.operation;

    // TODO - GM. Is denom equality check required?
    // TODO - GM. Should worry about denom casing?
    match operation {
        Operation::Add => Coin {
            amount: coin1.amount.checked_add(fund.amount).unwrap(),
            denom: fund.denom,
        },
        Operation::Sub => {
            if coin1.amount.u128() < fund.amount.u128() {
                panic!(
                    "Cannot make coin with negative balance {}-{}",
                    coin1.amount, fund.amount
                )
            }
            Coin {
                amount: coin1.amount.checked_sub(fund.amount).unwrap(),
                denom: fund.denom,
            }
        }
        Operation::Replace => fund, // _ => panic!("Unknown operation type {:?}", operation)
    }
}

pub fn merge_coin_vector(coins: &Vec<Coin>, coin_vec_op: CoinVecOp) -> Vec<Coin> {
    let fund = coin_vec_op.fund;
    let operation = coin_vec_op.operation;

    match operation {
        Operation::Add => add_coin_vectors(coins, &fund),
        Operation::Sub => subtract_coin_vectors(coins, &fund),
        Operation::Replace => fund,
    }
}

fn add_coin_vectors(coins1: &Vec<Coin>, coins2: &Vec<Coin>) -> Vec<Coin> {
    let mut coin_map = add_coin_vector_to_map(&mut HashMap::new(), coins1);
    coin_map = add_coin_vector_to_map(&mut coin_map, coins2);
    return map_to_coin_vec(coin_map);
}

fn subtract_coin_vectors(coins1: &Vec<Coin>, coins2: &Vec<Coin>) -> Vec<Coin> {
    let mut coin_map = add_coin_vector_to_map(&mut HashMap::new(), coins1);
    coin_map = subtract_coin_vector_from_map(&mut coin_map, coins2);
    return map_to_coin_vec(coin_map);
}

// Not to be used with Vec<{(120, "token1"), (30, "token1") ..}. No denom should be present more than once.
pub fn add_coin_vector_to_map(
    existing_coins: &mut HashMap<String, Uint128>, new_coins: &Vec<Coin>,
) -> HashMap<String, Uint128> {
    let mut accumulated_coins: HashMap<String, Uint128> = existing_coins.clone();
    let mut denom_set: HashSet<String> = HashSet::new();
    for coin in new_coins {
        if existing_coins.contains_key(&coin.denom) {
            if denom_set.contains(&coin.denom) {
                panic!("Multiple coins of same denom found {}", coin.denom);
            } else {
                denom_set.insert(coin.denom.clone());
            }

            let existing_coin = existing_coins.get(&coin.denom).unwrap();
            accumulated_coins.insert(
                coin.denom.clone(),
                Uint128::new(coin.amount.u128() + existing_coin.u128()),
            );
        } else {
            accumulated_coins.insert(coin.denom.clone(), coin.amount);
        }
    }
    accumulated_coins
}

// Not to be used with Vec<{(120, "token1"), (30, "token1") ..}. No denom should be present more than once.
pub fn subtract_coin_vector_from_map(
    existing_coins: &mut HashMap<String, Uint128>, new_coins: &Vec<Coin>,
) -> HashMap<String, Uint128> {
    let mut dissipated_coins: HashMap<String, Uint128> = existing_coins.clone();
    let mut denom_set: HashSet<String> = HashSet::new();
    for coin in new_coins {
        if existing_coins.contains_key(&coin.denom) {
            if denom_set.contains(&coin.denom) {
                panic!("Multiple coins of same denom found {}", coin.denom);
            } else {
                denom_set.insert(coin.denom.clone());
            }

            let existing_coin = existing_coins.get(&coin.denom).unwrap();

            if existing_coin.lt(&coin.amount) {
                panic!(
                    "Cannot subtract {:?}-{:?} for denom {:?}",
                    existing_coin, &coin.amount, coin.denom
                );
            }

            dissipated_coins.insert(
                coin.denom.clone(),
                Uint128::new(existing_coin.u128() - coin.amount.u128()),
            );
        } else {
            panic!(
                "Cannot subtract {:?} for denom {:?} because there is no prior coin",
                &coin.amount, coin.denom
            );
        }
    }
    dissipated_coins
}

// TODO - GM. Generalize map_to_vec for Coin and DecCoin
pub fn map_to_coin_vec(coin_map: HashMap<String, Uint128>) -> Vec<Coin> {
    let mut coins: Vec<Coin> = vec![];
    for denom in coin_map.keys() {
        coins.push(Coin {
            denom: denom.clone(),
            amount: *coin_map.get(denom).unwrap(),
        })
    }
    coins
}
