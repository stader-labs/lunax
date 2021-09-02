#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Attribute, BankMsg, Binary, Coin, Decimal, Deps, DepsMut,
    DistributionMsg, Env, Fraction, MessageInfo, Order, Response, StakingMsg, StdResult, Uint128,
    WasmMsg,
};

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetCurrentUndelegationBatchIdResponse, GetStateResponse, GetTotalTokensResponse,
    GetUndelegationBatchInfoResponse, InstantiateMsg, QueryMsg,
};
use crate::state::{BatchUndelegationRecord, State, STATE, UNDELEGATION_INFO_LEDGER};
use cw_storage_plus::U64Key;
use std::collections::HashMap;
use std::ops::Add;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let mut state = State {
        manager: info.sender.clone(),
        scc_address: msg.scc_address,
        vault_denom: msg.strategy_denom,
        contract_genesis_block_height: _env.block.height,
        contract_genesis_timestamp: _env.block.time,
        current_undelegation_batch_id: 0,
        unbonding_period: msg
            .unbonding_period
            .unwrap_or_else(|| (21 * 24 * 3600 + 3600)),
        total_rewards_accumulated: Uint128::zero(),
        total_rewards_in_sic: Uint128::zero(),
        rewards_in_yield: Uint128::zero(),
    };

    STATE.save(deps.storage, &state)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::TransferRewards {} => try_transfer_rewards(deps, _env, info),
        ExecuteMsg::UndelegateRewards { amount } => {
            try_undelegate_rewards(deps, _env, info, amount)
        }
        ExecuteMsg::WithdrawRewards {
            user,
            undelegation_batch_id,
            amount,
        } => try_withdraw_rewards(deps, _env, info, user, undelegation_batch_id, amount),
        ExecuteMsg::ClaimAirdrops {
            airdrop_token_contract,
            airdrop_token,
            amount,
            claim_msg,
        } => try_claim_airdrops(
            deps,
            _env,
            info,
            airdrop_token_contract,
            airdrop_token,
            amount,
            claim_msg,
        ),
        ExecuteMsg::WithdrawAirdrops {
            airdrop_token_contract,
            airdrop_token,
            amount,
            user,
        } => try_withdraw_airdrops(
            deps,
            _env,
            info,
            airdrop_token_contract,
            airdrop_token,
            amount,
            user,
        ),
    }
}

// TODO: bchain99 - implement a very basic SIC contract which just holds some funds
pub fn try_claim_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    airdrop_token_contract: Addr,
    airdrop_token: String,
    amount: Uint128,
    claim_msg: Binary,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

pub fn try_withdraw_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    airdrop_token_contract: Addr,
    airdrop_token: String,
    amount: Uint128,
    user: Addr,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

pub fn try_transfer_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

// SCC needs to call this when it processes the undelegations.
pub fn try_undelegate_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

pub fn try_withdraw_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    user: Addr,
    undelegation_batch_id: u64,
    amount: Uint128,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

pub fn try_reinvest(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetTotalTokens {} => to_binary(&query_total_tokens(deps, _env)?),
        QueryMsg::GetCurrentUndelegationBatchId {} => {
            to_binary(&query_current_undelegation_batch_id(deps, _env)?)
        }
        QueryMsg::GetState {} => to_binary(&query_state(deps)?),
        QueryMsg::GetUndelegationBatchInfo {
            undelegation_batch_id,
        } => to_binary(&query_undelegation_batch_info(deps, undelegation_batch_id)?),
    }
}

fn query_state(deps: Deps) -> StdResult<GetStateResponse> {
    let state = STATE.may_load(deps.storage).unwrap();

    Ok(GetStateResponse { state })
}

fn query_total_tokens(deps: Deps, _env: Env) -> StdResult<GetTotalTokensResponse> {
    let state = STATE.load(deps.storage).unwrap();
    Ok(GetTotalTokensResponse {
        total_tokens: Option::from(state.rewards_in_yield),
    })
}

fn query_current_undelegation_batch_id(
    deps: Deps,
    _env: Env,
) -> StdResult<GetCurrentUndelegationBatchIdResponse> {
    let state = STATE.load(deps.storage).unwrap();

    Ok(GetCurrentUndelegationBatchIdResponse {
        current_undelegation_batch_id: state.current_undelegation_batch_id,
    })
}

fn query_undelegation_batch_info(
    deps: Deps,
    undelegation_batch_id: u64,
) -> StdResult<GetUndelegationBatchInfoResponse> {
    let undelegation_batch_info = UNDELEGATION_INFO_LEDGER
        .may_load(deps.storage, U64Key::new(undelegation_batch_id))
        .unwrap();

    Ok(GetUndelegationBatchInfoResponse {
        undelegation_batch_info,
    })
}
