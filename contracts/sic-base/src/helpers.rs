#![allow(dead_code)]

use cosmwasm_bignumber::Decimal256;
use cosmwasm_std::{Coin, Decimal, Fraction, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// TODO: bchain99 - There are some cyclic dependencies when I add stader-utils to sic-base. Fix them in the future.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Operation {
    Add,
    Sub,
    Replace,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CoinOp {
    pub fund: Coin,
    pub operation: Operation,
}

// Supports vector of coins only
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CoinVecOp {
    pub fund: Vec<Coin>,
    pub operation: Operation,
}

// TODO - GM. What happens to all these methods where amount is not available but sub amount is 0, Especiolly for vec case.
// (coin1 + coin2) or (coin1 - coin2)
pub fn merge_coin(coin1: Coin, coin_op: CoinOp) -> Coin {
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

// Jumbles the order of the vector
// (Coins + CoinVecOp.fund) and (Coins - CoinVecOp.fund) [Element wise operation but Sub is stricter than set operation]
pub fn merge_coin_vector(coins: Vec<Coin>, coin_vec_op: CoinVecOp) -> Vec<Coin> {
    let fund = coin_vec_op.fund;
    let operation = coin_vec_op.operation;

    match operation {
        Operation::Add => add_coin_vectors(&coins, &fund),
        Operation::Sub => subtract_coin_vectors(&coins, &fund),
        Operation::Replace => fund,
    }
}

/// return a * b
pub fn decimal_multiplication_in_256(a: Decimal, b: Decimal) -> Decimal {
    let a_u256: Decimal256 = a.into();
    let b_u256: Decimal256 = b.into();
    let c_u256: Decimal = (b_u256 * a_u256).into();
    c_u256
}

/// return a + b
pub fn decimal_summation_in_256(a: Decimal, b: Decimal) -> Decimal {
    let a_u256: Decimal256 = a.into();
    let b_u256: Decimal256 = b.into();
    let c_u256: Decimal = (b_u256 + a_u256).into();
    c_u256
}

/// return a - b
pub fn decimal_subtraction_in_256(a: Decimal, b: Decimal) -> Decimal {
    let a_u256: Decimal256 = a.into();
    let b_u256: Decimal256 = b.into();
    let c_u256: Decimal = (a_u256 - b_u256).into();
    c_u256
}

// Not to be used with Vec<{(120, "token1"), (30, "token1") ..}. No denom should be present more than once.
pub fn add_coin_vector_to_map(
    existing_coins: &mut HashMap<String, Uint128>,
    new_coins: &Vec<Coin>,
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
    existing_coins: &mut HashMap<String, Uint128>,
    new_coins: &Vec<Coin>,
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

// TODO - GM. Make these add & subtract coinvecs and deccoinvecs more efficient
fn add_coin_vectors(coins1: &Vec<Coin>, coins2: &Vec<Coin>) -> Vec<Coin> {
    let mut coin_map = add_coin_vector_to_map(&mut HashMap::new(), coins1);
    coin_map = add_coin_vector_to_map(&mut coin_map, coins2);
    map_to_coin_vec(coin_map)
}

fn subtract_coin_vectors(coins1: &Vec<Coin>, coins2: &Vec<Coin>) -> Vec<Coin> {
    let mut coin_map = add_coin_vector_to_map(&mut HashMap::new(), coins1);
    coin_map = subtract_coin_vector_from_map(&mut coin_map, coins2);
    map_to_coin_vec(coin_map)
}

pub fn multiply_coin_with_decimal(coin: &Coin, ratio: Decimal) -> Coin {
    Coin::new(
        coin.amount.u128() * ratio.numerator() / ratio.denominator(),
        coin.denom.clone(),
    )
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

#[cfg(test)]
mod tests {

    use super::*;
    use crate::contract::instantiate;
    use crate::msg::InstantiateMsg;
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
    };
    use cosmwasm_std::{Empty, OwnedDeps, Response, Timestamp};

    #[test]
    fn test__add_coin_vector_to_map() {
        let coin1 = Coin {
            amount: Uint128::new(35),
            denom: "token1".to_string(),
        };
        let coin2 = Coin {
            amount: Uint128::new(12),
            denom: "token2".to_string(),
        };
        let coin3 = Coin {
            amount: Uint128::new(82),
            denom: "token3".to_string(),
        };
        let coin4 = Coin {
            amount: Uint128::new(29),
            denom: "token1".to_string(),
        };
        let coin5 = Coin {
            amount: Uint128::new(11),
            denom: "token3".to_string(),
        };
        let coin6 = Coin {
            amount: Uint128::new(0),
            denom: "token6".to_string(),
        };
        let coin7 = Coin {
            amount: Uint128::new(3),
            denom: "token7".to_string(),
        };
        let vec1 = vec![coin1, coin2, coin3];
        let vec2 = vec![coin4, coin5, coin6, coin7];

        let mut total_rewards: HashMap<String, Uint128> = HashMap::new();
        total_rewards = add_coin_vector_to_map(&mut total_rewards, &vec1);
        assert_eq!(total_rewards.get("token1").unwrap().u128(), 35_u128);
        assert_eq!(total_rewards.get("token2").unwrap().u128(), 12_u128);
        assert_eq!(total_rewards.get("token3").unwrap().u128(), 82_u128);

        total_rewards = add_coin_vector_to_map(&mut total_rewards, &vec2);
        assert_eq!(total_rewards.get("token1").unwrap().u128(), 64_u128);
        assert_eq!(total_rewards.get("token2").unwrap().u128(), 12_u128);
        assert_eq!(total_rewards.get("token3").unwrap().u128(), 93_u128);
        assert_eq!(total_rewards.get("token6").unwrap().u128(), 0_u128);
        assert_eq!(total_rewards.get("token7").unwrap().u128(), 3_u128);
    }

    #[test]
    fn test__subtract_coin_vector_from_map() {
        let coin1 = Coin {
            amount: Uint128::new(35),
            denom: "token1".to_string(),
        };
        let coin2 = Coin {
            amount: Uint128::new(12),
            denom: "token2".to_string(),
        };
        let coin3 = Coin {
            amount: Uint128::new(82),
            denom: "token3".to_string(),
        };
        let coin4 = Coin {
            amount: Uint128::new(29),
            denom: "token1".to_string(),
        };
        let coin5 = Coin {
            amount: Uint128::new(12),
            denom: "token2".to_string(),
        };
        let coin6 = Coin {
            amount: Uint128::new(0),
            denom: "token3".to_string(),
        };
        let coin7 = Coin {
            amount: Uint128::new(3),
            denom: "token7".to_string(),
        };
        let vec1 = vec![coin1, coin2, coin3, coin7];
        let vec2 = vec![coin4, coin5, coin6];

        let mut total_rewards: HashMap<String, Uint128> = HashMap::new();
        total_rewards = add_coin_vector_to_map(&mut total_rewards, &vec1);
        assert_eq!(total_rewards.get("token1").unwrap().u128(), 35_u128);
        assert_eq!(total_rewards.get("token2").unwrap().u128(), 12_u128);
        assert_eq!(total_rewards.get("token3").unwrap().u128(), 82_u128);
        assert_eq!(total_rewards.get("token7").unwrap().u128(), 3_u128);

        total_rewards = subtract_coin_vector_from_map(&mut total_rewards, &vec2);
        assert_eq!(total_rewards.get("token1").unwrap().u128(), 6_u128);
        assert_eq!(total_rewards.get("token2").unwrap().u128(), 0_u128);
        assert_eq!(total_rewards.get("token3").unwrap().u128(), 82_u128);
        assert_eq!(total_rewards.get("token7").unwrap().u128(), 3_u128);
    }
}
