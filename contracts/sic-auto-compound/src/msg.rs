use crate::state::{BatchUndelegationRecord, State};
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::U64Key;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub scc_address: Addr,
    // denomination of the staking coin
    pub vault_denom: String,
    // initial set of validators who make up the validator pool
    pub initial_validators: Vec<Addr>,
    // unbonding period in seconds (defaults to 21 days + 3600s)
    pub unbonding_period: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    TransferRewards {},
    UndelegateRewards {
        amount: Uint128,
    },
    WithdrawRewards {
        user: Addr,
        amount: Uint128,
        undelegation_batch_id: u64,
    },
    ReconcileUndelegationBatch {
        undelegation_batch_id: u64,
    },
    Swap {},
    Reinvest {},
    RedeemRewards {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetTotalTokens {},
    GetCurrentUndelegationBatchId {},
    GetUndelegationBatchInfo { undelegation_batch_id: u64 },
    GetState {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetStateResponse {
    pub state: Option<State>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetTotalTokensResponse {
    pub total_tokens: Option<Uint128>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetCurrentUndelegationBatchIdResponse {
    pub current_undelegation_batch_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetUndelegationBatchInfoResponse {
    pub undelegation_batch_info: Option<BatchUndelegationRecord>,
}
