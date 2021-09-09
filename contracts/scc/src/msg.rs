use crate::state::{State, StrategyInfo, UserRewardInfo};
use cosmwasm_std::{Addr, Coin, Decimal, Timestamp, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub strategy_denom: String,

    pub pools_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UpdateUserRewardsRequest {
    pub user: Addr,
    // funds will be in native chain token
    pub funds: Uint128,
    // one of the registered strategies
    // if the strategy is provided then that means the user is depositing only to that strategy
    // if no strategy is provided then we iterate over the user portfolio
    pub strategy_name: Option<String>,
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
        strategy_name: String,
        sic_contract_address: Addr,
        unbonding_period: Option<u64>,
    },
    // TODO: bchain99 - Add an UpdateStrategy message rather than having different messages to activate/deactivate. Pick this up once SCC is merged
    // as other PRs will introduce new strategy params
    ActivateStrategy {
        strategy_name: String,
    },
    DeactivateStrategy {
        strategy_name: String,
    },
    RemoveStrategy {
        strategy_name: String,
    },
    UpdateUserPortfolio {
        strategy_name: String,
        deposit_fraction: Decimal,
    },
    // called by validator contract to transfer rewards from validator contract to SCC
    // this message also moves rewards from SCC to the corresponding SIC. This message will
    // transfer the rewards to the SIC per user. this is because the batching is already being done
    // by the pools contract. Calls to this message will be paginated.
    UpdateUserRewards {
        update_user_rewards_requests: Vec<UpdateUserRewardsRequest>,
    },
    // called by validator contract to transfer airdrops from validator contract to SCC. This will be a separate
    // epoch in the pools contract.
    UpdateUserAirdrops {
        update_user_airdrops_requests: Vec<UpdateUserAirdropsRequest>,
    },
    // called by user to undelegate his rewards from a strategy
    UndelegateRewards {
        amount: Uint128,
        strategy_name: String,
    },
    // called by scc manager to periodically claim airdrops for a particular strategy if it supported
    ClaimAirdrops {
        strategy_name: String,
    },
    WithdrawRewards {
        undelegation_timestamp: Timestamp,
        strategy_name: String,
    },
    WithdrawAirdrops {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetState {},
    GetStrategyInfo { strategy_name: String },
    GetUserRewardInfo { user: Addr },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetStateResponse {
    pub state: Option<State>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetStrategyInfoResponse {
    pub strategy_info: Option<StrategyInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetUserRewardInfo {
    pub user_reward_info: Option<UserRewardInfo>,
}
