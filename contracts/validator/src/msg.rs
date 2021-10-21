use crate::state::{Config, State, VMeta};
use cosmwasm_std::{Addr, Binary, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub vault_denom: String,
    pub pools_contract: Addr,
    pub scc_contract: Addr,
    pub delegator_contract: Addr,
    pub airdrop_withdraw_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    SetRewardWithdrawAddress {
        reward_contract: Addr,
    },
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
        amount: Uint128,
        claim_msg: Binary,
        airdrop_contract: Addr,
        cw20_contract: Addr, // Send message to this addr to move airdrops.
    },

    TransferReconciledFunds {
        amount: Uint128,
    },

    AddSlashingFunds {},
    RemoveSlashingFunds {
        amount: Uint128,
    },
    UpdateConfig {
        pools_contract: Option<Addr>,
        delegator_contract: Option<Addr>,
        airdrop_withdraw_contract: Option<Addr>
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
    ValidatorMeta { val_addr: Addr },
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
