use crate::state::{
    BatchUndelegationRecord, Config, PoolRegistryInfo, BATCH_UNDELEGATION_REGISTRY, POOL_REGISTRY,
    VALIDATOR_REGISTRY,
};
use crate::ContractError;
use cosmwasm_std::{Addr, Env, MessageInfo, Storage, Uint128};
use cw_storage_plus::U64Key;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Verify {
    // If info.sender != manager, throw error
    SenderManager,

    // SenderPoolsContract,
    //
    // SenderManagerOrPoolsContract,
    //
    // SenderManagerOrPoolsContractOrSelf,

    //Info.funds is expected to be one
    NonZeroSingleInfoFund,
    // If info.funds are empty or zero
    // NonEmptyInfoFunds,
    NoFunds,
}

pub fn validate(
    config: &Config,
    info: &MessageInfo,
    _env: &Env,
    checks: Vec<Verify>,
) -> Result<(), ContractError> {
    for check in checks {
        match check {
            Verify::SenderManager => {
                if info.sender != config.manager {
                    return Err(ContractError::Unauthorized {});
                }
            }
            Verify::NonZeroSingleInfoFund => {
                if info.funds.is_empty() || info.funds[0].amount.is_zero() {
                    return Err(ContractError::NoFunds {});
                }
                if info.funds.len() > 1 {
                    return Err(ContractError::MultipleFunds {});
                }
                if info.funds[0].denom != config.vault_denom {
                    return Err(ContractError::InvalidDenom {});
                }
            }
            Verify::NoFunds => {
                if !info.funds.is_empty() {
                    return Err(ContractError::FundsNotExpected {});
                }
            }
        }
    }

    Ok(())
}

pub fn get_verified_pool(
    storage: &mut dyn Storage,
    pool_id: u64,
    active_check: bool,
) -> Result<PoolRegistryInfo, ContractError> {
    let pool_meta_opt = POOL_REGISTRY.may_load(storage, U64Key::new(pool_id))?;
    if pool_meta_opt.is_none() {
        return Err(ContractError::PoolNotFound {});
    }
    let pool_meta = pool_meta_opt.unwrap();
    if active_check && !pool_meta.active {
        return Err(ContractError::PoolInactive {});
    }
    Ok(pool_meta)
}

// Take in validator staked amounts into pool if the pool size is bigger.
pub fn get_validator_for_deposit(
    storage: &mut dyn Storage,
    validators: Vec<Addr>,
) -> Result<Addr, ContractError> {
    if validators.is_empty() {
        return Err(ContractError::NoValidatorsInPool {});
    }
    let mut min_staked = Uint128::new(u128::MAX);
    let mut min_val_addr = Addr::unchecked("invalid_address");
    for val_addr in validators {
        let val_meta = VALIDATOR_REGISTRY.load(storage, &val_addr).unwrap();
        if min_staked.ge(&val_meta.staked) {
            min_val_addr = val_addr;
            min_staked = val_meta.staked;
        }
    }

    Ok(min_val_addr)
}

// Take in validator staked amounts into pool if the pool size is bigger.
pub fn get_validator_for_undelegate(
    storage: &mut dyn Storage,
    validators: Vec<Addr>,
) -> Result<Addr, ContractError> {
    if validators.is_empty() {
        return Err(ContractError::NoValidatorsInPool {});
    }
    let mut max_staked = Uint128::zero();
    let mut max_val_addr = Addr::unchecked("invalid_address");
    for val_addr in validators {
        let val_meta = VALIDATOR_REGISTRY.load(storage, &val_addr).unwrap();
        if max_staked.le(&val_meta.staked) {
            max_val_addr = val_addr;
            max_staked = val_meta.staked;
        }
    }

    Ok(max_val_addr)
}

pub fn create_new_undelegation_batch(
    storage: &mut dyn Storage,
    env: Env,
    pool_id: u64,
    pool_meta: &mut PoolRegistryInfo,
) -> Result<(), ContractError> {
    pool_meta.current_undelegation_batch_id += 1;
    let new_batch_id = pool_meta.current_undelegation_batch_id;
    POOL_REGISTRY.save(storage, U64Key::new(pool_id), &pool_meta)?;

    BATCH_UNDELEGATION_REGISTRY.save(
        storage,
        (U64Key::new(pool_id), U64Key::new(new_batch_id)),
        &BatchUndelegationRecord {
            amount: Uint128::zero(),
            create_time: env.block.time,
            est_release_time: None,
            withdrawable_time: None,
        },
    )?;
    Ok(())
}
