use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal, Timestamp, Uint128};
use cw_storage_plus::{Item, Map, U64Key};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
    pub vault_denom: String, // Will be the same as reward denom in reward contract
    pub min_deposit: Uint128,
    pub max_deposit: Uint128,
    pub active: bool,

    pub reward_contract: Addr,
    pub cw20_token_contract: Addr,
    pub airdrop_registry_contract: Addr,
    pub airdrop_withdrawal_contract: Addr,

    pub protocol_fee_contract: Addr,
    pub protocol_reward_fee: Decimal,
    pub protocol_deposit_fee: Decimal,
    pub protocol_withdraw_fee: Decimal,

    pub unbonding_period: u64,
    pub undelegation_cooldown: u64,
    pub swap_cooldown: u64, // cooldown to avoid external users from spamming the swap message
    pub reinvest_cooldown: u64, // cooldown to avoid external users from spamming the reinvest message
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub total_staked: Uint128,
    pub exchange_rate: Decimal, // shares to token value. 1 share = (ExchangeRate) tokens.
    pub last_reconciled_batch_id: u64,
    pub current_undelegation_batch_id: u64,
    pub last_undelegation_time: Timestamp,
    pub last_swap_time: Timestamp,
    pub last_reinvest_time: Timestamp,
    pub validators: Vec<Addr>,
    pub reconciled_funds_to_withdraw: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VMeta {
    pub staked: Uint128, // Staked so far. This is the net sum and does not count filled funds.
    pub slashed: Uint128, // Slashed by this validator.
    pub filled: Uint128, // Filled with validator slashing insurance
}

impl VMeta {
    pub fn new() -> Self {
        VMeta {
            staked: Uint128::zero(),
            slashed: Uint128::zero(),
            filled: Uint128::zero(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UndelegationInfo {
    pub batch_id: u64,
    pub token_amount: Uint128, // Shares undelegated
}

// Validator address and pool Id as key.
pub const VALIDATOR_META: Map<&Addr, VMeta> = Map::new("validator_meta");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BatchUndelegationRecord {
    pub(crate) undelegated_tokens: Uint128,
    pub(crate) create_time: Timestamp,
    pub(crate) est_release_time: Option<Timestamp>,
    pub(crate) reconciled: bool,
    pub(crate) undelegation_er: Decimal,
    pub(crate) undelegated_stake: Uint128,
    pub(crate) unbonding_slashing_ratio: Decimal, // If Unbonding slashing happens during the 21 day period.
}

// (undelegation_batch_id) -> BatchUndelegationRecord
pub const BATCH_UNDELEGATION_REGISTRY: Map<U64Key, BatchUndelegationRecord> =
    Map::new("batch_undelegation_registry");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigUpdateRequest {
    pub(crate) active: Option<bool>,
    pub(crate) min_deposit: Option<Uint128>,
    pub(crate) max_deposit: Option<Uint128>,

    pub(crate) cw20_token_contract: Option<String>, // Only upgradeable once.
    pub(crate) protocol_fee_contract: Option<String>,
    pub(crate) protocol_reward_fee: Option<Decimal>,
    pub(crate) protocol_withdraw_fee: Option<Decimal>,
    pub(crate) protocol_deposit_fee: Option<Decimal>,
    pub(crate) airdrop_withdrawal_contract: Option<String>,
    pub(crate) airdrop_registry_contract: Option<String>,

    pub(crate) unbonding_period: Option<u64>,
    pub(crate) undelegation_cooldown: Option<u64>,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");

// (User_Address, Undelegation Batch)
pub const USERS: Map<(&Addr, U64Key), UndelegationInfo> = Map::new("users");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropRate {
    pub denom: String,
    pub amount: Uint128, // uAirdrop per 10^6 uBase
    pub stage: u8,
    pub proof: Vec<String>,
}
