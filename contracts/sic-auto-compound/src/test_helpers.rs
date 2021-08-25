use cosmwasm_std::{BankMsg, CosmosMsg};
use terra_cosmwasm::TerraMsgWrapper;

pub fn check_equal_vec<S: PartialEq>(v1: Vec<S>, v2: Vec<S>) -> bool {
    v1.len() == v2.len() && v1.iter().all(|x| v2.contains(x)) && v2.iter().all(|x| v1.contains(x))
}

// Currently only works for bank messages. We can probably extend it for all other messages.
pub fn check_equal_bnk_send_msgs(
    msg1: CosmosMsg<TerraMsgWrapper>,
    msg2: CosmosMsg<TerraMsgWrapper>,
) -> bool {
    let mut response: bool = false;

    match msg1 {
        CosmosMsg::Bank(BankMsg::Send {
                            to_address: _,
                            amount,
                        }) => {
            let msg1_amount = amount;
            match msg2 {
                CosmosMsg::Bank(BankMsg::Send {
                                    to_address: _,
                                    amount,
                                }) => {
                    response = check_equal_vec(msg1_amount, amount);
                }
                _ => {}
            }
        }
        _ => {}
    }

    response
}

#[cfg(test)]
mod tests {
    use crate::state::DecCoin;
    use crate::test_helpers::check_equal_vec;
    use cosmwasm_std::Decimal;

    #[test]
    fn test__check_equal_vec() {
        let a = vec![
            DecCoin::new(
                Decimal::from_ratio(2000_u128, 1_000_000_0000_u128),
                "anc".to_string(),
            ),
            DecCoin::new(
                Decimal::from_ratio(2000_u128, 1_000_000_0000_u128),
                "mir".to_string(),
            ),
        ];
        let b = vec![
            DecCoin::new(
                Decimal::from_ratio(2000_u128, 1_000_000_0000_u128),
                "mir".to_string(),
            ),
            DecCoin::new(
                Decimal::from_ratio(2000_u128, 1_000_000_0000_u128),
                "anc".to_string(),
            ),
        ];
        assert!(check_equal_vec(a, b));

        let a = vec![
            DecCoin::new(
                Decimal::from_ratio(1000_u128, 1_000_000_0000_u128),
                "anc".to_string(),
            ),
            DecCoin::new(
                Decimal::from_ratio(2000_u128, 1_000_000_0000_u128),
                "mir".to_string(),
            ),
        ];
        let b = vec![
            DecCoin::new(
                Decimal::from_ratio(2000_u128, 1_000_000_0000_u128),
                "mir".to_string(),
            ),
            DecCoin::new(
                Decimal::from_ratio(2000_u128, 1_000_000_0000_u128),
                "anc".to_string(),
            ),
        ];
        assert_eq!(check_equal_vec(a, b), false);
    }
}
