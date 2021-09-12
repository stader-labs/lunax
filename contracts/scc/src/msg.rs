use crate::state::{BatchUndelegationRecord, State, StrategyInfo, UserRewardInfo};
use cosmwasm_std::{Addr, Binary, Coin, Decimal, Timestamp, Uint128};
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
    /*
       Manager messages
    */
    RegisterStrategy {
        strategy_name: String,
        sic_contract_address: Addr,
        unbonding_buffer: Option<u64>,
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
    UpdateStrategy {
        strategy_name: String,
        unbonding_period: u64,
        unbonding_buffer: u64,
        is_active: bool,
    },
    UpdateUserPortfolio {
        strategy_name: String,
        deposit_fraction: Decimal,
    },
    RegisterCw20Contracts {
        denom: String,
        cw20_contract: Addr,
        airdrop_contract: Addr,
    },
    // undelegate all the queued up undelegation from all strategies. This takes into account
    // a cooling period for the strategy. Certain strategies cannot be undelegate
    UndelegateFromStrategies {
        strategies: Vec<String>,
    },
    // called by manager, this message goes to the SICs and fetches the undelegated rewards which are
    // sitting in the SIC.
    FetchUndelegatedRewardsFromStrategies {
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
    // called by scc manager to periodically claim airdrops for a particular strategy if it supported
    ClaimAirdrops {
        amount: Uint128,
        denom: String,
        claim_msg: Binary,
        strategy_name: String,
    },
    WithdrawRewards {
        undelegation_id: String,
        strategy_name: String,
        amount: Uint128,
    },
    // called by the user to withdraw pending rewards i.e rewards which are not in any strategy
    WithdrawPendingRewards {},
    WithdrawAirdrops {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetState {},
    GetStrategyInfo {
        strategy_name: String,
    },
    GetUndelegationBatchInfo {
        strategy_name: String,
        batch_id: u64,
    },
    GetUserRewardInfo {
        user: Addr,
    },
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetUndelegationBatchInfoResponse {
    pub undelegation_batch_info: Option<BatchUndelegationRecord>,
}
