use crate::state::{Config, TmpManagerStore};
use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub staking_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Transfer {
        reward_amount: Uint128,
        reward_withdraw_contract: Addr,
        protocol_fee_amount: Uint128,
        protocol_fee_contract: Addr,
    }, // Transfer swapped rewards to SCC.
    UpdateConfig {
        staking_contract: Option<String>,
    },
    SetManager {
        manager: String,
    },
    AcceptManager {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    TmpManagerStore {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetConfigResponse {
    pub config: Config,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TmpManagerStoreResponse {
    pub tmp_manager_store: Option<TmpManagerStore>,
}
