use crate::state::{
    AirdropRate, AirdropRegistryInfo, BatchUndelegationRecord, Config, ConfigUpdateRequest,
    PoolRegistryInfo, State,
};
use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub vault_denom: String,
    pub delegator_contract: Addr,
    pub unbonding_period: Option<u64>,
    pub unbonding_buffer: Option<u64>,
    pub min_deposit: Uint128,
    pub max_deposit: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    AddPool {
        name: String,
        validator_contract: Addr,
        reward_contract: Addr,
    },
    AddValidator {
        val_addr: Addr,
        pool_id: u64,
    },
    RemoveValidator {
        val_addr: Addr,
        redel_addr: Addr,
        pool_id: u64
    },
    Deposit {
        pool_id: u64,
    },
    RedeemRewards {
        pool_id: u64,
    },
    Swap {
        pool_id: u64,
    },
    SendRewardsToScc {
        pool_id: u64,
    },
    QueueUndelegate {
        pool_id: u64,
        amount: Uint128,
    },
    Undelegate {
        pool_id: u64,
    },
    ReconcileFunds {
        pool_id: u64,
    },
    WithdrawFundsToWallet {
        pool_id: u64,
        batch_id: u64,
        undelegate_id: u64,
        amount: Uint128,
    },
    UpdateAirdropRegistry {
        airdrop_token: String,
        airdrop_contract: Addr,
        cw20_contract: Addr,
    },
    ClaimAirdrops {
        rates: Vec<AirdropRate>,
    },
    UpdateConfig {
        config_request: ConfigUpdateRequest,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
    Pool { pool_id: u64 },
    // ValidatorInPool { val_addr: Addr, pool_id: u64 },
    BatchUndelegation { pool_id: u64, batch_id: u64 },
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
pub struct QueryPoolResponse {
    pub pool: Option<PoolRegistryInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryBatchUndelegationResponse {
    pub batch: Option<BatchUndelegationRecord>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetAirdropMetaResponse {
    pub airdrop_meta: Option<AirdropRegistryInfo>,
}
