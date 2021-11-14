use crate::state::{
    AirdropRate, AirdropTransferRequest, BatchUndelegationRecord, Config, ConfigUpdateRequest,
    State, VMeta,
};
use cosmwasm_std::{Addr, Decimal, Uint128};
use cw20::{Cw20Coin, Cw20ReceiveMsg};
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
    RedeemRewards {},
    Swap {},
    QueueUndelegate {
        cw20_msg: Cw20ReceiveMsg,
    },
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
    BatchUndelegation {
        batch_id: u64,
    },
    GetUserComputedInfo {
        user_addr: String,
        start_after: Option<u64>,
        limit: Option<u64>,
    }, // return shares & undelegation list.
    GetUserUndelegationRecord {
        user_addr: Addr,
        batch_id: u64,
    },
    GetValMeta {
        val_addr: Addr,
    },
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
pub struct QueryBatchUndelegationResponse {
    pub batch: Option<BatchUndelegationRecord>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetValMetaResponse {
    pub val_meta: Option<VMeta>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetFundsClaimRecord {
    pub user_withdrawal_amount: Uint128,
    pub protocol_fee: Uint128,
    pub undelegated_amount: Uint128,
}

// #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
// pub struct UserComputedInfo {
//     pub tokens: Uint128,
//     pub staked: Uint128,
//     pub undelegations: Uint128,
// }
