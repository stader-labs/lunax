use crate::state::ValidatorMetrics;
use cosmwasm_bignumber::Decimal256;
use cosmwasm_std::{Decimal, Uint128};

pub fn decimal_summation_in_256(a: Decimal, b: Decimal) -> Decimal {
    let a_u256: Decimal256 = a.into();
    let b_u256: Decimal256 = b.into();
    let c_u256: Decimal = (b_u256 + a_u256).into();
    c_u256
}

pub fn clamp(min: u32, value: u32, max: u32) -> u32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

pub fn decimal_subtraction_in_256(a: Decimal, b: Decimal) -> Decimal {
    let a_u256: Decimal256 = a.into();
    let b_u256: Decimal256 = b.into();
    let c_u256: Decimal = (a_u256 - b_u256).into();
    c_u256
}

pub fn decimal_multiplication_in_256(a: Decimal, b: Decimal) -> Decimal {
    let a_u256: Decimal256 = a.into();
    let b_u256: Decimal256 = b.into();
    let c_u256: Decimal = (b_u256 * a_u256).into();
    c_u256
}

pub fn decimal_division_in_256(a: Decimal, b: Decimal) -> Decimal {
    let a_u256: Decimal256 = a.into();
    let b_u256: Decimal256 = b.into();
    let c_u256: Decimal = (a_u256 / b_u256).into();
    c_u256
}

pub fn uint128_to_decimal(num: Uint128) -> Decimal {
    let numerator: u128 = num.into();
    Decimal::from_ratio(numerator, 1_u128)
}

pub fn u64_to_decimal(num: u64) -> Decimal {
    let numerator: u128 = num.into();
    Decimal::from_ratio(numerator, 1_u128)
}

pub fn compute_apr(
    h1: &ValidatorMetrics,
    h2: &ValidatorMetrics,
    time_diff_in_seconds: u64,
) -> Decimal {
    let numerator = decimal_multiplication_in_256(
        decimal_subtraction_in_256(h2.rewards, h1.rewards),
        u64_to_decimal(3153600000), // (365 * 86400) * 100 => (365 * 86400) = Seconds in an year, 100 = percentage
    );

    let denominator = decimal_multiplication_in_256(
        uint128_to_decimal(h2.delegated_amount),
        u64_to_decimal(time_diff_in_seconds),
    );

    decimal_division_in_256(numerator, denominator)
}
