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

// this is a tmp store to store the intermediate values of manager updates.
// manager updates are 2 phase, we set it and then accept it. This is done to
// add a greater assurance of the update.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TmpManagerStore {
    pub manager: String,
}

pub const TMP_MANAGER_STORE: Item<TmpManagerStore> = Item::new("tmp_manager_store");
