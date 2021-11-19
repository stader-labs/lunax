use cosmwasm_std::{Addr, BankMsg, Coin, QuerierWrapper, StdResult, Uint128};
use cw20::BalanceResponse;

pub fn send_funds_msg(recipient_addr: &Addr, funds: &[Coin]) -> BankMsg {
    BankMsg::Send {
        to_address: String::from(recipient_addr),
        amount: funds
            .iter()
            .filter(|&x| !x.amount.is_zero())
            .cloned()
            .collect(),
    }
}
