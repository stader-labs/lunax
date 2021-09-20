use cosmwasm_std::{Addr, BankMsg, Coin, Decimal, Fraction, QuerierWrapper, Uint128};
use std::collections::HashMap;
use terra_cosmwasm::TerraQuerier;

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

// Skips denoms whose exchange rate cannot be found.
pub fn query_exchange_rates(
    querier: QuerierWrapper,
    base_denom: String,
    quote_denoms: &Vec<String>,
) -> HashMap<String, Decimal> {
    let querier = TerraQuerier::new(&querier);
    let mut er_map: HashMap<String, Decimal> = HashMap::new();
    for denom in quote_denoms {
        if denom.eq(&base_denom) {
            er_map.insert(denom.clone(), Decimal::one());
            continue;
        }
        let result = querier.query_exchange_rates(denom.clone(), vec![base_denom.to_string()]);
        if result.is_err() {
            continue;
        }
        let exchange_rate_response = result.unwrap();
        let exchange_rate_item = exchange_rate_response.exchange_rates.first().unwrap();
        er_map.insert(denom.clone(), exchange_rate_item.exchange_rate);
    }
    er_map
}
