use crate::state::State;
use cosmwasm_std::{Addr, Binary, Uint128};
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
pub enum ExecuteMsg {
    // called by SCC to move rewards to SIC from SCC
    TransferRewards {},
    // called by SCC to undelegate 'amount' from SIC strategy back to SIC contract
    // All the batching for the undelegations is handled in the SCC
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
    // The airdrops are claimed by the SIC contract and then the ownership of the airdrops are transferred back to the SCC.
    // In the current SIC/SCC design, airdrops are completely handled by SCC. SIC's are currently only responsible
    // for sending the airdrops back to the SCC.
    ClaimAirdrops {
        airdrop_token_contract: Addr,
        // used to transfer ownership from SIC to SCC
        cw20_token_contract: Addr,
        airdrop_token: String,
        amount: Uint128,
        claim_msg: Binary,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetTotalTokens {},
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
