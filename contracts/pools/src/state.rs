use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Binary, Decimal, Timestamp, Uint128};
use cw_storage_plus::{Item, Map, U64Key};
use stader_utils::coin_utils::DecCoin;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
    pub vault_denom: String, // Will be the same as reward denom in reward contract
    pub delegator_contract: Addr,
    pub scc_contract: Addr, // Contract to send generated rewards so as to be put into strategies. Usually assigned the SCC contract.
    pub unbonding_period: u64,
    pub unbonding_buffer: u64,
    pub min_deposit: Uint128,
    pub max_deposit: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub next_pool_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VMeta {
    pub staked: Uint128, // Staked so far. This is the net sum and does not count filled funds.
    pub slashed: Uint128, // Slashed by this validator.
    pub filled: Uint128, // Filled with validator slashing insurance
}

// Get pool_id to traits & val_addr to moniker mapping from offchain APIs
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolRegistryInfo {
    pub name: String,
    pub active: bool,                  // activates by default.
    pub validator_contract: Addr,      // Actual contract that delegates on behalf of this pool
    pub reward_contract: Addr, // Contract to send redeemed rewards to & later move swapped rewards from.
    pub protocol_fee_contract: Addr, // Contract to send protocol fee funds from generated rewards. Usually assigned as the treasury contract.
    pub protocol_fee_percent: Decimal, // Decimal - "0.01" is 1%
    pub validators: Vec<Addr>,       // We estimate to have no more than 10 validators per pool.
    pub staked: Uint128,
    pub rewards_pointer: Decimal,
    pub airdrops_pointer: Vec<DecCoin>,
    pub slashing_pointer: Decimal, // Value starts at 1 and keeps going down with slashing events and never up.
    pub current_undelegation_batch_id: u64,
    pub last_reconciled_batch_id: u64,
}

// Validator address and pool Id as key.
pub const VALIDATOR_META: Map<(Addr, U64Key), VMeta> = Map::new("validator_meta");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolConfigUpdateRequest {
    pub(crate) active: Option<bool>,
    pub(crate) reward_contract: Option<String>,
    pub(crate) protocol_fee_contract: Option<String>,
    pub(crate) protocol_fee_percent: Option<Decimal>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BatchUndelegationRecord {
    // Every time slashing is noticed, we pro-rate the "undelegated amount" that's been requested by delegators and hasn't been actually undelegated
    pub(crate) prorated_amount: Decimal,
    // At the time of undelegation, we convert prorated_amount into a Uint. Saving the Uint128 conversion to the last minute is so we don't run into precision issues.
    pub(crate) undelegated_amount: Uint128,
    pub(crate) create_time: Timestamp,
    pub(crate) est_release_time: Option<Timestamp>,
    pub(crate) reconciled: bool,
    pub(crate) last_updated_slashing_pointer: Decimal, // pool pointer from every time user action is processed or "most" managerial messages are executed.
    pub(crate) unbonding_slashing_ratio: Decimal, // If Unbonding happens during the 21 day period.
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigUpdateRequest {
    pub(crate) delegator_contract: Option<Addr>,
    pub(crate) scc_contract: Option<Addr>,
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
