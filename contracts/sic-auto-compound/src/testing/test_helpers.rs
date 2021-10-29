#[allow(dead_code)]
use cosmwasm_std::{BankMsg, CosmosMsg};

pub fn check_equal_vec<S: PartialEq>(v1: Vec<S>, v2: Vec<S>) -> bool {
    v1.len() == v2.len() && v1.iter().all(|x| v2.contains(x)) && v2.iter().all(|x| v1.contains(x))
}

// Currently only works for bank messages. We can probably extend it for all other messages.
pub fn check_equal_bnk_send_msgs(msg1: CosmosMsg, msg2: CosmosMsg) -> bool {
    let mut response: bool = false;

    if let CosmosMsg::Bank(BankMsg::Send { to_address, amount }) = msg1 {
        let msg1_amount = amount;
        let msg1_to_address = to_address;
        if let CosmosMsg::Bank(BankMsg::Send { to_address, amount }) = msg2 {
            response = check_equal_vec(msg1_amount, amount) && (msg1_to_address == to_address);
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use crate::testing::test_helpers::{check_equal_bnk_send_msgs, check_equal_vec};
    use cosmwasm_std::{BankMsg, Coin, Decimal};
    use stader_utils::coin_utils::DecCoin;

    #[test]
    fn test_check_equal_bank_msg() {
        let msg1 = BankMsg::Send {
            to_address: "user1".to_string(),
            amount: vec![
                Coin::new(100_u128, "abc".to_string()),
                Coin::new(200_u128, "def".to_string()),
            ],
        };
        let msg2 = BankMsg::Send {
            to_address: "user1".to_string(),
            amount: vec![
                Coin::new(200_u128, "def".to_string()),
                Coin::new(100_u128, "abc".to_string()),
            ],
        };

        assert!(check_equal_bnk_send_msgs(msg1.into(), msg2.into()));
    }

    #[test]
    fn test_check_equal_vec() {
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
