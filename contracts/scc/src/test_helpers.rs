// test_helpers specific to scc

use crate::state::{UserRewardInfo, UserStrategyInfo};
use stader_utils::coin_utils::DecCoin;
use stader_utils::test_helpers::check_equal_vec;

pub fn check_equal_user_strategies(a: Vec<UserStrategyInfo>, b: Vec<UserStrategyInfo>) -> bool {
    a.len() == b.len()
        && a.iter().all(|x| {
            b.iter().any(|y| {
                y.shares.eq(&x.shares)
                    && y.strategy_name.eq(&x.strategy_name)
                    && check_equal_vec(y.airdrop_pointer.clone(), x.airdrop_pointer.clone())
            })
        })
        && b.iter().all(|x| {
            a.iter().any(|y| {
                y.shares.eq(&x.shares)
                    && y.strategy_name.eq(&x.strategy_name)
                    && check_equal_vec(y.airdrop_pointer.clone(), x.airdrop_pointer.clone())
            })
        })
}

pub fn check_equal_reward_info(a: UserRewardInfo, b: UserRewardInfo) -> bool {
    if !check_equal_user_strategies(a.strategies.clone(), b.strategies.clone()) {
        return false;
    }

    if !check_equal_vec(a.pending_airdrops, b.pending_airdrops) {
        return false;
    }

    return true;
}
