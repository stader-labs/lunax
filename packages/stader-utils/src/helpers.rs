use cosmwasm_std::{Addr, BankMsg, Coin};

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
