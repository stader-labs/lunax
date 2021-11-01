use crate::state::State;
use cosmwasm_std::{Addr, Binary, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub scc_address: String,
    pub reward_contract_address: String,
    // denomination of the staking coin
    pub strategy_denom: String,
    // initial set of validators who make up the validator pool
    pub initial_validators: Vec<Addr>,
    // minimum number of validators in a pool
    pub min_validator_pool_size: Option<u64>,
    // amount of funds sic-manager has seeded the sic with
    pub manager_seed_funds: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    TransferRewards {},
    UndelegateRewards {
        amount: Uint128,
    },
    TransferUndelegatedRewards {
        amount: Uint128,
    },
    Swap {},
    Reinvest {
        // if true, then only transferred_rewards are reinvested. rewards which are claimed by
        // the reward contract are not reinvested
        invest_transferred_rewards: Option<bool>,
    },
    RedeemRewards {},
    // Called by the manager to claim airdrops from different protocols. Airdrop token contract fed from SCC.
    // The ownership of the airdrops is transferred back to the SCC.
    ClaimAirdrops {
        airdrop_token_contract: String,
        // used to transfer ownership from SIC to SCC
        cw20_token_contract: String,
        // this is just for the SIC's reference.
        airdrop_token: String,
        amount: Uint128,
        claim_msg: Binary,
    },
    // Called by manager to add a validator to the current pool
    AddValidator {
        validator: String,
    },
    ReplaceValidator {
        src_validator: String,
        dst_validator: String,
    },
    RemoveValidator {
        removed_val: String,
        redelegate_val: String,
    },
    UpdateConfig {
        min_validator_pool_size: Option<u64>,
        scc_address: Option<String>,
    },
    SetRewardWithdrawAddress {
        reward_contract: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetTotalTokens {},
    GetState {},
    GetFulfillableUndelegatedFunds { amount: Uint128 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetStateResponse {
    pub state: Option<State>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetTotalTokensResponse {
    pub total_tokens: Option<Uint128>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetFulfillableUndelegatedFundsResponse {
    pub undelegated_funds: Option<Uint128>,
}
