use cosmwasm_bignumber::Decimal256;
use cosmwasm_std::{Coin, Decimal, Fraction, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::Display;

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, JsonSchema)]
pub struct DecCoin {
    pub amount: Decimal,
    pub denom: String,
}

impl DecCoin {
    pub fn new<S: Into<String>>(amount: Decimal, denom: S) -> Self {
        DecCoin {
            amount,
            denom: denom.into(),
        }
    }
}

impl Display for DecCoin {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // We use the formatting without a space between amount and denom,
        // which is common in the Cosmos SDK ecosystem:
        // https://github.com/cosmos/cosmos-sdk/blob/v0.42.4/types/coin.go#L643-L645
        // For communication to end users, Coin needs to transformed anways (e.g. convert integer uatom to decimal ATOM).
        write!(f, "{}{}", self.amount, self.denom)
    }
}

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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct DecCoinVecOp {
    pub fund: Vec<DecCoin>,
    pub operation: Operation,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct DecimalOp {
    pub fund: Decimal,
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

// TODO - GM. This has to be a trait with DecCoin.
pub fn check_equal_deccoin_vector(deccoins1: &[DecCoin], deccoins2: &[DecCoin]) -> bool {
    deccoins1.len() == deccoins2.len()
        && deccoins1.iter().all(|x| deccoins2.contains(x))
        && deccoins2.iter().all(|x| deccoins1.contains(x))
}

pub fn check_equal_coin_vector(coins1: &[Coin], coins2: &[Coin]) -> bool {
    coins1.len() == coins2.len()
        && coins1.iter().all(|x| coins2.contains(x))
        && coins2.iter().all(|x| coins1.contains(x))
}

// TODO - GM. Generalize map_to_vec for Coin and DecCoin
pub fn map_to_coin_vec(coin_map: HashMap<String, Uint128>) -> Vec<Coin> {
    let mut coins: Vec<Coin> = vec![];
    let mut sorted_coin_map: Vec<(&String, &Uint128)> =
        coin_map.iter().collect::<Vec<(&String, &Uint128)>>();
    sorted_coin_map.sort();
    for denom_amount in coin_map.iter() {
        let denom = denom_amount.0.clone();
        let amount = *denom_amount.1;
        coins.push(Coin { denom, amount })
    }
    coins
}

pub fn map_to_deccoin_vec(coin_map: HashMap<String, Decimal>) -> Vec<DecCoin> {
    let mut coins: Vec<DecCoin> = vec![];
    let mut sorted_coin_map: Vec<(&String, &Decimal)> =
        coin_map.iter().collect::<Vec<(&String, &Decimal)>>();
    sorted_coin_map.sort();
    for denom_amount in coin_map.iter() {
        let denom = denom_amount.0.clone();
        let amount = *denom_amount.1;
        coins.push(DecCoin { denom, amount })
    }
    coins
}

// Jumbles the order of the vector
// (Coins + CoinVecOp.fund) and (Coins - CoinVecOp.fund) [Element wise operation but Sub is stricter than set operation]
pub fn merge_dec_coin_vector(deccoins: &[DecCoin], deccoin_vec_op: DecCoinVecOp) -> Vec<DecCoin> {
    let fund = deccoin_vec_op.fund;
    let operation = deccoin_vec_op.operation;

    match operation {
        Operation::Add => add_deccoin_vectors(deccoins, &fund),
        Operation::Sub => subtract_deccoin_vectors(deccoins, &fund),
        Operation::Replace => fund,
    }
}

// Jumbles the order of the vector
// (Coins + CoinVecOp.fund) and (Coins - CoinVecOp.fund) [Element wise operation but Sub is stricter than set operation]
pub fn merge_coin_vector(coins: &[Coin], coin_vec_op: CoinVecOp) -> Vec<Coin> {
    let fund = coin_vec_op.fund;
    let operation = coin_vec_op.operation;

    match operation {
        Operation::Add => add_coin_vectors(coins, &fund),
        Operation::Sub => subtract_coin_vectors(coins, &fund),
        Operation::Replace => fund,
    }
}

/// return a * b
pub fn decimal_division_in_256(a: Decimal, b: Decimal) -> Decimal {
    let a_u256: Decimal256 = a.into();
    let b_u256: Decimal256 = b.into();
    let c_u256: Decimal = (a_u256 / b_u256).into();
    c_u256
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

pub fn u128_from_decimal(a: Decimal) -> u128 {
    a.numerator() / a.denominator()
}

pub fn uint128_from_decimal(a: Decimal) -> Uint128 {
    Uint128::new(u128_from_decimal(a))
}

pub fn get_decimal_from_uint128(a: Uint128) -> Decimal {
    Decimal::from_ratio(a, 1_u128)
}

pub fn merge_decimal(decimal1: Decimal, decimal_op: DecimalOp) -> Decimal {
    let fund = decimal_op.fund;
    let operation = decimal_op.operation;

    match operation {
        Operation::Add => decimal_summation_in_256(decimal1, fund),
        Operation::Sub => {
            if decimal1 < fund {
                panic!(
                    "Cannot make decimal with negative value {}-{}",
                    decimal1.to_string(),
                    fund.to_string()
                )
            }
            decimal_subtraction_in_256(decimal1, fund)
        }
        Operation::Replace => fund, // _ => panic!("Unknown operation type {:?}", operation)
    }
}

// Not to be used with Vec<{(120, "token1"), (30, "token1") ..}. No denom should be present more than once.
pub fn add_coin_vector_to_map(
    existing_coins: &mut HashMap<String, Uint128>,
    new_coins: &[Coin],
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
    new_coins: &[Coin],
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

// Not to be used with Vec<{(120/200, "token1"), (30/23, "token1") ..}. No denom should be present more than once.
pub fn add_deccoin_vector_to_map(
    existing_deccoins: &mut HashMap<String, Decimal>,
    new_deccoins: &[DecCoin],
) -> HashMap<String, Decimal> {
    let mut accumulated_coins: HashMap<String, Decimal> = existing_deccoins.clone();
    let mut denom_set: HashSet<String> = HashSet::new();
    for dec_coin in new_deccoins {
        if existing_deccoins.contains_key(&dec_coin.denom) {
            if denom_set.contains(&dec_coin.denom) {
                panic!("Multiple coins of same denom found {}", &dec_coin.denom);
            } else {
                denom_set.insert(dec_coin.denom.clone());
            }

            let existing_decimal = existing_deccoins.get(&dec_coin.denom).unwrap();
            accumulated_coins.insert(
                dec_coin.denom.clone(),
                decimal_summation_in_256(dec_coin.amount, *existing_decimal),
            );
        } else {
            accumulated_coins.insert(dec_coin.denom.clone(), dec_coin.amount);
        }
    }
    accumulated_coins
}

// Not to be used with Vec<{(120/200, "token1"), (30/23, "token1") ..}. No denom should be present more than once.
// (existing_deccoins - new_deccoins) vector subtraction.
pub fn subtract_deccoin_vector_from_map(
    existing_deccoins: &mut HashMap<String, Decimal>,
    new_deccoins: &[DecCoin],
) -> HashMap<String, Decimal> {
    let mut dissipated_coins: HashMap<String, Decimal> = existing_deccoins.clone();
    let mut denom_set: HashSet<String> = HashSet::new();
    for dec_coin in new_deccoins {
        if existing_deccoins.contains_key(&dec_coin.denom) {
            if denom_set.contains(&dec_coin.denom) {
                panic!("Multiple coins of same denom found {}", dec_coin.denom);
            } else {
                denom_set.insert(dec_coin.denom.clone());
            }

            let existing_decimal = existing_deccoins.get(&dec_coin.denom).unwrap();

            if existing_decimal.lt(&dec_coin.amount) {
                panic!(
                    "Cannot subtract {:?}-{:?} for denom {:?}",
                    existing_decimal, &dec_coin.amount, dec_coin.denom
                );
            }

            dissipated_coins.insert(
                dec_coin.denom.clone(),
                decimal_subtraction_in_256(*existing_decimal, dec_coin.amount),
            );
        } else {
            panic!(
                "Cannot subtract {:?} for denom {:?} because there is no prior coin",
                &dec_coin.amount, dec_coin.denom
            );
        }
    }
    dissipated_coins
}

pub fn filter_by_denom(coin_vector: &[Coin], denoms: Vec<String>) -> Vec<Coin> {
    coin_vector
        .iter()
        .filter(|&x| denoms.contains(&x.denom))
        .cloned()
        .collect()
}

pub fn filter_by_other_denom(coin_vector: &[Coin], denoms: Vec<String>) -> Vec<Coin> {
    coin_vector
        .iter()
        .filter(|&x| !denoms.contains(&x.denom))
        .cloned()
        .collect()
}

// TODO - GM. Make these add & subtract coinvecs and deccoinvecs more efficient
fn add_coin_vectors(coins1: &[Coin], coins2: &[Coin]) -> Vec<Coin> {
    let mut coin_map = add_coin_vector_to_map(&mut HashMap::new(), coins1);
    coin_map = add_coin_vector_to_map(&mut coin_map, coins2);
    map_to_coin_vec(coin_map)
}

fn subtract_coin_vectors(coins1: &[Coin], coins2: &[Coin]) -> Vec<Coin> {
    let mut coin_map = add_coin_vector_to_map(&mut HashMap::new(), coins1);
    coin_map = subtract_coin_vector_from_map(&mut coin_map, coins2);
    map_to_coin_vec(coin_map)
}

fn add_deccoin_vectors(deccoin1: &[DecCoin], deccoin2: &[DecCoin]) -> Vec<DecCoin> {
    let mut deccoin_map = add_deccoin_vector_to_map(&mut HashMap::new(), deccoin1);
    deccoin_map = add_deccoin_vector_to_map(&mut deccoin_map, deccoin2);
    map_to_deccoin_vec(deccoin_map)
}

fn subtract_deccoin_vectors(deccoin1: &[DecCoin], deccoin2: &[DecCoin]) -> Vec<DecCoin> {
    let mut deccoin_map = add_deccoin_vector_to_map(&mut HashMap::new(), deccoin1);
    deccoin_map = subtract_deccoin_vector_from_map(&mut deccoin_map, deccoin2);
    map_to_deccoin_vec(deccoin_map)
}

pub fn multiply_deccoin_vector_with_decimal(coins: &[DecCoin], ratio: Decimal) -> Vec<DecCoin> {
    let mut result: Vec<DecCoin> = vec![];
    for deccoin in coins {
        let decimal = decimal_multiplication_in_256(deccoin.amount, ratio);
        result.push(DecCoin {
            denom: deccoin.denom.clone(),
            amount: decimal,
        });
    }
    result
}

pub fn multiply_deccoin_vector_with_uint128(deccoins: &[DecCoin], amount: Uint128) -> Vec<DecCoin> {
    let mut result: Vec<DecCoin> = vec![];
    if amount.is_zero() {
        return vec![];
    }
    for deccoin in deccoins {
        if deccoin.amount.is_zero() {
            continue;
        }
        let decimal = decimal_multiplication_in_256(
            deccoin.amount,
            Decimal::from_ratio(amount.u128(), 1_u128),
        );
        result.push(DecCoin {
            denom: deccoin.denom.clone(),
            amount: decimal,
        });
    }
    result
}

pub fn multiply_coin_with_decimal(coin: &Coin, ratio: Decimal) -> Coin {
    Coin::new(
        coin.amount.u128() * ratio.numerator() / ratio.denominator(),
        coin.denom.clone(),
    )
}
pub fn multiply_u128_with_decimal(num: u128, dec: Decimal) -> u128 {
    (num * dec.numerator() / dec.denominator()) as u128
}

pub fn coin_to_deccoin(coin: Coin) -> DecCoin {
    DecCoin {
        amount: Decimal::from_ratio(coin.amount, Uint128::new(1_u128)),
        denom: coin.denom,
    }
}

pub fn deccoin_to_coin(deccoin: DecCoin) -> Coin {
    Coin::new(
        deccoin.amount.numerator() / deccoin.amount.denominator(),
        deccoin.denom,
    )
}

pub fn coin_vec_to_deccoin_vec(coins: &[Coin]) -> Vec<DecCoin> {
    coins
        .iter()
        .map(|coin| coin_to_deccoin(coin.clone()))
        .collect()
}

pub fn deccoin_vec_to_coin_vec(deccoins: &[DecCoin]) -> Vec<Coin> {
    deccoins
        .iter()
        .map(|deccoin| deccoin_to_coin(deccoin.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::coin_utils::{
        add_coin_vector_to_map, add_deccoin_vector_to_map, check_equal_coin_vector,
        check_equal_deccoin_vector, decimal_division_in_256, decimal_multiplication_in_256,
        decimal_subtraction_in_256, decimal_summation_in_256, map_to_coin_vec, map_to_deccoin_vec,
        merge_coin, merge_coin_vector, merge_dec_coin_vector, merge_decimal,
        subtract_coin_vector_from_map, subtract_deccoin_vector_from_map, CoinOp, CoinVecOp,
        DecCoin, DecCoinVecOp, DecimalOp, Operation,
    };
    use cosmwasm_std::{Coin, Decimal, Fraction, Uint128};
    use std::borrow::Borrow;
    use std::collections::HashMap;

    pub fn check_equal_vec<S: PartialEq>(v1: Vec<S>, v2: Vec<S>) -> bool {
        v1.len() == v2.len()
            && v1.iter().all(|x| v2.contains(x))
            && v2.iter().all(|x| v1.contains(x))
    }

    macro_rules! hashmap {
        ($( $key: expr => $val: expr ),*) => {{
             let mut map = ::std::collections::HashMap::new();
             $( map.insert($key, $val); )*
             map
        }}
    }

    #[test]
    fn test_add_coin_vector_to_map() {
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
    fn test_subtract_coin_vector_from_map() {
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

    #[test]
    fn test_add_deccoin_vector_to_map() {
        let deccoin1 = DecCoin {
            amount: Decimal::from_ratio(4_u128, 23_u128),
            denom: "token1".to_string(),
        };
        let deccoin2 = DecCoin {
            amount: Decimal::from_ratio(12_u128, 23_u128),
            denom: "token2".to_string(),
        };
        let deccoin3 = DecCoin {
            amount: Decimal::from_ratio(14_u128, 23_u128),
            denom: "token3".to_string(),
        };
        let deccoin4 = DecCoin {
            amount: Decimal::from_ratio(19_u128, 23_u128),
            denom: "token1".to_string(),
        };
        let deccoin5 = DecCoin {
            amount: Decimal::from_ratio(0_u128, 23_u128),
            denom: "token3".to_string(),
        };
        let deccoin6 = DecCoin {
            amount: Decimal::from_ratio(4_u128, 23_u128),
            denom: "token4".to_string(),
        };
        let vec1 = vec![deccoin1, deccoin2, deccoin3];
        let vec2 = vec![deccoin4, deccoin5, deccoin6];

        let mut total_rewards: HashMap<String, Decimal> = HashMap::new();
        total_rewards = add_deccoin_vector_to_map(&mut total_rewards, &vec1);
        assert_eq!(
            total_rewards.get("token1").unwrap().numerator(),
            173913043478260869_u128
        );
        assert_eq!(
            total_rewards.get("token1").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
        assert_eq!(
            total_rewards.get("token2").unwrap().numerator(),
            521739130434782608_u128
        );
        assert_eq!(
            total_rewards.get("token2").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
        assert_eq!(
            total_rewards.get("token3").unwrap().numerator(),
            608695652173913043_u128
        );
        assert_eq!(
            total_rewards.get("token3").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );

        total_rewards = add_deccoin_vector_to_map(&mut total_rewards, &vec2);
        assert_eq!(
            total_rewards.get("token1").unwrap().numerator(),
            999999999999999999_u128
        );
        assert_eq!(
            total_rewards.get("token1").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
        assert_eq!(
            total_rewards.get("token2").unwrap().numerator(),
            521739130434782608_u128
        );
        assert_eq!(
            total_rewards.get("token2").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
        assert_eq!(
            total_rewards.get("token3").unwrap().numerator(),
            608695652173913043_u128
        );
        assert_eq!(
            total_rewards.get("token3").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
        assert_eq!(
            total_rewards.get("token4").unwrap().numerator(),
            173913043478260869_u128
        );
        assert_eq!(
            total_rewards.get("token4").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
    }

    #[test]
    fn test_subtract_deccoin_vector_to_map() {
        let deccoin1 = DecCoin {
            amount: Decimal::from_ratio(4_u128, 23_u128),
            denom: "token1".to_string(),
        };
        let deccoin2 = DecCoin {
            amount: Decimal::from_ratio(12_u128, 23_u128),
            denom: "token2".to_string(),
        };
        let deccoin3 = DecCoin {
            amount: Decimal::from_ratio(14_u128, 23_u128),
            denom: "token3".to_string(),
        };
        let deccoin4 = DecCoin {
            amount: Decimal::from_ratio(4_u128, 23_u128),
            denom: "token1".to_string(),
        };
        let deccoin5 = DecCoin {
            amount: Decimal::from_ratio(10_u128, 23_u128),
            denom: "token2".to_string(),
        };
        let deccoin6 = DecCoin {
            amount: Decimal::from_ratio(0_u128, 23_u128),
            denom: "token3".to_string(),
        };
        let deccoin7 = DecCoin {
            amount: Decimal::from_ratio(4_u128, 23_u128),
            denom: "token4".to_string(),
        };

        let vec1 = vec![deccoin1, deccoin2, deccoin3, deccoin7];
        let vec2 = vec![deccoin4, deccoin5, deccoin6];

        let mut total_rewards: HashMap<String, Decimal> = HashMap::new();
        total_rewards = add_deccoin_vector_to_map(&mut total_rewards, &vec1);
        assert_eq!(
            total_rewards.get("token1").unwrap().numerator(),
            173913043478260869_u128
        );
        assert_eq!(
            total_rewards.get("token1").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
        assert_eq!(
            total_rewards.get("token2").unwrap().numerator(),
            521739130434782608_u128
        );
        assert_eq!(
            total_rewards.get("token2").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
        assert_eq!(
            total_rewards.get("token3").unwrap().numerator(),
            608695652173913043_u128
        );
        assert_eq!(
            total_rewards.get("token3").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
        assert_eq!(
            total_rewards.get("token4").unwrap().numerator(),
            173913043478260869_u128
        );
        assert_eq!(
            total_rewards.get("token4").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );

        total_rewards = subtract_deccoin_vector_from_map(&mut total_rewards, &vec2);
        assert_eq!(total_rewards.get("token1").unwrap().numerator(), 0_u128);
        assert_eq!(
            total_rewards.get("token1").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
        assert_eq!(
            total_rewards.get("token2").unwrap().numerator(),
            86_956_521_739_130_435_u128
        );
        assert_eq!(
            total_rewards.get("token2").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
        assert_eq!(
            total_rewards.get("token3").unwrap().numerator(),
            608695652173913043_u128
        );
        assert_eq!(
            total_rewards.get("token3").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
        assert_eq!(
            total_rewards.get("token4").unwrap().numerator(),
            173913043478260869_u128
        );
        assert_eq!(
            total_rewards.get("token4").unwrap().denominator(),
            1_000_000_000_000_000_000_u128
        );
    }

    #[test]
    fn test_decimal_summation_in_256() {
        let a = Decimal::from_ratio(5_u128, 1_u128);
        let b = Decimal::from_ratio(10_u128, 1_u128);

        let res = decimal_summation_in_256(a, b);

        assert_eq!(res, Decimal::from_ratio(15_u128, 1_u128))
    }

    #[test]
    #[should_panic]
    fn test_decimal_subtraction_in_256_underflow() {
        let a = Decimal::from_ratio(5_u128, 1_u128);
        let b = Decimal::from_ratio(10_u128, 1_u128);

        decimal_subtraction_in_256(a, b);
    }

    #[test]
    fn test_decimal_subtraction_in_256() {
        let a = Decimal::from_ratio(5_u128, 1_u128);
        let b = Decimal::from_ratio(10_u128, 1_u128);

        let res = decimal_subtraction_in_256(b, a);

        assert_eq!(res, Decimal::from_ratio(5_u128, 1_u128))
    }

    #[test]
    fn test_decimal_multiplication_in_256() {
        let a = Decimal::from_ratio(5_u128, 1_u128);
        let b = Decimal::from_ratio(10_u128, 1_u128);

        let res = decimal_multiplication_in_256(b, a);

        assert_eq!(res, Decimal::from_ratio(50_u128, 1_u128))
    }

    #[test]
    #[should_panic]
    fn test_decimal_division_in_256_divide_by_zero() {
        let a = Decimal::from_ratio(5_u128, 1_u128);
        let b = Decimal::zero();

        decimal_division_in_256(a, b);
    }

    #[test]
    fn test_decimal_division_in_256() {
        let a = Decimal::from_ratio(50_u128, 1_u128);
        let b = Decimal::from_ratio(10_u128, 1_u128);

        let res = decimal_division_in_256(a, b);

        assert_eq!(res, Decimal::from_ratio(5_u128, 1_u128));
    }

    #[test]
    fn test_merge_coin_add() {
        let coin1 = Coin::new(100_u128, "uluna".to_string());

        let res = merge_coin(
            coin1,
            CoinOp {
                fund: Coin::new(100_u128, "uluna".to_string()),
                operation: Operation::Add,
            },
        );

        assert_eq!(res, Coin::new(200_u128, "uluna".to_string()))
    }

    #[test]
    #[should_panic]
    fn test_merge_coin_sub_underflow() {
        let coin1 = Coin::new(100_u128, "uluna".to_string());

        merge_coin(
            coin1,
            CoinOp {
                fund: Coin::new(200_u128, "uluna".to_string()),
                operation: Operation::Sub,
            },
        );
    }

    #[test]
    fn test_merge_coin_sub() {
        let coin1 = Coin::new(100_u128, "uluna".to_string());

        let res = merge_coin(
            coin1,
            CoinOp {
                fund: Coin::new(50_u128, "uluna".to_string()),
                operation: Operation::Sub,
            },
        );

        assert_eq!(res, Coin::new(50_u128, "uluna".to_string()))
    }

    #[test]
    fn test_merge_coin_replace() {
        let coin1 = Coin::new(100_u128, "uluna".to_string());

        let res = merge_coin(
            coin1,
            CoinOp {
                fund: Coin::new(90_u128, "uluna".to_string()),
                operation: Operation::Replace,
            },
        );

        assert_eq!(res, Coin::new(90_u128, "uluna".to_string()))
    }

    #[test]
    fn test_check_equal_deccoin_vector() {
        let a = vec![
            DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uluna".to_string()),
            DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "anc".to_string()),
            DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "mir".to_string()),
        ];
        let b = vec![
            DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "mir".to_string()),
            DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uluna".to_string()),
            DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "anc".to_string()),
        ];

        let res = check_equal_deccoin_vector(a.borrow(), b.borrow());
        assert!(res);

        let a = vec![
            DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uluna".to_string()),
            DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "anc".to_string()),
            DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "mir".to_string()),
        ];
        let b = vec![
            DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "mir".to_string()),
            DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uluna".to_string()),
            DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "anc".to_string()),
        ];

        let res = check_equal_deccoin_vector(a.borrow(), b.borrow());
        assert!(!res);
    }

    #[test]
    fn test_check_equal_coin_vector() {
        let a = vec![
            Coin::new(100_u128, "uluna".to_string()),
            Coin::new(200_u128, "anc".to_string()),
            Coin::new(300_u128, "mir".to_string()),
        ];
        let b = vec![
            Coin::new(300_u128, "mir".to_string()),
            Coin::new(100_u128, "uluna".to_string()),
            Coin::new(200_u128, "anc".to_string()),
        ];

        let res = check_equal_coin_vector(a.borrow(), b.borrow());
        assert!(res);

        let a = vec![
            Coin::new(100_u128, "uluna".to_string()),
            Coin::new(300_u128, "anc".to_string()),
            Coin::new(300_u128, "mir".to_string()),
        ];
        let b = vec![
            Coin::new(300_u128, "mir".to_string()),
            Coin::new(100_u128, "uluna".to_string()),
            Coin::new(200_u128, "anc".to_string()),
        ];

        let res = check_equal_coin_vector(a.borrow(), b.borrow());
        assert!(!res);
    }

    #[test]
    fn test_map_to_coin_vec() {
        let coin_map = hashmap!("uluna".to_string() => Uint128::new(100_u128), "uusd".to_string() => Uint128::new(200_u128), "ukrt".to_string() => Uint128::new(300_u128));

        let coin_vec = map_to_coin_vec(coin_map);

        assert!(check_equal_vec(
            coin_vec,
            vec![
                Coin::new(100_u128, "uluna".to_string()),
                Coin::new(200_u128, "uusd".to_string()),
                Coin::new(300_u128, "ukrt".to_string())
            ]
        ))
    }

    #[test]
    fn test_map_to_deccoin_vec() {
        let deccoin_map = hashmap!("uluna".to_string() => Decimal::from_ratio(100_u128, 1_u128), "uusd".to_string() => Decimal::from_ratio(200_u128, 1_u128), "ukrt".to_string() => Decimal::from_ratio(300_u128, 1_u128));

        let deccoin_vec = map_to_deccoin_vec(deccoin_map);

        assert!(check_equal_vec(
            deccoin_vec,
            vec![
                DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uluna".to_string()),
                DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "uusd".to_string()),
                DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "ukrt".to_string()),
            ]
        ))
    }

    #[test]
    fn test_merge_decimal_add() {
        let a = Decimal::from_ratio(100_u128, 1_u128);
        let b = Decimal::from_ratio(200_u128, 1_u128);

        let res = merge_decimal(
            a,
            DecimalOp {
                fund: b,
                operation: Operation::Add,
            },
        );

        assert_eq!(res, Decimal::from_ratio(300_u128, 1_u128));
    }

    #[test]
    #[should_panic]
    fn test_merge_decimal_sub_underflow() {
        let a = Decimal::from_ratio(100_u128, 1_u128);
        let b = Decimal::from_ratio(200_u128, 1_u128);

        let res = merge_decimal(
            a,
            DecimalOp {
                fund: b,
                operation: Operation::Sub,
            },
        );

        assert_eq!(res, Decimal::from_ratio(300_u128, 1_u128));
    }

    #[test]
    fn test_merge_decimal_sub() {
        let a = Decimal::from_ratio(100_u128, 1_u128);
        let b = Decimal::from_ratio(200_u128, 1_u128);

        let res = merge_decimal(
            b,
            DecimalOp {
                fund: a,
                operation: Operation::Sub,
            },
        );

        assert_eq!(res, Decimal::from_ratio(100_u128, 1_u128));
    }

    #[test]
    fn test_merge_decimal_replace() {
        let a = Decimal::from_ratio(100_u128, 1_u128);
        let b = Decimal::from_ratio(230_u128, 1_u128);

        let res = merge_decimal(
            a,
            DecimalOp {
                fund: b,
                operation: Operation::Replace,
            },
        );

        assert_eq!(res, Decimal::from_ratio(230_u128, 1_u128));
    }

    #[test]
    fn test_merge_coin_vector_add() {
        let a = vec![
            Coin::new(100_u128, "uluna".to_string()),
            Coin::new(200_u128, "uusd".to_string()),
            Coin::new(300_u128, "ukrt".to_string()),
        ];
        let b = vec![
            Coin::new(100_u128, "uusd".to_string()),
            Coin::new(200_u128, "ukrt".to_string()),
            Coin::new(300_u128, "uluna".to_string()),
        ];

        let res = merge_coin_vector(
            a.borrow(),
            CoinVecOp {
                fund: b,
                operation: Operation::Add,
            },
        );

        assert!(check_equal_vec(
            res,
            vec![
                Coin::new(400_u128, "uluna".to_string()),
                Coin::new(300_u128, "uusd".to_string()),
                Coin::new(500_u128, "ukrt".to_string())
            ]
        ));
    }

    #[test]
    fn test_merge_coin_vector_replace() {
        let a = vec![
            Coin::new(100_u128, "uluna".to_string()),
            Coin::new(200_u128, "uusd".to_string()),
            Coin::new(300_u128, "ukrt".to_string()),
        ];
        let b = vec![
            Coin::new(100_u128, "uusd".to_string()),
            Coin::new(200_u128, "ukrt".to_string()),
            Coin::new(300_u128, "uluna".to_string()),
        ];

        let res = merge_coin_vector(
            a.borrow(),
            CoinVecOp {
                fund: b,
                operation: Operation::Replace,
            },
        );

        assert!(check_equal_vec(
            res,
            vec![
                Coin::new(100_u128, "uusd".to_string()),
                Coin::new(200_u128, "ukrt".to_string()),
                Coin::new(300_u128, "uluna".to_string())
            ]
        ));
    }

    #[test]
    #[should_panic]
    fn test_merge_coin_vector_sub_underflow() {
        let a = vec![
            Coin::new(100_u128, "uluna".to_string()),
            Coin::new(200_u128, "uusd".to_string()),
            Coin::new(300_u128, "ukrt".to_string()),
        ];
        let b = vec![
            Coin::new(100_u128, "uusd".to_string()),
            Coin::new(200_u128, "ukrt".to_string()),
            Coin::new(300_u128, "uluna".to_string()),
        ];

        merge_coin_vector(
            b.borrow(),
            CoinVecOp {
                fund: a,
                operation: Operation::Sub,
            },
        );
    }

    #[test]
    fn test_merge_coin_vector_sub() {
        let a = vec![
            Coin::new(100_u128, "uluna".to_string()),
            Coin::new(200_u128, "uusd".to_string()),
            Coin::new(300_u128, "ukrt".to_string()),
        ];
        let b = vec![
            Coin::new(100_u128, "uusd".to_string()),
            Coin::new(200_u128, "ukrt".to_string()),
            Coin::new(50_u128, "uluna".to_string()),
        ];

        let res = merge_coin_vector(
            a.borrow(),
            CoinVecOp {
                fund: b,
                operation: Operation::Sub,
            },
        );

        assert!(check_equal_vec(
            res,
            vec![
                Coin::new(50_u128, "uluna".to_string()),
                Coin::new(100_u128, "ukrt".to_string()),
                Coin::new(100_u128, "uusd".to_string())
            ]
        ));
    }

    #[test]
    fn test_merge_deccoin_vector_add() {
        let a = vec![
            DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uluna".to_string()),
            DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "uusd".to_string()),
            DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "ukrt".to_string()),
        ];
        let b = vec![
            DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uusd".to_string()),
            DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "ukrt".to_string()),
            DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "uluna".to_string()),
        ];

        let res = merge_dec_coin_vector(
            a.borrow(),
            DecCoinVecOp {
                fund: b,
                operation: Operation::Add,
            },
        );

        assert!(check_equal_vec(
            res,
            vec![
                DecCoin::new(Decimal::from_ratio(400_u128, 1_u128), "uluna".to_string()),
                DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "uusd".to_string()),
                DecCoin::new(Decimal::from_ratio(500_u128, 1_u128), "ukrt".to_string())
            ]
        ));
    }

    #[test]
    fn test_merge_deccoin_vector_replace() {
        let a = vec![
            DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uluna".to_string()),
            DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "uusd".to_string()),
            DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "ukrt".to_string()),
        ];
        let b = vec![
            DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uusd".to_string()),
            DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "ukrt".to_string()),
            DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "uluna".to_string()),
        ];

        let res = merge_dec_coin_vector(
            a.borrow(),
            DecCoinVecOp {
                fund: b,
                operation: Operation::Replace,
            },
        );

        assert!(check_equal_vec(
            res,
            vec![
                DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uusd".to_string()),
                DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "ukrt".to_string()),
                DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "uluna".to_string())
            ]
        ));
    }

    #[test]
    #[should_panic]
    fn test_merge_deccoin_vector_sub_underflow() {
        let a = vec![
            DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uluna".to_string()),
            DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "uusd".to_string()),
            DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "ukrt".to_string()),
        ];
        let b = vec![
            DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uusd".to_string()),
            DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "ukrt".to_string()),
            DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "uluna".to_string()),
        ];

        merge_dec_coin_vector(
            b.borrow(),
            DecCoinVecOp {
                fund: a,
                operation: Operation::Sub,
            },
        );
    }

    #[test]
    fn test_merge_deccoin_vector_sub() {
        let a = vec![
            DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uluna".to_string()),
            DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "uusd".to_string()),
            DecCoin::new(Decimal::from_ratio(300_u128, 1_u128), "ukrt".to_string()),
        ];
        let b = vec![
            DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uusd".to_string()),
            DecCoin::new(Decimal::from_ratio(200_u128, 1_u128), "ukrt".to_string()),
            DecCoin::new(Decimal::from_ratio(50_u128, 1_u128), "uluna".to_string()),
        ];

        let res = merge_dec_coin_vector(
            a.borrow(),
            DecCoinVecOp {
                fund: b,
                operation: Operation::Sub,
            },
        );

        assert!(check_equal_vec(
            res,
            vec![
                DecCoin::new(Decimal::from_ratio(50_u128, 1_u128), "uluna".to_string()),
                DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "ukrt".to_string()),
                DecCoin::new(Decimal::from_ratio(100_u128, 1_u128), "uusd".to_string())
            ]
        ));
    }
}
