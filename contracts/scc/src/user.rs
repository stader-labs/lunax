#![allow(dead_code)]
use crate::state::{UserRewardInfo, STRATEGY_MAP};
use cosmwasm_std::{Coin, Decimal, Storage};
use cw_storage_plus::U64Key;
use stader_utils::coin_utils::{
    deccoin_vec_to_coin_vec, merge_coin_vector, merge_dec_coin_vector,
    multiply_deccoin_vector_with_decimal, CoinVecOp, DecCoin, DecCoinVecOp, Operation,
};

pub fn compute_user_airdrops_across_strategies(
    storage: &dyn Storage,
    user_reward_info: &mut UserRewardInfo,
) {
    let mut total_allocated_airdrops: Vec<Coin> = user_reward_info.pending_airdrops.clone();
    for user_strategy_info in &mut user_reward_info.strategies {
        let strategy_id = user_strategy_info.strategy_id;
        let user_shares = user_strategy_info.shares;

        let strategy_info = if let Some(strategy_info) = STRATEGY_MAP
            .may_load(storage, U64Key::new(strategy_id))
            .unwrap()
        {
            strategy_info
        } else {
            continue;
        };

        let strategy_global_airdrop_pointer = strategy_info.global_airdrop_pointer;
        let user_airdrop_pointer = &user_strategy_info.airdrop_pointer;
        let user_airdrops_for_strategy = get_user_airdrops(
            &strategy_global_airdrop_pointer,
            user_airdrop_pointer,
            user_shares,
        );

        if let Some(user_airdrops) = user_airdrops_for_strategy {
            total_allocated_airdrops = merge_coin_vector(
                &total_allocated_airdrops,
                CoinVecOp {
                    fund: user_airdrops,
                    operation: Operation::Add,
                },
            );
        }

        user_strategy_info.airdrop_pointer = strategy_global_airdrop_pointer;
    }

    user_reward_info.pending_airdrops = total_allocated_airdrops;
}

pub fn get_user_airdrops(
    global_airdrop_pointer: &[DecCoin],
    user_airdrop_pointer: &[DecCoin],
    user_shares: Decimal,
) -> Option<Vec<Coin>> {
    if global_airdrop_pointer.is_empty() {
        return None;
    }

    let airdrop_pointer_difference = merge_dec_coin_vector(
        &global_airdrop_pointer,
        DecCoinVecOp {
            fund: user_airdrop_pointer.to_vec(),
            operation: Operation::Sub,
        },
    );

    let user_airdrops =
        multiply_deccoin_vector_with_decimal(&airdrop_pointer_difference, user_shares);

    Some(deccoin_vec_to_coin_vec(&user_airdrops))
}
