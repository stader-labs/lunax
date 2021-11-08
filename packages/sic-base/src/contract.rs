#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
    Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetFulfillableUndelegatedFundsResponse, GetStateResponse, GetTotalTokensResponse,
    InstantiateMsg, MerkleAirdropMsg, QueryMsg,
};
use crate::state::{State, STATE};
use std::cmp::min;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        manager: info.sender,
        scc_address: msg.scc_address,
        strategy_denom: msg.strategy_denom,
        contract_genesis_block_height: _env.block.height,
        contract_genesis_timestamp: _env.block.time,
        total_rewards_accumulated: Uint128::zero(),
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
        ExecuteMsg::TransferRewards {} => transfer_rewards(deps, _env, info),
        ExecuteMsg::UndelegateRewards { amount } => undelegate_rewards(deps, _env, info, amount),
        ExecuteMsg::TransferUndelegatedRewards { amount } => {
            transfer_undelegated_rewards(deps, _env, info, amount)
        }
        ExecuteMsg::ClaimAirdrops {
            airdrop_token_contract,
            airdrop_token,
            amount,
            stage,
            proof,
        } => claim_airdrops(
            deps,
            _env,
            info,
            airdrop_token_contract,
            airdrop_token,
            amount,
            stage,
            proof,
        ),
        ExecuteMsg::TransferAirdropsToScc {
            cw20_token_contract,
            airdrop_token,
            amount,
        } => transfer_airdrops_to_scc(deps, _env, info, cw20_token_contract, airdrop_token, amount),
    }
}

pub fn transfer_airdrops_to_scc(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    cw20_token_contract: String,
    _airdrop_token: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.scc_address {
        return Err(ContractError::Unauthorized {});
    }

    let cw20_token_contract = deps.api.addr_validate(cw20_token_contract.as_str())?;

    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: cw20_token_contract.to_string(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
            recipient: state.scc_address.to_string(),
            amount,
        })
        .unwrap(),
        funds: vec![],
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn claim_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    airdrop_token_contract: String,
    _airdrop_token: String,
    amount: Uint128,
    stage: u8,
    proof: Vec<String>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.scc_address {
        return Err(ContractError::Unauthorized {});
    }

    let airdrop_token_contract = deps.api.addr_validate(airdrop_token_contract.as_str())?;
    // this wasm-msg will transfer the airdrops from the merkle airdrop cw20 token contract to the
    // SIC contract
    Ok(Response::new().add_message(WasmMsg::Execute {
        contract_addr: airdrop_token_contract.to_string(),
        msg: to_binary(&MerkleAirdropMsg::Claim {
            stage,
            amount,
            proof,
        })?,
        funds: vec![],
    }))
}

pub fn transfer_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // transfer rewards should only be called by the SCC
    if info.sender != state.scc_address {
        return Err(ContractError::Unauthorized {});
    }

    if info.funds.is_empty() {
        return Err(ContractError::NoFundsSent {});
    }

    if info.funds.len() > 1 {
        return Err(ContractError::MultipleCoinsSent {});
    }

    let coin_sent = info.funds.get(0).unwrap();

    if coin_sent.denom.ne(&state.strategy_denom) {
        return Err(ContractError::WrongDenom(coin_sent.denom.clone()));
    }

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.total_rewards_accumulated = state
            .total_rewards_accumulated
            .checked_add(coin_sent.amount)
            .unwrap();
        Ok(state)
    })?;

    Ok(Response::default())
}

// SCC needs to call this when it processes the undelegations.
pub fn undelegate_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _amount: Uint128,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // undelegate_rewards should only be called by the SCC
    if info.sender != state.scc_address {
        return Err(ContractError::Unauthorized {});
    }

    /*
       Undelegation amount sent to undelegate_rewards to SIC is a batched amount from the SCC. SCC batches
       up all the user reward undelegations and sends it to the SIC.

       The main intent of this message is to handle cases where transferring the rewards from source(the strategy)
       to destination takes time and has complexities on the way(yes, I am speaking about unbonding slashing).

       If there are no issues for transferring the rewards from the source to the destination then the strategist
       can choose to leave this as a no-op and directly call transfer.
    */

    Ok(Response::default())
}

pub fn transfer_undelegated_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.scc_address {
        return Err(ContractError::Unauthorized {});
    }

    if amount.is_zero() {
        return Err(ContractError::ZeroWithdrawal {});
    }

    if state.total_rewards_accumulated.lt(&amount) {
        return Err(ContractError::InSufficientFunds {});
    }

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.total_rewards_accumulated =
            state.total_rewards_accumulated.checked_sub(amount).unwrap();
        Ok(state)
    })?;

    // undelegation_batch_id is ignored here as we are not batching anything up
    Ok(Response::new().add_message(BankMsg::Send {
        to_address: state.scc_address.to_string(),
        amount: vec![Coin::new(amount.u128(), state.strategy_denom)],
    }))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetTotalTokens {} => to_binary(&query_total_tokens(deps, _env)?),
        QueryMsg::GetFulfillableUndelegatedFunds { amount } => {
            to_binary(&query_fulfillable_undelegated_funds(deps, amount)?)
        }
        QueryMsg::GetState {} => to_binary(&query_state(deps)?),
    }
}

fn query_state(deps: Deps) -> StdResult<GetStateResponse> {
    let state = STATE.may_load(deps.storage).unwrap();

    Ok(GetStateResponse { state })
}

fn query_fulfillable_undelegated_funds(
    deps: Deps,
    amount: Uint128,
) -> StdResult<GetFulfillableUndelegatedFundsResponse> {
    let state = STATE.load(deps.storage)?;

    Ok(GetFulfillableUndelegatedFundsResponse {
        undelegated_funds: Some(min(state.total_rewards_accumulated, amount)),
    })
}

fn query_total_tokens(deps: Deps, _env: Env) -> StdResult<GetTotalTokensResponse> {
    let state = STATE.load(deps.storage).unwrap();
    Ok(GetTotalTokensResponse {
        total_tokens: Some(state.total_rewards_accumulated),
    })
}
