use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Decimal, Timestamp, Uint128};
use cw_storage_plus::{Item, Map, U64Key};
use stader_utils::coin_utils::DecCoin;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub manager: Addr,

    pub pool_contract: Addr,
    pub scc_denom: String,
    pub contract_genesis_block_height: u64,
    pub contract_genesis_timestamp: Timestamp,

    // total historical rewards accumulated in the SCC
    pub total_accumulated_rewards: Uint128,
    pub total_accumulated_airdrops: Vec<Coin>,

    pub current_undelegated_strategies: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StrategyInfo {
    pub name: String,
    pub sic_contract_address: Addr,
    pub unbonding_period: u64,
    pub unbonding_buffer: u64,
    pub undelegation_batch_id_pointer: u64,
    pub reconciled_batch_id_pointer: u64,
    pub is_active: bool,
    pub total_shares: Decimal,
    pub current_undelegated_shares: Decimal,
    pub global_airdrop_pointer: Vec<DecCoin>,
    pub total_airdrops_accumulated: Vec<Coin>,
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
            undelegation_batch_id_pointer: 0,
            reconciled_batch_id_pointer: 0,
            is_active: false,
            total_shares: Decimal::zero(),
            current_undelegated_shares: Decimal::zero(),
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
            undelegation_batch_id_pointer: 0,
            reconciled_batch_id_pointer: 0,
            is_active: false,
            total_shares: Decimal::zero(),
            current_undelegated_shares: Decimal::zero(),
            global_airdrop_pointer: vec![],
            total_airdrops_accumulated: vec![],
            shares_per_token_ratio: Decimal::from_ratio(10_u128, 1_u128),
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
    pub user_portfolio: Vec<UserStrategyPortfolio>,
    pub strategies: Vec<UserStrategyInfo>,
    // pending_airdrops is the airdrops accumulated from the validator_contract and all the strategy contracts
    pub pending_airdrops: Vec<Coin>,
    pub undelegation_records: Vec<UserUndelegationRecord>,
    // rewards which are not put into any strategy. they are just sitting in the SCC.
    // this is the "retain rewards" strategy
    pub pending_rewards: Uint128,
}

impl UserRewardInfo {
    pub fn default() -> Self {
        UserRewardInfo {
            user_portfolio: vec![],
            strategies: vec![],
            pending_airdrops: vec![],
            undelegation_records: vec![],
            pending_rewards: Default::default(),
        }
    }

    pub fn new() -> Self {
        UserRewardInfo {
            user_portfolio: vec![],
            strategies: vec![],
            pending_airdrops: vec![],
            undelegation_records: vec![],
            pending_rewards: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserUndelegationRecord {
    pub id: Timestamp,
    pub amount: Uint128,
    pub shares: Decimal,
    pub strategy_name: String,
    pub undelegation_batch_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserStrategyPortfolio {
    pub strategy_name: String,
    pub deposit_fraction: Decimal,
}

impl UserStrategyPortfolio {
    pub fn new(strategy_name: String, deposit_fraction: Decimal) -> Self {
        UserStrategyPortfolio {
            strategy_name,
            deposit_fraction,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Cw20TokenContractsInfo {
    pub airdrop_contract: Addr,
    pub cw20_token_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BatchUndelegationRecord {
    pub amount: Uint128,
    pub shares: Decimal,
    pub unbonding_slashing_ratio: Decimal,
    pub undelegation_s_t_ratio: Decimal,
    pub create_time: Timestamp,
    pub est_release_time: Timestamp,
    pub slashing_checked: bool,
}

pub const STATE: Item<State> = Item::new("state");

pub const STRATEGY_MAP: Map<&str, StrategyInfo> = Map::new("strategy_map");
pub const USER_REWARD_INFO_MAP: Map<&Addr, UserRewardInfo> = Map::new("user_reward_info_map");
pub const CW20_TOKEN_CONTRACTS_REGISTRY: Map<String, Cw20TokenContractsInfo> =
    Map::new("cw20_token_contracts_registry");
pub const UNDELEGATION_BATCH_MAP: Map<(U64Key, &str), BatchUndelegationRecord> =
    Map::new("undelegation_batch_map");
