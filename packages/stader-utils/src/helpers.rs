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

pub fn query_cw20_token_balance(
    querier: QuerierWrapper,
    cw20_token_address: Addr,
    address: String
) -> StdResult<Uint128> {
    let res: BalanceResponse = querier.query_wasm_smart(cw20_token_address.to_string(), &cw20::Cw20QueryMsg::Balance {
        address
    })?;

    Ok(res.balance)
}