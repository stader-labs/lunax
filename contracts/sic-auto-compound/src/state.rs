use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Decimal, Timestamp, Uint128};
use cw_storage_plus::{Item, Map, U64Key};
use std::fmt;
use std::fmt::Display;

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, JsonSchema)]
pub struct DecCoin {
    pub(crate) amount: Decimal,
    pub(crate) denom: String,
}

impl DecCoin {
    pub fn new<S: Into<String>>(amount: Decimal, denom: S) -> Self {
        DecCoin {
            amount,
            denom: denom.into(),
        }
    }
}

impl Display for DecCoin {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // We use the formatting without a space between amount and denom,
        // which is common in the Cosmos SDK ecosystem:
        // https://github.com/cosmos/cosmos-sdk/blob/v0.42.4/types/coin.go#L643-L645
        // For communication to end users, Coin needs to transformed anways (e.g. convert integer uatom to decimal ATOM).
        write!(f, "{}{}", self.amount, self.denom)
    }
}

// Store the delegation related info specific to a validator
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StakeQuota {
    pub(crate) amount: Coin,
    // Ratio of coin staked with this validator to the total coin staked through vault.
    pub(crate) vault_stake_fraction: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
    pub scc_contract_address: Addr,

    pub vault_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub contract_genesis_block_height: u64,
    pub contract_genesis_timestamp: Timestamp,

    pub unbonding_period: u64, // the blockchain's unbonding_period + buffer_time

    pub current_undelegation_batch_id: u64,

    pub accumulated_vault_airdrops: Vec<Coin>,
    // pub global_airdrop_pointer: Vec<DecCoin>,
    pub validator_pool: Vec<Addr>,
    pub unswapped_rewards: Vec<Coin>,
    pub uninvested_rewards: Coin,
    pub total_staked_tokens: Uint128,
    pub total_slashed_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BatchUndelegationRecord {
    pub(crate) amount: Coin,
    pub(crate) unbonding_slashing_ratio: Decimal,
    pub(crate) create_time: Timestamp,
    pub(crate) est_release_time: Timestamp,
    pub(crate) slashing_checked: bool,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");

// TODO: bchain99 - review if we need this
// Map of the validators we staked with. this is to give O(1) lookups to check if we staked in a validator
pub const VALIDATORS_TO_STAKED_QUOTA: Map<&Addr, StakeQuota> =
    Map::new("validator_to_staked_quota");

// Map of airdrop token to the token contract
pub const AIRDROP_REGISTRY: Map<String, Addr> = Map::new("airdrop_registry");

// // Map of undelegation order per undelegation epoch loop
pub const UNDELEGATION_INFO_LEDGER: Map<U64Key, BatchUndelegationRecord> =
    Map::new("undelegation_info_ledger");
