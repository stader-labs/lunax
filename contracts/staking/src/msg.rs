use crate::state::{
    AirdropRate, BatchUndelegationRecord, Config, ConfigUpdateRequest,
    OperationControlsUpdateRequest, State, TmpManagerStore, VMeta,
};
use cosmwasm_std::{Addr, Coin, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub min_deposit: Uint128,
    pub max_deposit: Uint128,

    pub reward_contract: String,
    pub airdrops_registry_contract: String,
    pub airdrop_withdrawal_contract: String,

    pub protocol_fee_contract: String,
    pub protocol_reward_fee: Decimal, // "1 is 100%, 0.02 is 2%"
    pub protocol_deposit_fee: Decimal,
    pub protocol_withdraw_fee: Decimal, // "1 is 100%, 0.02 is 2%"

    pub unbonding_period: u64,
    pub undelegation_cooldown: u64,
    pub reinvest_cooldown: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserQueryInfo {
    pub total_tokens: Uint128,
    pub total_amount: Coin, // value of tokens in luna with the exchange rate at that point
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    QueueUndelegate {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    AddValidator {
        val_addr: Addr,
    },
    RemoveValidator {
        val_addr: Addr,
        redel_addr: Addr,
    },
    RebalancePool {
        amount: Uint128,
        val_addr: Addr,
        redel_addr: Addr,
    },
    Deposit {},
    RedeemRewards {
        validators: Option<Vec<Addr>>,
    },
    Receive(Cw20ReceiveMsg),
    Reinvest {},
    Undelegate {},
    ReconcileFunds {},
    WithdrawFundsToWallet {
        batch_id: u64,
    },
    ClaimAirdrops {
        rates: Vec<AirdropRate>,
    },
    UpdateConfig {
        config_request: ConfigUpdateRequest,
    },
    UpdateOperationFlags {
        operation_controls_update_request: OperationControlsUpdateRequest,
    },
    SetManager {
        manager: String,
    },
    AcceptManager {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MerkleAirdropMsg {
    Claim {
        stage: u8,
        amount: Uint128,
        proof: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
    OperationControls {},
    TmpManagerStore {},
    BatchUndelegation {
        batch_id: u64,
    },
    GetUserUndelegationRecords {
        user_addr: String,
        start_after: Option<u64>,
        limit: Option<u64>,
    }, // return shares & undelegation list.
    GetUserUndelegationInfo {
        user_addr: String,
        batch_id: u64,
    },
    GetValMeta {
        val_addr: Addr,
    },
    GetUserInfo {
        user_addr: String,
    },
    ComputeDepositBreakdown {
        amount: Uint128,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryConfigResponse {
    pub config: Config,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TmpManagerStoreResponse {
    pub tmp_manager_store: Option<TmpManagerStore>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserInfoResponse {
    pub user_info: UserQueryInfo,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryStateResponse {
    pub state: State,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryBatchUndelegationResponse {
    pub batch: Option<BatchUndelegationRecord>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetValMetaResponse {
    pub val_meta: Option<VMeta>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetFundsDepositRecord {
    pub user_deposit_amount: Uint128,
    pub protocol_fee: Uint128,
    pub staked_amount: Uint128,
    pub tokens_to_mint: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetFundsClaimRecord {
    pub user_withdrawal_amount: Uint128,
    pub protocol_fee: Uint128,
    pub undelegated_tokens: Uint128,
}
