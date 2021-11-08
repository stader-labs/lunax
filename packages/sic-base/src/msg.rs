use crate::state::State;
use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub scc_address: Addr,
    // denomination of the staking coin
    pub strategy_denom: String,
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
pub enum ExecuteMsg {
    // called by SCC to move rewards to SIC from SCC
    TransferRewards {},
    // called by SCC to undelegate 'amount' from SIC strategy back to SIC contract
    // All the batching for the undelegations is handled in the SCC
    // The undelegate rewards can choose to create a batch if it has to handle unbonding slashing (unfortunately)
    // or any form of slashing at the strategy end when unbonding the rewards from the strategy.
    // In order to attribute the user to the undelegation slashing, we need to keep track of an undelegation batch id
    // which is a unique id representing the batch.
    // The GetCurrentUndelegationBatchId query returns the undelegation batch id
    UndelegateRewards {
        amount: Uint128,
    },
    // Called by the SCC to transfer the undelegated rewards back to the SIC after the unbonding period.
    // The SIC needs to send back the undelegated rewards back to the SCC with best effort.
    // eg: If SCC has previously undelegated 800uluna and SIC receives only 780uluna, then SIC will
    // send back 780uluna. It will send back min(amount, received_undelegated_funds)
    TransferUndelegatedRewards {
        amount: Uint128,
    },
    // Called by the SCC to claim airdrops from different protocols for the strategy (if airdrop applies)
    // Airdrop token contract is fed from SCC.
    // The airdrops are claimed by the SIC contract. SCC sends a separate message to SIC to claim the
    // airdrops claimed by the SIC. We send a separate message to make sure we are accurately updating the airdrop
    // pointers in SCC.
    // In the current SIC/SCC design, airdrops are completely handled by SCC. SIC's are currently only responsible
    // for sending the airdrops back to the SCC.
    ClaimAirdrops {
        airdrop_token_contract: String,
        airdrop_token: String,
        amount: Uint128,
        stage: u8,
        proof: Vec<String>,
    },
    // Called by the SCC to transfer "amount" airdrop tokens back to the SCC. The SCC checks for the balance
    // of the token for the SIC and claims the tokens back and updates the airdrop pointers.
    TransferAirdropsToScc {
        cw20_token_contract: String,
        airdrop_token: String,
        amount: Uint128,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetTotalTokens {},
    GetFulfillableUndelegatedFunds { amount: Uint128 },
    GetState {},
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
