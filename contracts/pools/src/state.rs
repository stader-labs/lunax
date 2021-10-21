use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal, Timestamp, Uint128, Binary};
use cw_storage_plus::{Item, Map, U64Key};
use stader_utils::coin_utils::DecCoin;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
    pub vault_denom: String, // Will be the same as reward denom in reward contract
    pub delegator_contract: Addr,
    pub unbonding_period: u64,
    pub unbonding_buffer: u64,
    pub min_deposit: Uint128,
    pub max_deposit: Uint128,
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
    pub validator_contract: Addr,
    pub reward_contract: Addr,
    pub validators: Vec<Addr>, // We estimate to have no more than 10 validators per pool.
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
pub struct ConfigUpdateRequest {
    pub(crate) delegator_contract: Option<Addr>,
    pub(crate) min_deposit: Option<Uint128>,
    pub(crate) max_deposit: Option<Uint128>,
    pub(crate) unbonding_period: Option<u64>,
    pub(crate) unbonding_buffer: Option<u64>,
}

pub const POOL_REGISTRY: Map<U64Key, PoolRegistryInfo> = Map::new("pool_registry");
// Validator contract (that actually delegates) per pool
pub const VALIDATOR_CONTRACTS: Map<&Addr, u64> = Map::new("validator_contracts");
// Reward contract (that accrues rewards) per pool
pub const REWARD_CONTRACTS: Map<&Addr, u64> = Map::new("reward_contracts");

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropRegistryInfo {
    pub airdrop_contract: Addr,
    pub cw20_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropRate {
    pub pool_id: u64,
    pub denom: String,
    pub amount: Uint128, // uAirdrop per 10^6 uBase
    pub claim_msg: Binary,
}

// Map of airdrop token to the token contract
pub const AIRDROP_REGISTRY: Map<String, AirdropRegistryInfo> = Map::new("airdrop_registry");

// (Pool_id, undelegation_batch_id) -> BatchUndelegationRecord
pub const BATCH_UNDELEGATION_REGISTRY: Map<(U64Key, U64Key), BatchUndelegationRecord> =
    Map::new("batch_undelegation_registry");
