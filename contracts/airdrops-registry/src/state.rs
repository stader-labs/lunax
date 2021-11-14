use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");

// Move the registered contracts to a central location.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropRegistryInfo {
    pub token: String,
    pub airdrop_contract: Addr,
    pub cw20_contract: Addr,
}
// Map of airdrop token to the token contract
pub const AIRDROP_REGISTRY: Map<String, AirdropRegistryInfo> = Map::new("airdrop_registry");