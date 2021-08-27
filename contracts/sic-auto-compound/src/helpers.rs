use cosmwasm_std::{Addr, BankMsg, Coin};

// filters out all coins with 0 amount
pub fn get_bank_msg(recipient: Addr, coins_to_send: Vec<Coin>) -> Vec<BankMsg> {
    let mut bank_msgs: Vec<BankMsg> = vec![];
    for coin in coins_to_send {
        if coin.amount.is_zero() {
            continue;
        }

        bank_msgs.push(BankMsg::Send {
            to_address: String::from(recipient.clone()),
            amount: vec![coin],
        });
    }

    bank_msgs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::check_equal_vec;

    #[test]
    fn test__get_bank_msg() {
        let recipient: Addr = Addr::unchecked("abc");

        let coins_to_send = vec![];
        let banks_msgs: Vec<BankMsg> = get_bank_msg(recipient.clone(), coins_to_send);
        assert_eq!(banks_msgs, vec![]);

        let coins_to_send = vec![Coin::new(100_u128, "uluna"), Coin::new(50_u128, "uluna")];
        let banks_msgs: Vec<BankMsg> = get_bank_msg(recipient.clone(), coins_to_send);
        assert!(check_equal_vec(
            banks_msgs,
            vec![
                BankMsg::Send {
                    to_address: String::from(recipient.clone()),
                    amount: vec![Coin::new(100_u128, "uluna")]
                },
                BankMsg::Send {
                    to_address: String::from(recipient.clone()),
                    amount: vec![Coin::new(50_u128, "uluna")]
                }
            ]
        ));

        let coins_to_send = vec![Coin::new(100_u128, "uluna"), Coin::new(0_u128, "uluna")];
        let banks_msgs: Vec<BankMsg> = get_bank_msg(recipient.clone(), coins_to_send);
        assert!(check_equal_vec(
            banks_msgs,
            vec![BankMsg::Send {
                to_address: String::from(recipient.clone()),
                amount: vec![Coin::new(100_u128, "uluna")]
            },]
        ));

        let coins_to_send = vec![Coin::new(0_u128, "uluna"), Coin::new(0_u128, "uluna")];
        let banks_msgs: Vec<BankMsg> = get_bank_msg(recipient.clone(), coins_to_send);
        assert!(check_equal_vec(banks_msgs, vec![]));
    }
}
