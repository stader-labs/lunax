use crate::state::{State, StrategyInfo};
use cosmwasm_std::{Addr, Coin, Timestamp, Uint128};
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
    pub pool_airdrops: Vec<Coin>,
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
    // called by validator contract to transfer rewards from validator contract to SCC
    // this message also moves rewards from SCC to the corresponding SIC
    UpdateUserRewards {
        update_user_rewards_requests: Vec<UpdateUserRewardsRequest>,
    },
    // called by validator contract to transfer airdrops from validator contract to SCC
    UpdateUserAirdrops {
        update_user_airdrops_requests: Vec<UpdateUserAirdropsRequest>,
    },
    // called by user to undelegate his rewards from a strategy
    UndelegateRewards {
        amount: Uint128,
        strategy_id: String,
    },
    // called by scc manager to periodically claim airdrops for a particular strategy if it supported
    ClaimAirdrops {
        strategy_id: String,
    },
    WithdrawRewards {
        undelegation_timestamp: Timestamp,
        strategy_id: String,
    },
    WithdrawAirdrops {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetState {},
    GetStrategyInfo {
        strategy_name: String
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetStateResponse {
    pub state: Option<State>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetStrategyInfoResponse {
    pub strategy_info: Option<StrategyInfo>,
}
