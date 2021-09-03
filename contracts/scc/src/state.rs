use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Decimal, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};
use stader_utils::coin_utils::DecCoin;
use std::collections::HashMap;
use std::fmt;
use std::fmt::Display;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub manager: Addr,

    pub pool_contract: Addr,
    pub scc_denom: String,
    pub contract_genesis_block_height: u64,
    pub contract_genesis_timestamp: Timestamp,

    // total historical rewards accumulated in the SCC
    pub total_accumulated_rewards: Uint128,
    // current rewards sitting in the SCC
    pub current_rewards_in_scc: Uint128,
    pub total_accumulated_airdrops: Vec<Coin>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StrategyInfo {
    pub name: String,
    pub sic_contract_address: Addr,
    pub unbonding_period: Option<u64>,
    pub is_active: bool,
    pub total_shares: Decimal,
    pub global_airdrop_pointer: Vec<DecCoin>,
    pub total_airdrops_accumulated: Vec<Coin>,
    // TODO: bchain99 - i want this for strategy APR calc but cross check if we actually need this.
    pub shares_per_token_ratio: Decimal,
    pub current_unprocessed_undelegations: Uint128,
}

impl StrategyInfo {
    pub(crate) fn new(
        strategy_name: String,
        sic_contract_address: Addr,
        unbonding_period: Option<u64>,
    ) -> Self {
        StrategyInfo {
            name: strategy_name,
            sic_contract_address,
            unbonding_period,
            is_active: false,
            total_shares: Decimal::zero(),
            global_airdrop_pointer: vec![],
            total_airdrops_accumulated: vec![],
            shares_per_token_ratio: Decimal::from_ratio(100_000_000_u128, 1_u128),
            current_unprocessed_undelegations: Uint128::zero(),
        }
    }

    pub(crate) fn default(strategy_name: String) -> Self {
        StrategyInfo {
            name: strategy_name,
            sic_contract_address: Addr::unchecked("default-sic"),
            unbonding_period: None,
            is_active: false,
            total_shares: Default::default(),
            global_airdrop_pointer: vec![],
            total_airdrops_accumulated: vec![],
            shares_per_token_ratio: Default::default(),
            current_unprocessed_undelegations: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserStrategyInfo {
    pub strategy_name: String,
    pub shares: Decimal,
    // airdrop here is an unswappable reward. For v1, we are keeping airdrops idle and distributing
    // them back to the user. The airdrop_pointer here is only for the particular strategy.
    pub airdrop_pointer: Vec<DecCoin>,
}

impl UserStrategyInfo {
    pub fn default() -> Self {
        UserStrategyInfo {
            strategy_name: "".to_string(),
            shares: Default::default(),
            airdrop_pointer: vec![],
        }
    }
    pub fn new(strategy_name: String, airdrop_pointer: Vec<DecCoin>) -> Self {
        UserStrategyInfo {
            strategy_name,
            shares: Decimal::zero(),
            airdrop_pointer,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserRewardInfo {
    pub strategies: Vec<UserStrategyInfo>,
    // pending_airdrops is the airdrops accumulated from the validator_contract and all the strategy contracts
    pub pending_airdrops: Vec<Coin>,
}

impl UserRewardInfo {
    pub fn new() -> Self {
        UserRewardInfo {
            strategies: vec![],
            pending_airdrops: vec![],
        }
    }
}

pub const STATE: Item<State> = Item::new("state");

pub const STRATEGY_MAP: Map<&str, StrategyInfo> = Map::new("strategy_map");
pub const USER_REWARD_INFO_MAP: Map<&Addr, UserRewardInfo> = Map::new("user_reward_info_map");
pub const AIRDROP_REGISTRY: Map<&str, Addr> = Map::new("airdrop_registry");
