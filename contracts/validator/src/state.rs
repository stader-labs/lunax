use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Uint128};
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
    pub vault_denom: String,
    pub pools_contract: Addr,
    pub delegator_contract: Addr,
    pub airdrop_withdraw_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub slashing_funds: Uint128, // Although can be changed by manager, state is a better fit
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VMeta {
    pub staked: Uint128,
}

pub const VALIDATOR_REGISTRY: Map<&Addr, VMeta> = Map::new("validator_registry");

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");

