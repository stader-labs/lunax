use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Decimal, Timestamp, Uint128};
use cw_storage_plus::{Item, Map, U64Key};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub manager: Addr,
    pub scc_address: Addr,

    pub strategy_denom: String,

    pub contract_genesis_block_height: u64,
    pub contract_genesis_timestamp: Timestamp,

    pub current_undelegation_batch_id: u64,

    pub unbonding_period: u64, // the blockchain's unbonding_period + buffer_time
    pub total_rewards_accumulated: Uint128,
    pub accumulated_airdrops: Vec<Coin>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BatchUndelegationRecord {
    pub(crate) amount: Coin,
    pub(crate) unbonding_slashing_ratio: Decimal,
    pub(crate) create_time: Timestamp,
    pub(crate) est_release_time: Timestamp,
    pub(crate) slashing_checked: bool,
}

pub const STATE: Item<State> = Item::new("state");

// Map of undelegation order per undelegation epoch loop
pub const UNDELEGATION_INFO_LEDGER: Map<U64Key, BatchUndelegationRecord> =
    Map::new("undelegation_info_ledger");
