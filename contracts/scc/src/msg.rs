use crate::state::State;
use cosmwasm_std::{Addr, Coin, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub strategy_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UpdateUserRewardsRequest {
    pub user: Addr,
    // rewards will be in native chain token
    pub rewards: Uint128,
    // one of the registered strategies
    pub strategy_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UpdateUserAirdropsRequest {
    pub user: Addr,
    pub airdrops: Vec<Coin>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    RegisterStrategy {
        strategy_id: String,
        sic_contract_address: Addr,
        unbonding_period: Option<u64>,
        supported_airdrops: Vec<String>,
    },
    ActivateStrategy {
        strategy_id: String,
    },
    DeactivateStrategy {
        strategy_id: String,
    },
    RemoveStrategy {
        strategy_id: String,
    },
    UpdateUserRewards {
        update_user_rewards_request: Vec<UpdateUserRewardsRequest>,
    },
    // called by validator contract to transfer airdrops from validator contract to SCC
    UpdateUserAirdrops {
        update_user_airdrops_request: Vec<UpdateUserAirdropsRequest>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetState {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    pub state: Option<State>,
}
