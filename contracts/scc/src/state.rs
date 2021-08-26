use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Decimal, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};
use std::collections::HashMap;
use std::fmt;
use std::fmt::Display;

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, JsonSchema)]
pub struct DecCoin {
    pub(crate) amount: Decimal,
    pub(crate) denom: String,
}

impl DecCoin {
    pub fn new<S: Into<String>>(amount: Decimal, denom: S) -> Self {
        DecCoin {
            amount,
            denom: denom.into(),
        }
    }
}

impl Display for DecCoin {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // We use the formatting without a space between amount and denom,
        // which is common in the Cosmos SDK ecosystem:
        // https://github.com/cosmos/cosmos-sdk/blob/v0.42.4/types/coin.go#L643-L645
        // For communication to end users, Coin needs to transformed anways (e.g. convert integer uatom to decimal ATOM).
        write!(f, "{}{}", self.amount, self.denom)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub manager: Addr,
    pub scc_denom: String,
    pub contract_genesis_block_height: u64,
    pub contract_genesis_timestamp: Timestamp,

    // total historical rewards accumulated in the SCC
    pub total_accumulated_rewards: Uint128,
    // current rewards sitting in the SCC
    pub current_rewards_in_scc: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StrategyInfo {
    pub name: String,
    pub sic_contract_address: Addr,
    pub unbonding_period: Option<u64>,
    pub supported_airdrops: Vec<String>,
    pub is_active: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StrategyMetadata {
    pub name: String,
    pub total_shares: Decimal,
    pub global_airdrop_pointer: Vec<DecCoin>,
    // TODO: bchain99 - i want this for strategy APR calc but cross check if we actually need this.
    pub shares_per_token_ratio: Decimal,
    pub current_unprocessed_undelegations: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserStrategyInfo {
    pub shares: Decimal,
    pub airdrop_pointer: Vec<DecCoin>,
    pub pending_airdrops: Vec<Coin>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserRewardInfo {
    pub strategy_map: HashMap<String, UserStrategyInfo>,
    // user airdrops which are currently owned by the SCC
    pub pending_airdrops: Vec<Coin>,
}

pub const STATE: Item<State> = Item::new("state");

pub const STRATEGY_INFO_MAP: Map<String, StrategyInfo> = Map::new("strategy_info_map");
pub const STRATEGY_METADATA_MAP: Map<String, StrategyMetadata> = Map::new("strategy_metadata_map");
pub const USER_REWARD_INFO_MAP: Map<Addr, UserRewardInfo> = Map::new("user_reward_info_map");
pub const AIRDROP_REGISTRY: Map<String, Addr> = Map::new("airdrop_registry");
