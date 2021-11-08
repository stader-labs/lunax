use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MerkleAirdropMsg {
    Claim {
        stage: u8,
        amount: Uint128,
        proof: Vec<String>,
    },
}