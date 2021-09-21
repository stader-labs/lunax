use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub manager: Addr,
}

pub const STATE: Item<State> = Item::new("state");

pub const CONTRACTS: Map<String, Addr> = Map::new("contracts");
