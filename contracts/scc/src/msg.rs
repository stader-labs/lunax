use crate::state::{State, StrategyInfo, UserRewardInfo};
use cosmwasm_std::{Addr, Binary, Coin, Timestamp, Uint128};
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
    /*
       Manager messages
    */
    RegisterStrategy {
        strategy_id: String,
        sic_contract_address: Addr,
        unbonding_buffer: Option<u64>,
        unbonding_period: Option<u64>,
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
    RegisterCw20Contracts {
        denom: String,
        cw20_contract: Addr,
        airdrop_contract: Addr,
    },
    // called by scc manager to periodically claim airdrops for a particular strategy if it supported
    ClaimAirdrops {
        strategy_id: String,
        amount: Uint128,
        denom: String,
        claim_msg: Binary,
    },
    // undelegate all the queued up undelegation from all strategies. This takes into account
    // a cooling period for the strategy. Certain strategies cannot be undelegate
    UndelegateFromStrategies {
        strategies: Vec<String>,
    },
    // creates the undelegation records for the users.
    // CreateUserUndelegationRecords {},
    CreateUndelegationBatches {
        strategies: Vec<String>,
    },
    /*
       Pools contract messages
    */
    // called by pools contract to transfer rewards from validator contract to SCC
    // this message also moves rewards from SCC to the corresponding SIC. This message will
    // transfer the rewards to the SIC per user. this is because the batching is already being done
    // by the pools contract. Calls to this message will be paginated.
    UpdateUserRewards {
        update_user_rewards_requests: Vec<UpdateUserRewardsRequest>,
    },
    // called by pools contract to transfer airdrops from validator contract to SCC. This will be a separate
    // epoch in the pools contract.
    UpdateUserAirdrops {
        update_user_airdrops_requests: Vec<UpdateUserAirdropsRequest>,
    },
    /*
       User messages
    */
    // called by user to undelegate his rewards from a strategy
    UndelegateRewards {
        amount: Uint128,
        strategy_name: String,
    },
    WithdrawRewards {
        undelegation_id: String,
        strategy_name: String,
        amount: Uint128,
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
