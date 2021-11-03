use crate::state::{
    AirdropRate, AirdropRegistryInfo, BatchUndelegationRecord, Config, ConfigUpdateRequest,
    PoolConfigUpdateRequest, PoolRegistryInfo, State, VMeta,
};
use cosmwasm_std::{Addr, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub delegator_contract: String,
    pub scc_contract: String,
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
        validator_contract: String,
        reward_contract: String,
        protocol_fee_percent: Decimal,
        protocol_fee_contract: String,
    },
    AddValidator {
        val_addr: Addr,
        pool_id: u64,
    },
    RemoveValidator {
        val_addr: Addr,
        redel_addr: Addr,
        pool_id: u64,
    },
    RebalancePool {
        pool_id: u64,
        amount: Uint128,
        val_addr: Addr,
        redel_addr: Addr,
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
    },
    UpdateAirdropRegistry {
        airdrop_token: String,
        airdrop_contract: String,
        cw20_contract: String,
    },
    ClaimAirdrops {
        rates: Vec<AirdropRate>,
    },
    UpdateConfig {
        config_request: ConfigUpdateRequest,
    },
    UpdatePoolMetadata {
        pool_id: u64,
        pool_config_update_request: PoolConfigUpdateRequest,
    },
    SimulateSlashing {
        pool_id: u64,
        val_addr: Addr,
        amount: Uint128,
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
    GetUserComputedInfo { pool_id: u64, user_addr: Addr },
    GetValMeta { pool_id: u64, val_addr: Addr },
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetValMetaResponse {
    pub val_meta: Option<VMeta>,
}
