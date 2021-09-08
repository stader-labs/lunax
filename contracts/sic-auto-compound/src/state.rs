use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Decimal, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};

// Store the delegation related info specific to a validator
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StakeQuota {
    pub amount: Coin,
    // Ratio of coin staked with this validator to the total coin staked through vault.
    pub vault_stake_fraction: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub manager: Addr,
    pub scc_address: Addr,

    // TODO: bchain99 - change this to strategy_denom
    pub vault_denom: String,

    pub contract_genesis_block_height: u64,
    pub contract_genesis_timestamp: Timestamp,

    pub validator_pool: Vec<Addr>,
    pub unswapped_rewards: Vec<Coin>,
    pub uninvested_rewards: Coin,
    pub total_staked_tokens: Uint128,
    // total_slashed_amount = total_stake_slashed + total_undelegations_slashed. This field is mainly for metric tracking
    pub total_slashed_amount: Uint128,
}

pub const STATE: Item<State> = Item::new("state");

// TODO: bchain99 - review if we need this
// Map of the validators we staked with. this is to give O(1) lookups to check if we staked in a validator
pub const VALIDATORS_TO_STAKED_QUOTA: Map<&Addr, StakeQuota> =
    Map::new("validator_to_staked_quota");
