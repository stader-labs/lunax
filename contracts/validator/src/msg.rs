use crate::state::Config;
use cosmwasm_std::{Addr, Binary, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {}

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
    ClaimAirdrop {
        amount: Uint128,
        claim_msg: Binary,
        airdrop_contract: Addr,
    },
    TransferAirdrop {
        amount: Uint128,
        cw20_contract: Addr,
    },
    TransferReconciledFunds {
        amount: Uint128,
    },
    UpdateConfig {
        pools_contract: Option<Addr>,
        delegator_contract: Option<Addr>,
        airdrop_withdraw_contract: Option<Addr>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    ValidatorMeta { val_addr: Addr },
    GetUnaccountedBaseFunds {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetConfigResponse {
    pub config: Config,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetValidatorMetaResponse {
    pub val_meta: bool,
}
