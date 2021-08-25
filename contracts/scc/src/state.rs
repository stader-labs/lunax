use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal, Coin};
use cw_storage_plus::{Item, Map};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub count: i32,
    pub owner: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StrategyInfo {
    pub name: String,
    pub unbonding_period: Option<u64>,
    pub has_airdrops: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StrategyMetadata {
    pub name: String,
    pub total_shares: Decimal,
    pub global_airdrop_pointer: Vec<DecCoin>,
    // TODO: bchain99 - i want this for strategy APR calc but cross check if we actually need this.
    pub shares_per_token_ratio: Decimal
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserStrategyInfo {
    pub shares: Decimal,
    pub airdrop_pointer: Vec<DecCoin>,
    pub pending_airdrops: Vec<Coin>
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserRewardInfo {
    pub strategy_map: HashMap<String, UserStrategyInfo>
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");

pub const STRATEGY_INFO_MAP: Map<String, StrategyInfo> = Map::new("strategy_info_map");
pub const STRATEGY_METADATA_MAP: Map<String, StrategyMetadata> = Map::new("strategy_metadata_map");
pub const USER_REWARD_INFO_MAP: Map<Addr, UserRewardInfo> = Map::new("user_reward_info_map");
