use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Uint128, Decimal, Timestamp};
use cw_storage_plus::{Item, Map, U64Key};
use stader_utils::coin_utils::DecCoin;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
    pub vault_denom: String,
    pub validator_contract: Addr,
    pub delegator_contract: Addr,
    pub unbonding_period: u64,
    pub unbonding_buffer: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub next_pool_id: u64,
}

// Get pool_id to traits & val_addr to moniker mapping from offchain APIs
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolRegistryInfo {
    pub name: String,
    pub active: bool, // activates by default.
    pub validators: Vec<Addr>,
    pub staked: Uint128,
    pub rewards_pointer: Decimal,
    pub airdrops_pointer: Vec<DecCoin>,
    pub current_undelegation_batch_id: u64,
    pub last_reconciled_batch_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BatchUndelegationRecord {
    pub(crate) amount: Uint128,
    pub(crate) create_time: Timestamp,
    pub(crate) est_release_time: Option<Timestamp>,
    pub(crate) withdrawable_time: Option<Timestamp>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ValInfo {
    pub pool_id: u64,
    pub staked: Uint128,
}

// #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
// pub struct RedelMeta {
//     pub pool_id: u64,
//     pub amount: String,
//     pub staked: Uint128,
// }

pub const POOL_REGISTRY: Map<U64Key, PoolRegistryInfo> = Map::new("pool_registry");
pub const VALIDATOR_REGISTRY: Map<&Addr, ValInfo> = Map::new("validator_registry");

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropRegistryInfo {
    pub airdrop_contract: Addr,
    pub token_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropRate {
    pub pool_id: u64,
    pub denom: String,
    pub amount: Uint128, // uAirdrop per 10^6 uBase
}

// Map of airdrop token to the token contract
pub const AIRDROP_REGISTRY: Map<String, AirdropRegistryInfo> = Map::new("airdrop_registry");

// (Pool_id, undelegation_batch_id) -> BatchUndelegationRecord
pub const BATCH_UNDELEGATION_REGISTRY: Map<(U64Key, U64Key), BatchUndelegationRecord> = Map::new("batch_undelegation_registry");
