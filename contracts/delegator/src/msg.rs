use crate::state::{Config, PoolPointerInfo, State, UserPoolInfo};
use cosmwasm_std::{Addr, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stader_utils::coin_utils::DecCoin;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub vault_denom: String,
    pub pools_contract: Addr,
    pub scc_contract: Addr,
    pub protocol_fee: Decimal,
    pub protocol_fee_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Deposit {
        user_addr: Addr,
        pool_id: u64,
        amount: Uint128,
        pool_rewards_pointer: Decimal,
        pool_airdrops_pointer: Vec<DecCoin>,
        pool_slashing_pointer: Decimal,
    },
    Undelegate {
        user_addr: Addr,
        batch_id: u64,
        from_pool: u64,
        amount: Uint128,
        pool_rewards_pointer: Decimal,
        pool_airdrops_pointer: Vec<DecCoin>,
        pool_slashing_pointer: Decimal,
    },
    WithdrawFunds {
        user_addr: Addr,
        pool_id: u64,
        undelegate_id: u64,
        undelegation_batch_slashing_pointer: Decimal,
        undelegation_batch_unbonding_slashing_ratio: Decimal,
    },
    AllocateRewards {
        user_addrs: Vec<Addr>,
        pool_pointers: Vec<PoolPointerInfo>,
    },
    UpdateConfig {
        pools_contract: Option<Addr>,
        scc_contract: Option<Addr>,
        protocol_fee: Option<Decimal>,
        protocol_fee_contract: Option<Addr>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
    User {
        user_addr: Addr,
    },
    UserPool {
        user_addr: Addr,
        pool_id: u64,
    },
    ComputeUndelegationAmounts {
        user_addr: Addr,
        pool_id: u64,
        undelegate_id: u64,
        undelegation_slashing_pointer: Decimal, // Undelegation batch's last slashing pointer update.
        batch_slashing_ratio: Decimal,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetConfigResponse {
    pub config: Config,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetStateResponse {
    pub state: State,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserPoolResponse {
    pub info: Option<UserPoolInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserResponse {
    pub info: Vec<UserPoolInfo>,
}
