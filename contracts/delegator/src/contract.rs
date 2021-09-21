#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use stader_utils::helpers::send_funds_msg;
use terra_cosmwasm::TerraMsgWrapper;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, Deps, StdResult, Binary, Addr, Uint128, WasmMsg, to_binary, attr, Timestamp, Decimal, Coin, Order, Storage};
use crate::msg::{InstantiateMsg, ExecuteMsg, QueryMsg, GetConfigResponse, GetStateResponse, UserPoolResponse, UserResponse};
use crate::ContractError;
use crate::state::{Config, State, CONFIG, STATE, USER_REGISTRY, UserPoolInfo, DepositInfo, UndelegationInfo, PoolPointerInfo};
use crate::request_validation::{validate, Verify, update_user_pointers};
use cw_storage_plus::U64Key;
use scc::msg::{ExecuteMsg as SccMsg, UpdateUserRewardsRequest, UpdateUserAirdropsRequest};
use stader_utils::coin_utils::{multiply_u128_with_decimal, DecCoin};
use std::collections::HashMap;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = Config {
        manager: info.sender.clone(),
        vault_denom: msg.vault_denom,
        pools_contract: msg.pools_contract,
        scc_contract: msg.scc_contract,
        protocol_fee: msg.protocol_fee,
        protocol_fee_contract: msg.protocol_fee_contract
    };
    let state = State {
        next_redelegation_id: 1_u64,
        next_undelegation_id: 1_u64,
    };
    // TODO - GM. What happens when initiate function is called twice. Does it create two contracts?
    validate(&config, &info, &env, vec![Verify::NoFunds])?;
    CONFIG.save(deps.storage, &config)?;
    STATE.save(deps.storage, &state)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    match msg {
        ExecuteMsg::Deposit { user_addr, pool_id, amount, pool_rewards_pointer, pool_airdrops_pointer } =>
            deposit(deps, info, env, user_addr, pool_id, amount, pool_rewards_pointer, pool_airdrops_pointer),
        ExecuteMsg::Redelegate { user_addr, batch_id, from_pool, to_pool, amount, eta, pool_rewards_pointer, pool_airdrops_pointer } =>
            redelegate(deps, info, env, user_addr, batch_id, from_pool, to_pool, amount, eta, pool_rewards_pointer, pool_airdrops_pointer),
        ExecuteMsg::Undelegate { user_addr, from_pool, batch_id, amount, pool_rewards_pointer, pool_airdrops_pointer } =>
            undelegate(deps, info, env, user_addr, batch_id, from_pool, amount, pool_rewards_pointer, pool_airdrops_pointer),
        ExecuteMsg::WithdrawFunds { user_addr, pool_id, undelegate_id, amount } =>
            withdraw_funds(deps, info, env, user_addr, pool_id, undelegate_id, amount),
        ExecuteMsg::AllocateRewards { user_addrs, pool_pointers } => allocate_rewards_and_airdrops(deps, info, env, user_addrs, pool_pointers),

        ExecuteMsg::UpdateConfig { pools_contract, scc_contract, protocol_fee, protocol_fee_contract } =>
            update_config(deps, info, env, pools_contract, scc_contract, protocol_fee_contract, protocol_fee),
    }
}

pub fn deposit(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    user_addr: Addr,
    pool_id: u64,
    amount: Uint128,
    pool_rewards_pointer: Decimal,
    pool_airdrops_pointer: Vec<DecCoin>
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract, Verify::NoFunds])?;

    if amount == Uint128::zero() {
        return Err(ContractError::ZeroAmount {});
    }

    USER_REGISTRY.update(deps.storage, (&user_addr, U64Key::new(pool_id)), |user_info_opt| -> StdResult<_> {
        let mut user_info = user_info_opt.unwrap_or_else(|| UserPoolInfo {
            pool_id,
            deposit: DepositInfo { staked: Uint128::zero() },
            airdrops_pointer: pool_airdrops_pointer.clone(),
            pending_airdrops: vec![],
            rewards_pointer: pool_rewards_pointer,
            pending_rewards: Uint128::zero(),
            redelegations: vec![],
            undelegations: vec![],
        });
        update_user_pointers(&mut user_info, pool_airdrops_pointer, pool_rewards_pointer);
        user_info.deposit.staked = user_info.deposit.staked.checked_add(amount).unwrap();

        Ok(user_info)
    })?;

    Ok(Response::new()
        .add_attributes(vec![
            attr("deposit_amount", amount.to_string()),
            attr("user_addr", user_addr.to_string()),
            attr("deposit_pool", pool_id.to_string()),
        ])
    )
}

// TODO - GM. Decrease pending rewards as well and update pointer. Write tests
pub fn redelegate(
    _deps: DepsMut,
    _info: MessageInfo,
    _env: Env,
    _user_addr: Addr,
    _batch_id: u64,
    _from_pool: u64,
    _to_pool: u64,
    _amount: Uint128,
    _eta: Option<Timestamp>,
    _pool_rewards_pointer: Decimal,
    _pool_airdrops_pointer: Vec<DecCoin>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    // let config = CONFIG.load(deps.storage)?;
    // validate(&config, &info, &env, vec![Verify::SenderPoolsContract])?;
    //
    // if amount == Uint128::zero() {
    //     return Err(ContractError::ZeroAmount {});
    // }
    //
    // if from_pool.eq(&to_pool) {
    //     return Err(ContractError::NoOp {});
    // }
    //
    // let mut from_meta = USER_REGISTRY.may_load(deps.storage, (&user_addr, U64Key::new(from_pool)))?.unwrap_or_else(|| UserPoolInfo {
    //     deposit: DepositInfo { staked: Uint128::zero() },
    //     airdrops_pointer: vec![],
    //     pending_airdrops: vec![],
    //     rewards_pointer: Decimal::zero(),
    //     pending_rewards: Uint128::zero(),
    //     redelegations: vec![],
    //     undelegations: vec![]
    // });
    //
    // if from_meta.deposit.staked.lt(&amount) {
    //     return Err(ContractError::InSufficientFunds {})
    // }
    // let mut state = STATE.load(deps.storage)?;
    //
    // update_user_pointers(&mut from_meta, pool_airdrops_pointer, pool_rewards_pointer);
    // from_meta.deposit.staked = from_meta.deposit.staked.checked_sub(amount).unwrap();
    // from_meta.redelegations.push(RedelegationInfo { id: state.next_redelegation_id, batch_id, from_pool, to_pool, amount, eta });
    //
    // USER_REGISTRY.save(deps.storage, (&user_addr, U64Key::new(from_pool)), &from_meta);
    //
    // state.next_redelegation_id = state.next_redelegation_id + 1;
    // STATE.save(deps.storage, &state)?;
    //
    // Ok(Response::new()
    //     .add_attributes(vec![
    //         attr("redelegate_amount", amount.to_string()),
    //         attr("from_pool", from_pool.to_string()),
    //         attr("to_pool", to_pool.to_string()),
    //         attr("user_addr", user_addr.to_string()),
    //         attr("eta", eta.unwrap_or_else(|| Timestamp::from_nanos(0)).to_string())
    //     ])
    // )
    Err(ContractError::NotImplemented {})
}

pub fn undelegate(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    user_addr: Addr,
    batch_id: u64,
    from_pool: u64,
    amount: Uint128,
    pool_rewards_pointer: Decimal,
    pool_airdrops_pointer: Vec<DecCoin>
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract, Verify::NoFunds])?;

    if amount == Uint128::zero() {
        return Err(ContractError::ZeroAmount {});
    }

    let map_key = (&user_addr, U64Key::new(from_pool));
    let user_pool_meta_opt = USER_REGISTRY.may_load(deps.storage, map_key.clone())?;
    if user_pool_meta_opt.is_none() {
        return Err(ContractError::UserNotFound {});
    }
    let mut user_meta = user_pool_meta_opt.unwrap();
    if user_meta.deposit.staked.lt(&amount) {
        return Err(ContractError::InSufficientFunds {})
    }
    let mut state = STATE.load(deps.storage)?;

    update_user_pointers(&mut user_meta, pool_airdrops_pointer, pool_rewards_pointer);
    user_meta.undelegations.push(UndelegationInfo { batch_id, id: state.next_undelegation_id, amount, pool_id: from_pool });
    user_meta.deposit.staked = user_meta.deposit.staked.checked_sub(amount).unwrap();

    USER_REGISTRY.save(deps.storage, map_key, &user_meta)?;

    state.next_undelegation_id = state.next_undelegation_id + 1;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attributes(vec![
            attr("undelegate_amount", amount.to_string()),
            attr("from_pool", from_pool.to_string()),
            attr("user_addr", user_addr.to_string()),
        ])
    )
}

pub fn withdraw_funds(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    user_addr: Addr,
    pool_id: u64,
    undelegate_id: u64,
    amount: Uint128, // Necessary for pools contract bookkeeping.
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract, Verify::NoFunds])?;

    let user_pool_meta_opt = USER_REGISTRY.may_load(deps.storage, (&user_addr, U64Key::new(pool_id)))?;
    if user_pool_meta_opt.is_none() {
        return Err(ContractError::UserNotFound {});
    }
    let mut user_pool_meta = user_pool_meta_opt.unwrap();
    let info_opt = user_pool_meta.undelegations.clone().into_iter().find(|x| x.id.eq(&undelegate_id));
    if info_opt.is_none() {
        return Err(ContractError::RecordNotFound {});
    }
    let info = info_opt.unwrap();
    if info.amount.ne(&amount) {
        return Err(ContractError::NonMatchingAmount {});
    }
    let others = user_pool_meta.undelegations.into_iter().filter(|x| x.id.ne(&undelegate_id)).collect();
    user_pool_meta.undelegations = others;
    USER_REGISTRY.save(deps.storage, (&user_addr, U64Key::new(pool_id)), &user_pool_meta)?;

    let mut msgs = vec![];
    let mut logs = vec![];
    let protocol_fee_amount = multiply_u128_with_decimal(amount.u128(), config.protocol_fee);
    let user_amount = amount.u128() - protocol_fee_amount;
    if protocol_fee_amount != 0 {
        logs.push(attr("protocol_fee", protocol_fee_amount.to_string()));
        msgs.push(send_funds_msg(&config.protocol_fee_contract, &vec![Coin::new(protocol_fee_amount, config.vault_denom.clone())]));
    }
    logs.push(attr("user_withdrawal", user_amount.to_string()));
    msgs.push(send_funds_msg(&user_addr, &vec![Coin::new(user_amount, config.vault_denom)]));
    Ok(Response::new().add_messages(msgs).add_attributes(logs))
}

pub fn allocate_rewards_and_airdrops(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    user_addrs: Vec<Addr>,
    pool_pointers: Vec<PoolPointerInfo>,
)-> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager, Verify::NoFunds])?;

    let mut pool_info_map: HashMap<u64, PoolPointerInfo> = HashMap::new();
    for info in pool_pointers {
        pool_info_map.insert(info.pool_id, info);
    }

    let mut logs = vec![];
    let mut messages = vec![];
    let mut scc_user_reward_requests = vec![];
    let mut scc_user_airdrop_requests = vec![];
    for user_addr in user_addrs {
        let user_pools = query_user(deps.storage, user_addr.clone()).unwrap().info;
        for mut user_pool_info in user_pools {
            let pool_id = user_pool_info.pool_id;
            if !pool_info_map.contains_key(&pool_id) {
                continue;
            }
            let pool_pointer_info = pool_info_map.get(&pool_id).unwrap().clone();
            update_user_pointers(&mut user_pool_info, pool_pointer_info.airdrops_pointer, pool_pointer_info.rewards_pointer);
            scc_user_reward_requests.push(UpdateUserRewardsRequest {
                user: user_addr.clone(),
                funds: user_pool_info.pending_rewards,
                strategy_id: None
            });
            logs.push(attr(user_addr.to_string(), user_pool_info.pending_rewards.to_string()));
            scc_user_airdrop_requests.push(UpdateUserAirdropsRequest {
                user: user_addr.clone(),
                pool_airdrops: user_pool_info.pending_airdrops.clone()
            });
            logs.push(attr(user_addr.to_string(), user_pool_info.pending_airdrops.iter().map(|x| x.to_string()).collect::<Vec<String>>().join(",")));
            user_pool_info.pending_airdrops = vec![];
            user_pool_info.pending_rewards = Uint128::zero();
            USER_REGISTRY.save(deps.storage, (&user_addr, U64Key::new(pool_id)), &user_pool_info)?;
        }
    }

    messages.push(WasmMsg::Execute {
        contract_addr: config.scc_contract.to_string(),
        msg: to_binary(&SccMsg::UpdateUserRewards {
            update_user_rewards_requests: scc_user_reward_requests.clone()
        }).unwrap(),
        funds: vec![]
    });
    messages.push(WasmMsg::Execute {
        contract_addr: config.scc_contract.to_string(),
        msg: to_binary(&SccMsg::UpdateUserAirdrops {
            update_user_airdrops_requests: scc_user_airdrop_requests.clone(),
        }).unwrap(),
        funds: vec![]
    });

    Ok(Response::new().add_messages(messages).add_attributes(logs))
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pools_contract: Option<Addr>,
    scc_contract: Option<Addr>,
    protocol_fee_contract: Option<Addr>,
    protocol_fee: Option<Decimal>
)-> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager, Verify::NoFunds])?;

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.pools_contract = pools_contract.unwrap_or(config.pools_contract.clone());
        config.scc_contract = scc_contract.unwrap_or(config.scc_contract.clone());
        config.protocol_fee_contract = protocol_fee_contract.unwrap_or(config.protocol_fee_contract.clone());
        config.protocol_fee = protocol_fee.unwrap_or(config.protocol_fee);
        Ok(config)
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::UserPool { user_addr, pool_id } => to_binary(&query_user_pool(deps, user_addr, pool_id)?),
        QueryMsg::User { user_addr } => to_binary(&query_user(deps.storage, user_addr)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<GetConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(GetConfigResponse { config })
}

pub fn query_state(deps: Deps) -> StdResult<GetStateResponse> {
    let state: State = STATE.load(deps.storage)?;
    Ok(GetStateResponse { state })
}

pub fn query_user_pool(deps: Deps, user_addr: Addr, pool_id: u64) -> StdResult<UserPoolResponse> {
    let user_pool_opt = USER_REGISTRY.may_load(deps.storage, (&user_addr, U64Key::new(pool_id)))?;
    Ok(UserPoolResponse { info: user_pool_opt })
}

pub fn query_user<'a>(storage: &'a dyn Storage, user_addr: Addr) -> StdResult<UserResponse> {
    let mut res = vec![];
    USER_REGISTRY
        .prefix(&user_addr)
        .range(storage, None, None, Order::Ascending)
        .for_each(|x| {
            res.push(x.unwrap().1);
    });
    Ok(UserResponse { info: res })
}
