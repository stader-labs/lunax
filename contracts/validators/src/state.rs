use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Decimal, Uint128};
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub manager: Addr,
    pub vault_denom: String,
    pub pools_contract_addr: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VMeta {
    pub staked: Uint128,
    pub reward_pointer: Decimal,
    pub accrued_rewards: Vec<Coin>,
}

pub const VALIDATOR_META: Map<&Addr, VMeta> = Map::new("validator_meta");

pub const STATE: Item<State> = Item::new("state");
