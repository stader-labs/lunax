use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
    pub vault_denom: String,
    pub pools_contract: Addr,
    pub delegator_contract: Addr,
    pub airdrop_withdraw_contract: Addr, // SCC
}

pub const VALIDATOR_REGISTRY: Map<&Addr, bool> = Map::new("validator_registry");
pub const CONFIG: Item<Config> = Item::new("config");
