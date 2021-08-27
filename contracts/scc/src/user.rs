use crate::state::DecCoin;
use crate::utils::{
    check_equal_deccoin_vector, deccoin_vec_to_coin_vec, merge_dec_coin_vector,
    multiply_deccoin_vector_with_decimal, DecCoinVecOp, Operation,
};
use cosmwasm_std::{Addr, Coin, Decimal, Env, Storage};

pub fn get_user_airdrops(
    global_airdrop_pointer: Vec<DecCoin>,
    user_airdrop_pointer: Vec<DecCoin>,
    user_shares: Decimal,
) -> Option<Vec<Coin>> {
    if global_airdrop_pointer.is_empty() || user_airdrop_pointer.is_empty() {
        return None;
    }

    if check_equal_deccoin_vector(&global_airdrop_pointer, &user_airdrop_pointer) {
        return None;
    }

    let airdrop_pointer_difference = merge_dec_coin_vector(
        &global_airdrop_pointer,
        DecCoinVecOp {
            fund: user_airdrop_pointer,
            operation: Operation::Sub,
        },
    );

    let user_airdrops =
        multiply_deccoin_vector_with_decimal(&airdrop_pointer_difference, user_shares);

    return Some(deccoin_vec_to_coin_vec(&user_airdrops));
}
