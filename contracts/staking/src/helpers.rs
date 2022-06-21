#![allow(dead_code)]

use crate::state::{
    BatchUndelegationRecord, Config, VMeta, BATCH_UNDELEGATION_REGISTRY, STATE, VALIDATOR_META,
};
use crate::ContractError;
use airdrops_registry::msg::GetAirdropContractsResponse;
use airdrops_registry::msg::QueryMsg as AirdropsQueryMsg;
use cosmwasm_std::{
    to_binary, Addr, Decimal, Delegation, DepsMut, Env, MessageInfo, QuerierWrapper, StdResult,
    Storage, Uint128, WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, TokenInfoResponse};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Verify {
    SenderManager,

    //Info.funds is expected to be one
    NonZeroSingleInfoFund,
    // If info.funds are empty or zero
    // NonEmptyInfoFunds,
    NoFunds,
}

pub fn validate_unbonding_period(unbonding_period: u64) -> bool {
    // unbonding period should be in [21 days, 21 days + 30mins]
    unbonding_period < 1816200 && unbonding_period >= 1814400
}

pub fn validate_undelegation_cooldown(undelegation_cooldown: u64) -> bool {
    // undelegation cooldown should be in [3 days - 10mins, 3 days + 10mins]
    undelegation_cooldown <= 259800 && undelegation_cooldown >= 258600
}

pub fn validate_min_deposit(min_deposit: Uint128) -> bool {
    // Min deposit should be b/w 10uluna and 1luna
    min_deposit.ge(&Uint128::new(10)) && min_deposit.le(&Uint128::new(1_000_000))
}

pub fn validate_max_deposit(max_deposit: Uint128) -> bool {
    // Min deposit should be b/w 100 luna to 1 million luna
    max_deposit.ge(&Uint128::new(100_000_000)) && max_deposit.le(&Uint128::new(1_000_000_000_000))
}

// Let's not add assertions for these checks in other tests
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

// Take in validator staked amounts into pool if the pool size is bigger.
pub fn get_validator_for_deposit(
    querier: QuerierWrapper,
    validators: Vec<Addr>,
    all_delegations: &[Delegation],
) -> Result<Addr, ContractError> {
    if validators.is_empty() {
        return Err(ContractError::NoValidatorsInPool {});
    }
    let all_terra_validators = querier.query_all_validators()?;

    let mut stake_tuples = vec![];
    for val_addr in validators {
        let validator = all_terra_validators
            .iter()
            .find(|x| x.address.eq(&val_addr));
        if validator.is_none() {
            continue;
        }

        let delegation_opt = all_delegations.iter().find(|x| x.validator.eq(&val_addr));

        if delegation_opt.is_none() {
            // No delegation. So use the validator
            return Ok(val_addr);
        }
        stake_tuples.push((
            delegation_opt.unwrap().amount.amount.u128(),
            val_addr.to_string(),
        ))
    }
    if stake_tuples.is_empty() {
        return Err(ContractError::AllValidatorsJailed {});
    }
    stake_tuples.sort();
    Ok(Addr::unchecked(stake_tuples.first().unwrap().clone().1))
}

// Take in validator staked amounts into pool if the pool size is bigger.
pub fn get_active_validators_sorted_by_stake(
    querier: QuerierWrapper,
    validators: Vec<Addr>,
    all_delegations: &[Delegation],
) -> Result<Vec<(Uint128, String)>, ContractError> {
    if validators.is_empty() {
        return Err(ContractError::NoValidatorsInPool {});
    }
    let all_validators = querier.query_all_validators()?;

    let mut stake_tuples = vec![];
    for val_addr in validators {
        let validator = all_validators.iter().find(|x| x.address.eq(&val_addr));
        if validator.is_none() {
            continue;
        }
        let delegation_opt = all_delegations.iter().find(|x| x.validator.eq(&val_addr));

        if let Some(full_delegation) = delegation_opt {
            stake_tuples.push((full_delegation.amount.amount, val_addr.to_string()))
        } else {
            stake_tuples.push((Uint128::zero(), val_addr.to_string()));
        }
    }
    if stake_tuples.is_empty() {
        return Err(ContractError::AllValidatorsJailed {});
    }
    stake_tuples.sort();
    Ok(stake_tuples)
}

pub fn create_new_undelegation_batch(
    storage: &mut dyn Storage,
    env: Env,
) -> Result<(), ContractError> {
    let mut state = STATE.load(storage)?;
    let next_undelegation_batch_id = state.current_undelegation_batch_id + 1;
    BATCH_UNDELEGATION_REGISTRY.save(
        storage,
        next_undelegation_batch_id,
        &BatchUndelegationRecord {
            undelegated_tokens: Uint128::zero(),
            create_time: env.block.time,
            est_release_time: None,
            reconciled: false,
            undelegation_er: state.exchange_rate,
            undelegated_stake: Uint128::zero(),
            unbonding_slashing_ratio: Decimal::one(),
        },
    )?;
    state.current_undelegation_batch_id += 1;
    STATE.save(storage, &state)?;
    Ok(())
}

pub fn increase_tracked_stake(
    deps: &mut DepsMut,
    val_addr: &Addr,
    amount: Uint128,
) -> Result<(), ContractError> {
    VALIDATOR_META.update(deps.storage, val_addr, |x| -> StdResult<_> {
        let mut vmeta = x.unwrap_or_else(VMeta::new);
        vmeta.staked = vmeta.staked.checked_add(amount).unwrap();
        Ok(vmeta)
    })?;
    Ok(())
}

pub fn decrease_tracked_stake(
    deps: &mut DepsMut,
    val_addr: &Addr,
    amount: Uint128,
) -> Result<(), ContractError> {
    VALIDATOR_META.update(deps.storage, val_addr, |x| -> StdResult<_> {
        let mut vmeta = x.unwrap_or_else(VMeta::new);
        vmeta.staked = vmeta.staked.saturating_sub(amount);
        Ok(vmeta)
    })?;
    Ok(())
}

pub fn decrease_tracked_slashing(
    deps: &mut DepsMut,
    val_addr: &Addr,
    amount: Uint128,
) -> Result<(), ContractError> {
    VALIDATOR_META.update(deps.storage, val_addr, |x| -> StdResult<_> {
        let mut vmeta = x.unwrap_or_else(VMeta::new);
        vmeta.slashed = vmeta.slashed.saturating_sub(amount);
        Ok(vmeta)
    })?;
    Ok(())
}

pub fn calculate_exchange_rate(total_staked: Uint128, total_token_supply: Uint128) -> Decimal {
    if total_staked.is_zero() || total_token_supply.is_zero() {
        return Decimal::one();
    }
    Decimal::from_ratio(total_staked, total_token_supply)
}

pub fn get_airdrop_contracts(
    querier_wrapper: QuerierWrapper,
    airdrop_registry_contract: Addr,
    token: String,
) -> StdResult<GetAirdropContractsResponse> {
    querier_wrapper.query_wasm_smart(
        airdrop_registry_contract,
        &AirdropsQueryMsg::GetAirdropContracts { token },
    )
}

pub fn get_total_token_supply(
    querier_wrapper: QuerierWrapper,
    token_contract_addr: Addr,
) -> StdResult<Uint128> {
    let token_info_res: TokenInfoResponse = querier_wrapper
        .query_wasm_smart(token_contract_addr.to_string(), &Cw20QueryMsg::TokenInfo {})?;
    Ok(token_info_res.total_supply)
}

pub fn get_user_balance(
    querier_wrapper: QuerierWrapper,
    token_contract_addr: Addr,
    user_addr: Addr,
) -> StdResult<Uint128> {
    let balance_res: BalanceResponse = querier_wrapper.query_wasm_smart(
        token_contract_addr.to_string(),
        &Cw20QueryMsg::Balance {
            address: user_addr.to_string(),
        },
    )?;

    Ok(balance_res.balance)
}

pub fn create_mint_message(
    token_contract_addr: Addr,
    amount: Uint128,
    recipient: Addr,
) -> StdResult<WasmMsg> {
    Ok(WasmMsg::Execute {
        contract_addr: token_contract_addr.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Mint {
            recipient: recipient.to_string(),
            amount,
        })?,
        funds: vec![],
    })
}

pub fn burn_minted_tokens(token_contract_addr: Addr, amount: Uint128) -> StdResult<WasmMsg> {
    Ok(WasmMsg::Execute {
        contract_addr: token_contract_addr.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Burn { amount })?,
        funds: vec![],
    })
}
