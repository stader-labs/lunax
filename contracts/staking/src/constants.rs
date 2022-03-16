use cosmwasm_std::Decimal;

pub fn get_deposit_fee_cap() -> Decimal {
    Decimal::from_ratio(5_u128, 100_u128)
}

pub fn get_withdraw_fee_cap() -> Decimal {
    Decimal::from_ratio(5_u128, 100_u128)
}

pub fn get_reward_fee_cap() -> Decimal {
    Decimal::from_ratio(10_u128, 100_u128)
}
