use crate::state::Config;
use cosmwasm_std::{Addr, Coin, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub delegator_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UpdateUserRewardsRequest {
    pub user: Addr,
    // funds will be in native chain token
    pub funds: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserRewardInfoQuery {
    pub total_airdrops: Vec<Coin>,
    pub retained_rewards: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /*
       Delegator contract messages
    */
    // called by pools contract to transfer rewards from validator contract to SCC
    // this message also moves rewards from SCC to the corresponding SIC. This message will
    // transfer the rewards to the SIC per user. this is because the batching is already being done
    // by the pools contract. Calls to this message will be paginated.
    UpdateUserRewards {
        update_user_rewards_requests: Vec<UpdateUserRewardsRequest>,
    },

    // Used for offline swapping of rewards to Stader tokens during CF.
    WithdrawFunds {
        withdraw_address: Addr,
        amount: Uint128,
        denom: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetUserRewardInfo { user: Addr },
    GetConfig {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetConfigResponse {
    pub config: Config,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetUserRewardResponse {
    pub user_reward_info: Option<Uint128>,
}
