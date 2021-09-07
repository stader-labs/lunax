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
    pub event_loop_size: u64,

    // total historical rewards accumulated in the SCC
    pub total_accumulated_rewards: Uint128,
    // current rewards sitting in the SCC
    pub current_rewards_in_scc: Uint128,
    pub total_accumulated_airdrops: Vec<Coin>,

    pub current_undelegated_strategies: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StrategyInfo {
    pub name: String,
    pub sic_contract_address: Addr,
    pub unbonding_period: u64,
    pub unbonding_buffer: u64,
    pub current_undelegation_batch_id: u64,
    pub is_active: bool,
    pub total_shares: Decimal,
    pub global_airdrop_pointer: Vec<DecCoin>,
    pub total_airdrops_accumulated: Vec<Coin>,
    // TODO: bchain99 - i want this for strategy APR calc but cross check if we actually need this.
    // TODO: bchain99 - remove this. not needed. We are computing the S/T ratio on demand when needed for a strategy
    pub shares_per_token_ratio: Decimal,
}

impl StrategyInfo {
    pub(crate) fn new(
        strategy_name: String,
        sic_contract_address: Addr,
        unbonding_period: Option<u64>,
        unbonding_buffer: Option<u64>,
    ) -> Self {
        StrategyInfo {
            name: strategy_name,
            sic_contract_address,
            unbonding_period: unbonding_period.unwrap_or(21 * 24 * 3600),
            unbonding_buffer: unbonding_buffer.unwrap_or(3600),
            current_undelegation_batch_id: 0,
            is_active: false,
            total_shares: Decimal::zero(),
            global_airdrop_pointer: vec![],
            total_airdrops_accumulated: vec![],
            shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
        }
    }

    pub(crate) fn default(strategy_name: String) -> Self {
        StrategyInfo {
            name: strategy_name,
            sic_contract_address: Addr::unchecked("default-sic"),
            unbonding_period: 21 * 24 * 3600,
            unbonding_buffer: 3600,
            current_undelegation_batch_id: 0,
            is_active: false,
            total_shares: Default::default(),
            global_airdrop_pointer: vec![],
            total_airdrops_accumulated: vec![],
            shares_per_token_ratio: Default::default(),
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
    pub undelegation_records: Vec<UserUndelegationRecord>,
}

impl UserRewardInfo {
    pub fn default() -> Self {
        UserRewardInfo {
            strategies: vec![],
            pending_airdrops: vec![],
            undelegation_records: vec![],
        }
    }

    pub fn new() -> Self {
        UserRewardInfo {
            strategies: vec![],
            pending_airdrops: vec![],
            undelegation_records: vec![],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserUndelegationRecord {
    pub id: Timestamp,
    pub amount: Uint128,
    pub strategy_name: String,
    pub est_release_time: Timestamp,
    // the undelegation batch id is specific to the strategy. It is mainly used by the SIC
    // to account for any undelegation slashing or any form of impact which has occured to the user
    // during undelegations.
    pub undelegation_batch_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserUnprocessedUndelegationInfo {
    pub undelegation_amount: Uint128,
    pub strategy_name: String,
}

impl UserUnprocessedUndelegationInfo {
    pub fn default() -> Self {
        UserUnprocessedUndelegationInfo {
            undelegation_amount: Uint128::zero(),
            strategy_name: "".to_string(),
        }
    }

    pub fn new(strategy_name: String) -> Self {
        UserUnprocessedUndelegationInfo {
            undelegation_amount: Uint128::zero(),
            strategy_name,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Cw20TokenContractsInfo {
    pub airdrop_contract: Addr,
    pub cw20_token_contract: Addr,
}

pub const STATE: Item<State> = Item::new("state");

pub const STRATEGY_MAP: Map<&str, StrategyInfo> = Map::new("strategy_map");
pub const USER_REWARD_INFO_MAP: Map<&Addr, UserRewardInfo> = Map::new("user_reward_info_map");
pub const CW20_TOKEN_CONTRACTS_REGISTRY: Map<String, Cw20TokenContractsInfo> =
    Map::new("cw20_token_contracts_registry");
pub const USER_UNPROCESSED_UNDELEGATIONS: Map<&Addr, Vec<UserUnprocessedUndelegationInfo>> =
    Map::new("user_unprocessed_undelegations");
pub const STRATEGY_UNPROCESSED_UNDELEGATIONS: Map<&str, Uint128> =
    Map::new("strategy_unprocessed_undelegations");
