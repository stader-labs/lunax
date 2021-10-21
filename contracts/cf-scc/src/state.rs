use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
    pub delegator_contract: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");

pub const USER_REWARDS: Map<&Addr, Uint128> = Map::new("user_rewards");
