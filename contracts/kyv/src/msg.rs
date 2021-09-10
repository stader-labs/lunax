use cosmwasm_std::{Addr, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub vault_denom: String,
    pub amount_to_stake_per_validator: Uint128,
    pub batch_size: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    RecordMetrics { timestamp: u64 }, // 12:00AM
    AddValidator { addr: Addr },
    UpdateConfig { batch_size: u32 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetAllTimestamps {},
    GetAllValidatorMetrics {
        addr: Addr,
    },
    GetValidatorMetricsBtwTimestamps {
        addr: Addr,
        timestamp1: u64,
        timestamp2: u64,
    },
    GetValidatorMetricsByTimestamp {
        timestamp: u64,
        addr: Addr,
    },
    GetValidatorsMetricsByTimestamp {
        timestamp: u64,
        from: u32,
        to: u32,
    },
    GetState {},
    GetConfig {},
    GetAllAprsByInterval {
        timestamp1: u64,
        timestamp2: u64,
        from: u32,
        to: u32,
    },
    GetAprByValidator {
        timestamp1: u64,
        timestamp2: u64,
        addr: Addr,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ValidatorAprResponse {
    pub addr: Addr,
    pub apr: Decimal,
}
