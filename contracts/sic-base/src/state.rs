use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Timestamp, Uint128};
use cw_storage_plus::Item;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub manager: Addr,
    pub scc_address: Addr,

    pub strategy_denom: String,

    pub contract_genesis_block_height: u64,
    pub contract_genesis_timestamp: Timestamp,

    pub total_rewards_accumulated: Uint128,
}

pub const STATE: Item<State> = Item::new("state");
