use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Timestamp, Uint128};
use cw_storage_plus::Item;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub manager: Addr,
    pub scc_address: Addr,
    pub manager_seed_funds: Uint128,
    pub min_validator_pool_size: u64,

    pub strategy_denom: String,

    pub contract_genesis_block_height: u64,
    pub contract_genesis_timestamp: Timestamp,

    pub validator_pool: Vec<Addr>,
    pub unswapped_rewards: Vec<Coin>,
    pub uninvested_rewards: Coin,
}

pub const STATE: Item<State> = Item::new("state");
