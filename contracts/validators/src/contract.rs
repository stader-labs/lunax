#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Coin, Decimal, Deps, DepsMut, DistributionMsg, Env, Event,
    MessageInfo, Response, StakingMsg, StdResult, Uint128,
};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, GetConfigResponse, InstantiateMsg, QueryMsg};
use crate::request_validation::{validate, Verify};
use crate::state::{Config, VMeta, CONFIG, VALIDATOR_META};
use stader_utils::coin_utils::{
    merge_coin, merge_coin_vector, multiply_coin_with_decimal, CoinOp, CoinVecOp, Operation,
};
use stader_utils::helpers::send_funds_msg;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut, _env: Env, info: MessageInfo, msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = Config {
        manager: info.sender,
        vault_denom: msg.vault_denom,
        pools_contract_addr: msg.pools_contract_addr,
    };
    CONFIG.save(deps.storage, &state)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::AddValidator { val_addr } => add_validator(deps, info, env, val_addr),
        ExecuteMsg::RemoveValidator { val_addr } => remove_validator(deps, info, env, val_addr),
        ExecuteMsg::Stake { val_addr } => stake_to_validator(deps, info, env, val_addr),
        ExecuteMsg::RedeemRewards { validators } => redeem_rewards(deps, info, env, validators),
    }
}

pub fn add_validator(
    deps: DepsMut, info: MessageInfo, _env: Env, val_addr: Addr,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, vec![Verify::SenderManager])?;

    if VALIDATOR_META
        .may_load(deps.storage, &val_addr)
        .unwrap()
        .is_some()
    {
        return Err(ContractError::ValidatorAlreadyExists {});
    }

    // check if the validator exists in the blockchain
    if deps.querier.query_validator(&val_addr).unwrap().is_none() {
        return Err(ContractError::ValidatorNotDiscoverable {});
    }

    VALIDATOR_META.save(
        deps.storage,
        &val_addr,
        &VMeta {
            staked: Uint128::zero(),
            accrued_rewards: vec![],
        },
    )?;

    Ok(Response::default())
}

pub fn remove_validator(
    deps: DepsMut, info: MessageInfo, _env: Env, val_addr: Addr,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

// stake_to_validator can be called for each users message rather than a batch. But this is inaccurate because swap has to be performed
// Batching is the way to go.
pub fn stake_to_validator(
    deps: DepsMut, info: MessageInfo, env: Env, val_addr: Addr,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        vec![Verify::SenderPoolsContract, Verify::NonZeroSingleInfoFund],
    )?;

    let val_meta_opt = VALIDATOR_META.may_load(deps.storage, &val_addr)?;
    if val_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }

    let val_meta = val_meta_opt.unwrap();
    let stake_amount = info.funds[0].clone();

    // TODO: GM - Don't need considering rewards as redeem_rewards will be called before stake.
    // let mut accrued_rewards: Vec<Coin> = vec![];
    // let full_delegation = deps
    //     .querier
    //     .query_delegation(&env.contract.address, &val_addr)?;
    // if full_delegation.is_some() {
    //     accrued_rewards = full_delegation.unwrap().accumulated_rewards
    // }

    VALIDATOR_META.save(
        deps.storage,
        &val_addr,
        &VMeta {
            staked: val_meta
                .staked
                .checked_add(stake_amount.amount.clone())
                .unwrap(),
            accrued_rewards: val_meta.accrued_rewards,
        },
    )?;

    Ok(
        Response::new()
            .add_message(StakingMsg::Delegate {
                validator: val_addr.to_string(),
                amount: stake_amount.clone(),
            })
            .add_attribute("Stake", stake_amount.to_string()), // .add_event(Event::new("rewards", accrued_rewards))
    )
}

pub fn redeem_rewards(
    deps: DepsMut, info: MessageInfo, env: Env, validators: Vec<Addr>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, vec![Verify::SenderPoolsContract])?;

    let mut messages = vec![];
    let mut failed_vals: Vec<String> = vec![];
    for val_addr in validators {
        let val_meta_opt = VALIDATOR_META.may_load(deps.storage, &val_addr)?;
        if val_meta_opt.is_none() {
            failed_vals.push(val_addr.to_string());
            continue;
        }

        let full_delegation_opt = deps
            .querier
            .query_delegation(&env.contract.address, &val_addr)?;
        if full_delegation_opt.is_none() {
            continue;
        }

        VALIDATOR_META.update(deps.storage, &val_addr, |v_meta| -> StdResult<_> {
            let mut val_meta = v_meta.unwrap();
            val_meta.accrued_rewards = merge_coin_vector(
                val_meta.accrued_rewards,
                CoinVecOp {
                    fund: full_delegation_opt.unwrap().accumulated_rewards,
                    operation: Operation::Add,
                },
            );
            Ok(val_meta)
        })?;

        messages.push(DistributionMsg::WithdrawDelegatorReward {
            validator: val_addr.to_string(),
        });
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("failed_validators", failed_vals.join(",")))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<GetConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(GetConfigResponse { config: config })
}
