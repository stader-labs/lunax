use crate::state::{AirdropRegistryInfo, Config, State, VMeta};
use cosmwasm_std::{Addr, Binary, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub vault_denom: String,
    pub pools_contract: Addr,
    pub scc_contract: Addr,
    pub delegator_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    AddValidator {
        val_addr: Addr,
    },
    RemoveValidator {
        val_addr: Addr,
        redelegate_addr: Addr,
    },
    Stake {
        val_addr: Addr,
    },
    RedeemRewards {
        validators: Vec<Addr>,
    },
    Redelegate {
        src: Addr,
        dst: Addr,
        amount: Uint128,
    },
    Undelegate {
        val_addr: Addr,
        amount: Uint128,
    },
    RedeemAirdropAndTransfer {
        airdrop_token: String,
        amount: Uint128,
        claim_msg: Binary,
    },
    SwapAndTransfer {
        validators: Vec<Addr>,
        identifier: String,
    },

    TransferReconciledFunds {
        amount: Uint128,
    },

    UpdateAirdropRegistry {
        denom: String,
        airdrop_contract: Addr,
        token_contract: Addr,
    },
    AddSlashingFunds {},
    RemoveSlashingFunds {
        amount: Uint128,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetConfig {},
    GetState {},
    GetValidatorMeta { val_addr: Addr },
    GetAirdropMeta { token: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetConfigResponse {
    pub config: Config,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetStateResponse {
    pub state: State,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetValidatorMetaResponse {
    pub val_meta: Option<VMeta>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetAirdropMetaResponse {
    pub airdrop_meta: Option<AirdropRegistryInfo>,
}
