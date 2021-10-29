use cosmwasm_std::{Addr, BankMsg, Coin, Decimal, QuerierWrapper};
use std::collections::HashMap;
use terra_cosmwasm::TerraQuerier;

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