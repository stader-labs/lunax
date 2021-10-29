#![allow(dead_code)]

// test_helpers specific to scc
use crate::msg::UserStrategyQueryInfo;
use crate::state::{UserRewardInfo, UserStrategyInfo};
use cosmwasm_std::{BankMsg, CosmosMsg};

pub fn check_equal_user_strategies(a: Vec<UserStrategyInfo>, b: Vec<UserStrategyInfo>) -> bool {
    a.len() == b.len()
        && a.iter().all(|x| {
            b.iter().any(|y| {
                y.shares.eq(&x.shares)
                    && y.strategy_id.eq(&x.strategy_id)
                    && check_equal_vec(y.airdrop_pointer.clone(), x.airdrop_pointer.clone())
            })
        })
        && b.iter().all(|x| {
            a.iter().any(|y| {
                y.shares.eq(&x.shares)
                    && y.strategy_id.eq(&x.strategy_id)
                    && check_equal_vec(y.airdrop_pointer.clone(), x.airdrop_pointer.clone())
            })
        })
}

pub fn check_equal_reward_info(a: UserRewardInfo, b: UserRewardInfo) -> bool {
    if !check_equal_user_strategies(a.strategies.clone(), b.strategies.clone()) {
        return false;
    }

    if !check_equal_vec(a.user_portfolio, b.user_portfolio) {
        return false;
    }

    if !check_equal_vec(a.pending_airdrops, b.pending_airdrops) {
        return false;
    }

    if !check_equal_vec(a.undelegation_records, b.undelegation_records) {
        return false;
    }

    if a.pending_rewards.ne(&b.pending_rewards) {
        return false;
    }

    true
}

pub fn check_equal_user_strategy_query_info(
    a: Vec<UserStrategyQueryInfo>,
    b: Vec<UserStrategyQueryInfo>,
) -> bool {
    a.len() == b.len()
        && a.iter().all(|x| {
            b.iter().any(|y| {
                y.strategy_id.eq(&x.strategy_id)
                    && y.strategy_name.eq(&x.strategy_name)
                    && y.total_rewards.eq(&x.total_rewards)
                    && check_equal_vec(y.total_airdrops.clone(), x.total_airdrops.clone())
            })
        })
        && b.iter().all(|x| {
            a.iter().any(|y| {
                y.strategy_id.eq(&x.strategy_id)
                    && y.strategy_name.eq(&x.strategy_name)
                    && y.total_rewards.eq(&x.total_rewards)
                    && check_equal_vec(y.total_airdrops.clone(), x.total_airdrops.clone())
            })
        })
}

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
