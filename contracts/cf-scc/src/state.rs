use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Uint128};
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
    pub delegator_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserInfo {
    pub amount: Uint128,
    pub airdrops: Vec<Coin>,
}

impl UserInfo {
    pub fn new() -> Self {
        UserInfo {
            amount: Uint128::zero(),
            airdrops: vec![],
        }
    }
}

pub const CONFIG: Item<Config> = Item::new("config");

pub const USER_REWARDS: Map<&Addr, UserInfo> = Map::new("user_rewards");
