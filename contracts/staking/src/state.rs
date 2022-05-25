use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
    pub vault_denom: String, // Will be the same as reward denom in reward contract
    pub min_deposit: Uint128,
    pub max_deposit: Uint128,
    pub active: bool,

    pub reward_contract: Addr,             // Non-changeable
    pub cw20_token_contract: Addr,         // Changeable once
    pub airdrop_registry_contract: Addr,   // Non-changeable
    pub airdrop_withdrawal_contract: Addr, // Non-changeable

    pub protocol_fee_contract: Addr, // Non-changeable
    pub protocol_reward_fee: Decimal,
    pub protocol_deposit_fee: Decimal,
    pub protocol_withdraw_fee: Decimal,

    pub unbonding_period: u64,
    pub undelegation_cooldown: u64,
    pub reinvest_cooldown: u64, // cooldown to avoid external users from spamming the reinvest message
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub total_staked: Uint128,
    pub exchange_rate: Decimal, // shares to token value. 1 share = (ExchangeRate) tokens.
    pub last_reconciled_batch_id: u64,
    pub current_undelegation_batch_id: u64,
    pub last_undelegation_time: Timestamp,
    pub last_reinvest_time: Timestamp,
    pub validators: Vec<Addr>,
    pub reconciled_funds_to_withdraw: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OperationControls {
    pub deposit_paused: bool,
    pub queue_undelegate_paused: bool,
    pub undelegate_paused: bool,
    pub withdraw_paused: bool,
    pub reinvest_paused: bool,
    pub reconcile_paused: bool,
    pub claim_airdrops_paused: bool,
    pub redeem_rewards_paused: bool,
    pub reimburse_slashing_paused: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VMeta {
    pub staked: Uint128, // Staked so far. This is the net sum and does not count filled funds.
    pub slashed: Uint128, // Slashed by this validator.
    pub filled: Uint128, // Filled with validator slashing insurance
}

impl Default for VMeta {
    fn default() -> Self {
        VMeta {
            staked: Uint128::zero(),
            slashed: Uint128::zero(),
            filled: Uint128::zero(),
        }
    }
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
pub const BATCH_UNDELEGATION_REGISTRY: Map<u64, BatchUndelegationRecord> =
    Map::new("batch_undelegation_registry");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigUpdateRequest {
    pub(crate) min_deposit: Option<Uint128>,
    pub(crate) max_deposit: Option<Uint128>,

    pub(crate) cw20_token_contract: Option<String>, // Only upgradeable once.
    pub(crate) protocol_reward_fee: Option<Decimal>,
    pub(crate) protocol_withdraw_fee: Option<Decimal>,
    pub(crate) protocol_deposit_fee: Option<Decimal>,
    pub(crate) airdrop_registry_contract: Option<String>,

    pub(crate) unbonding_period: Option<u64>,
    pub(crate) undelegation_cooldown: Option<u64>,
    pub(crate) reinvest_cooldown: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OperationControlsUpdateRequest {
    pub(crate) deposit_paused: Option<bool>,
    pub(crate) queue_undelegate_paused: Option<bool>,
    pub(crate) undelegate_paused: Option<bool>,
    pub(crate) withdraw_paused: Option<bool>,
    pub(crate) reinvest_paused: Option<bool>,
    pub(crate) reconcile_paused: Option<bool>,
    pub(crate) claim_airdrops_paused: Option<bool>,
    pub(crate) redeem_rewards_paused: Option<bool>,
    pub(crate) reimburse_slashing_paused: Option<bool>,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");
pub const OPERATION_CONTROLS: Item<OperationControls> = Item::new("operation_controls");

// (User_Address, Undelegation Batch)
pub const USERS: Map<(&Addr, u64), UndelegationInfo> = Map::new("users");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropRate {
    pub denom: String,
    pub amount: Uint128, // uAirdrop per 10^6 uBase
    pub stage: u8,
    pub proof: Vec<String>,
}

// this is a tmp store to store the intermediate values of manager updates.
// manager updates are 2 phase, we set it and then accept it. This is done to
// add a greater assurance of the update.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TmpManagerStore {
    pub manager: String,
}

pub const TMP_MANAGER_STORE: Item<TmpManagerStore> = Item::new("tmp_manager_store");
