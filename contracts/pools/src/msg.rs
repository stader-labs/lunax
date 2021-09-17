use crate::state::{AirdropRegistryInfo, Config, State, AirdropRate};
use cosmwasm_std::{Addr, Binary, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub vault_denom: String,
    pub validator_contract: Addr,
    pub delegator_contract: Addr,
    pub unbonding_period: Option<u64>,
    pub unbonding_buffer: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    AddPool { name: String },
    AddValidator { val_addr: Addr, pool_id: u64 },
    RemoveValidator { val_addr: Addr },
    Deposit { pool_id: u64 },
    RedeemRewards { pool_id: u64 },
    Swap { pool_id: u64 },
    QueueUndelegate { pool_id: u64, amount: Uint128 },
    Undelegate { pool_id: u64 },
    ReconcileFunds { pool_id: u64 },
    WithdrawFundsToWallet { pool_id: u64, batch_id: u64, undelegate_id: u64, amount: Uint128 },
    UpdateAirdropPointers { airdrop_amount: Uint128, rates: Vec<AirdropRate> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryConfigResponse {
    pub config: Config,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryStateResponse {
    pub state: State,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetAirdropMetaResponse {
    pub airdrop_meta: Option<AirdropRegistryInfo>,
}
