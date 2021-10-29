use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Decimal, Timestamp, Uint128};
use cw_storage_plus::{Item, Map, U64Key};
use stader_utils::coin_utils::DecCoin;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub default_user_portfolio: Vec<UserStrategyPortfolio>,
    // this is the strategy we will fallback to, if the strategy
    // in the user portfolio doesn't exist or is deactivated.
    pub fallback_strategy: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub manager: Addr,

    pub delegator_contract: Addr,
    pub scc_denom: String,
    pub contract_genesis_block_height: u64,
    pub contract_genesis_timestamp: Timestamp,

    pub next_undelegation_id: u64,
    pub next_strategy_id: u64,

    // sum of all the retained rewards in scc
    pub rewards_in_scc: Uint128,
    pub total_accumulated_airdrops: Vec<Coin>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StrategyInfo {
    // name of the strategy
    pub name: String,
    // address of the SIC for the strategy
    pub sic_contract_address: Addr,
    // the actual unbonding period for the strategy.
    // eg: unbonding_period for the auto compounding strat
    pub unbonding_period: u64,
    // adds buffer for the bots to execute
    pub unbonding_buffer: u64,
    // the id of the next undelegation batch which is created
    pub next_undelegation_batch_id: u64,
    // the id of the next undelegation batch to start reconciling
    pub next_reconciliation_batch_id: u64,
    // stops deposits to the strategy if it false. if anyone deposits to a deactivated strategy
    // their rewards go to the fallback strategy
    pub is_active: bool,
    // the total shares of this strategy
    pub total_shares: Decimal,
    // the total shares in undelegation. when undelegate_from_strategies run, these shares are undelegated from the
    // sic
    pub current_undelegated_shares: Decimal,
    pub global_airdrop_pointer: Vec<DecCoin>,
    // total airdrop accumulated in the strategy since inception
    pub total_airdrops_accumulated: Vec<Coin>,
    // the latest shares_per_token ratio value used by the strategy
    // the shares_per_token ratio is computed on demand
    pub shares_per_token_ratio: Decimal,
}

impl StrategyInfo {
    pub(crate) fn new(
        strategy_name: String,
        sic_contract_address: Addr,
        unbonding_period: u64,
        unbonding_buffer: u64,
    ) -> Self {
        StrategyInfo {
            name: strategy_name,
            sic_contract_address,
            unbonding_period,
            unbonding_buffer,
            next_undelegation_batch_id: 0,
            next_reconciliation_batch_id: 0,
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
            next_undelegation_batch_id: 0,
            next_reconciliation_batch_id: 0,
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
    pub strategy_id: u64,
    // total shares allocated to this user for the particular strategy
    pub shares: Decimal,
    // airdrop here is an unswappable reward. For v1, we are keeping airdrops idle and distributing
    // them back to the user. The airdrop_pointer here is only for the particular strategy.
    pub airdrop_pointer: Vec<DecCoin>,
}

impl UserStrategyInfo {
    pub fn default() -> Self {
        UserStrategyInfo {
            strategy_id: 0,
            shares: Default::default(),
            airdrop_pointer: vec![],
        }
    }
    pub fn new(strategy_id: u64, airdrop_pointer: Vec<DecCoin>) -> Self {
        UserStrategyInfo {
            strategy_id,
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

    pub fn new(default_user_portfolio: Vec<UserStrategyPortfolio>) -> Self {
        UserRewardInfo {
            user_portfolio: default_user_portfolio,
            strategies: vec![],
            pending_airdrops: vec![],
            undelegation_records: vec![],
            pending_rewards: Default::default(),
        }
    }
}

// estimated release time is fetched from the undelegation batch id
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserUndelegationRecord {
    pub id: u64,
    pub amount: Uint128,
    pub shares: Decimal,
    pub strategy_id: u64,
    pub undelegation_batch_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserStrategyPortfolio {
    pub strategy_id: u64,
    // deposit_fraction is always b/w 0 and 100
    pub deposit_fraction: Uint128,
}

impl UserStrategyPortfolio {
    pub fn new(strategy_id: u64, deposit_fraction: Uint128) -> Self {
        UserStrategyPortfolio {
            strategy_id,
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
#[serde(rename_all = "snake_case")]
pub enum UndelegationBatchStatus {
    Pending,
    InProgress,
    Done,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BatchUndelegationRecord {
    pub amount: Uint128,
    pub shares: Decimal,
    pub unbonding_slashing_ratio: Decimal,
    pub undelegation_s_t_ratio: Decimal,
    pub create_time: Option<Timestamp>,
    pub est_release_time: Option<Timestamp>,
    pub withdrawal_time: Option<Timestamp>,
    pub undelegation_batch_status: UndelegationBatchStatus,
    pub released: bool,
}

pub const STATE: Item<State> = Item::new("state");
pub const CONFIG: Item<Config> = Item::new("config");

pub const MAX_PAGINATION_LIMIT: u32 = 30;
pub const DEFAULT_PAGINATION_LIMIT: u32 = 10;

pub const STRATEGY_MAP: Map<U64Key, StrategyInfo> = Map::new("strategy_map");
pub const USER_REWARD_INFO_MAP: Map<&Addr, UserRewardInfo> = Map::new("user_reward_info_map");
pub const CW20_TOKEN_CONTRACTS_REGISTRY: Map<String, Cw20TokenContractsInfo> =
    Map::new("cw20_token_contracts_registry");
pub const UNDELEGATION_BATCH_MAP: Map<(U64Key, U64Key), BatchUndelegationRecord> =
    Map::new("undelegation_batch_map");
