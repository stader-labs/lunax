use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;
use cw_storage_plus::Item;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,          // Expect update config to be called from manager.
    pub reward_denom: String,   // Reward denom is expected to be Luna
    pub staking_contract: Addr, // Expect swap and transfer to be called from pools contract
}

pub const CONFIG: Item<Config> = Item::new("config");
