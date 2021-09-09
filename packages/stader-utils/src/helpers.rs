use cosmwasm_std::{Addr, BankMsg, Coin, Decimal, Fraction, Uint128};

pub fn send_funds_msg(recipient_addr: &Addr, funds: &Vec<Coin>) -> BankMsg {
    BankMsg::Send {
        to_address: String::from(recipient_addr),
        amount: funds
            .iter()
            .filter(|&x| !x.amount.is_zero())
            .cloned()
            .collect(),
    }
}

pub fn u128_from_decimal(a: Decimal) -> u128 {
    a.numerator() / a.denominator()
}

pub fn uint128_from_decimal(a: Decimal) -> Uint128 {
    Uint128::new(u128_from_decimal(a))
}
