#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128, WasmMsg, Addr, Decimal, CosmosMsg};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, UpdateUserAirdropsRequest, UpdateUserRewardsRequest, StateResponse};
use crate::state::{State, STATE, STRATEGY_INFO_MAP, StrategyInfo, STRATEGY_METADATA_MAP, StrategyMetadata};
use std::borrow::Borrow;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        manager: info.sender.clone(),
        scc_denom: msg.strategy_denom,
        contract_genesis_block_height: _env.block.height,
        contract_genesis_timestamp: _env.block.time,
        total_accumulated_rewards: Uint128::zero(),
        current_rewards_in_scc: Uint128::zero(),
    };
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterStrategy {
            strategy_id,
            unbonding_period,
            sic_contract_address,
            supported_airdrops
        } => try_register_strategy(deps, _env, info, strategy_id, unbonding_period, sic_contract_address, supported_airdrops),
        ExecuteMsg::DeactivateStrategy {
            strategy_id
        } => try_deactivate_strategy(deps, _env, info, strategy_id),
        ExecuteMsg::ActivateStrategy {
            strategy_id
        } => try_activate_strategy(deps, _env, info, strategy_id),
        ExecuteMsg::RemoveStrategy {
            strategy_id
        } => try_remove_strategy(deps, _env, info, strategy_id),
        ExecuteMsg::UpdateUserRewards {
            update_user_rewards_request,
        } => try_update_user_rewards(deps, _env, info, update_user_rewards_request),
        ExecuteMsg::UpdateUserAirdrops {
            update_user_airdrops_request,
        } => try_update_user_airdrops(deps, _env, info, update_user_airdrops_request),
    }
}

pub fn try_register_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_id: String,
    unbonding_period: Option<u64>,
    sic_contract_address: Addr,
    supported_airdrops: Vec<String>
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {})
    }

    match STRATEGY_INFO_MAP.may_load(deps.storage, strategy_id.clone()).unwrap() {
        None => {}
        Some(_) => {
            return Err(ContractError::StrategyInfoAlreadyExists {})
        }
    }

    match STRATEGY_METADATA_MAP.may_load(deps.storage, strategy_id.clone()).unwrap() {
        None => {}
        Some(_) => {
            return Err(ContractError::StrategyMetadataAlreadyExists {})
        }
    }

    STRATEGY_INFO_MAP.save(deps.storage, strategy_id.clone(), &StrategyInfo {
        name: strategy_id.clone().to_string(),
        sic_contract_address,
        unbonding_period,
        supported_airdrops,
        is_active: false
    })?;
    STRATEGY_METADATA_MAP.save(deps.storage, strategy_id.clone(), &StrategyMetadata {
        name: strategy_id.to_string(),
        total_shares: Decimal::zero(),
        global_airdrop_pointer: vec![],
        shares_per_token_ratio: Decimal::zero(),
        current_unprocessed_undelegations: Uint128::zero()
    })?;

    Ok(Response::default())
}

pub fn try_deactivate_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_id: String
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {})
    }

    STRATEGY_INFO_MAP.update(deps.storage, strategy_id, |strategy_info_option| -> Result<_, ContractError> {
        if strategy_info_option.is_none() {
            return Err(ContractError::StrategyInfoDoesNotExist {});
        }

        let mut strategy_info = strategy_info_option.unwrap();
        strategy_info.is_active = false;
        Ok(strategy_info)
    })?;

    Ok(Response::default())
}

pub fn try_activate_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_id: String
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {})
    }

    STRATEGY_INFO_MAP.update(deps.storage, strategy_id, |strategy_info_option| -> Result<_, ContractError> {
        if strategy_info_option.is_none() {
            return Err(ContractError::StrategyInfoDoesNotExist {});
        }

        let mut strategy_info = strategy_info_option.unwrap();
        strategy_info.is_active = false;
        Ok(strategy_info)
    })?;

    Ok(Response::default())
}

pub fn try_remove_strategy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    strategy_id: String
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {})
    }

    STRATEGY_INFO_MAP.remove(deps.storage, strategy_id);

    Ok(Response::default())
}

pub fn try_update_user_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    update_user_rewards_request: Vec<UpdateUserRewardsRequest>,
) -> Result<Response, ContractError> {
    // check for manager?
    let state = STATE.load(deps.storage).unwrap();
    if info.sender != state.manager {
        return Err(ContractError::Unauthorized {})
    }

    let messages: Vec<CosmosMsg<WasmMsg>> = vec![];
    // iterate thru all requests
    for user_request in update_user_rewards_request {
        let user_strategy = user_request.strategy_id;
        let user_amount = user_request.rewards;

        let strategy_info_option = STRATEGY_INFO_MAP.may_load(deps.storage, user_strategy.clone()).unwrap();
        if strategy_info_option.is_none() {
            // TODO: bchain99 - log something out here.
            continue;
        }
        let strategy_info = strategy_info_option.unwrap();

        let strategy_metadata_option = STRATEGY_METADATA_MAP.may_load(deps.storage, user_strategy).unwrap();
        if strategy_metadata_option.is_none() {
            // TODO: bchain99 - log something out here.
            continue;
        }
        let strategy_metadata = strategy_metadata_option.unwrap();

        // fetch the total tokens from the SIC contract and update the S/T ratio for the strategy

        // update user shares based on the S/T ratio

        // update total strategy shares by adding up the user_shares

        // do statewise book-keeping like adding up accumulated_rewards

        // send the rewards to sic
    }

    Ok(Response::default())
}

pub fn try_update_user_airdrops(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    update_user_airdrops_request: Vec<UpdateUserAirdropsRequest>,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetState {} => to_binary(&query_state(deps)?)
    }
}

fn query_state(deps: Deps) -> StdResult<StateResponse> {
    let state = STATE.load(deps.storage)?;
    Ok(StateResponse { state: Option::from(state) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary};

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg { strategy_denom: "uluna".to_string() };
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query_state(deps.as_ref()).unwrap();
        assert_eq!(res.state.unwrap(), State {
            manager: info.sender,
            scc_denom: "uluna".to_string(),
            contract_genesis_block_height: env.block.height,
            contract_genesis_timestamp: env.block.time,
            total_accumulated_rewards: Uint128::zero(),
            current_rewards_in_scc: Uint128::zero()
        });
    }
}
