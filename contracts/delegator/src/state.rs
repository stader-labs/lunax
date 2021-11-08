use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Decimal, Uint128};
use cw_storage_plus::{Item, Map, U64Key};
use stader_utils::coin_utils::DecCoin;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub manager: Addr,
    pub vault_denom: String,
    pub undelegations_max_limit: u32,
    pub pools_contract: Addr,
    pub scc_contract: Addr,
    pub protocol_fee: Decimal, // Used as a fee when a delegator withdraws staked funds.
    pub protocol_fee_contract: Addr, // Contract for the above fee. typically the revenue or treasury contract.
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub next_undelegation_id: u64,
}

// Get pool_id to traits & val_addr to moniker mapping from offchain APIs
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UndelegationInfo {
    pub batch_id: u64, // Need it so pools can make eligibility decision on withdraw_to_wallet
    pub id: u64,
    pub amount: Uint128,
    pub pool_id: u64,
    pub slashing_pointer: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct DepositInfo {
    pub staked: Uint128,
    // in process ones can be added here although we don't need to for the current model
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserPoolInfo {
    pub pool_id: u64, // redundant but avoids converting Vec<u8> on query_user.
    pub deposit: DepositInfo,
    pub airdrops_pointer: Vec<DecCoin>,
    pub pending_airdrops: Vec<Coin>,
    pub rewards_pointer: Decimal,
    pub pending_rewards: Uint128,
    pub slashing_pointer: Decimal,
    pub undelegations: Vec<UndelegationInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolPointerInfo {
    pub pool_id: u64,
    pub airdrops_pointer: Vec<DecCoin>,
    pub rewards_pointer: Decimal,
    pub slashing_pointer: Decimal,
}

// (User_Addr, Pool_id) -> UserPoolInfo
pub const USER_REGISTRY: Map<(&Addr, U64Key), UserPoolInfo> = Map::new("user_registry");

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");
