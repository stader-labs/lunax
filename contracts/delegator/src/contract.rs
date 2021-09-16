#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use stader_utils::helpers::{query_exchange_rates, send_funds_msg};
use terra_cosmwasm::{create_swap_msg, TerraMsgWrapper};
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, Deps, StdResult, Binary, Addr, Reply, ContractResult, SubMsgExecutionResponse, Uint128, SubMsg, WasmMsg, to_binary, attr, Timestamp, Decimal, Coin, Order};
use crate::msg::{InstantiateMsg, ExecuteMsg, QueryMsg, GetConfigResponse, GetStateResponse, UserPoolResponse, UserResponse, QueryUserInfo};
use crate::ContractError;
use crate::state::{Config, State, CONFIG, STATE, USER_REGISTRY, UserPoolInfo, DepositInfo, RedelegationInfo, UndelegationInfo};
use crate::request_validation::{validate, Verify, update_user_pointers};
use cw_storage_plus::U64Key;
use stader_utils::coin_utils::{decimal_subtraction_in_256, multiply_coin_with_decimal, multiply_u128_with_decimal, DecCoin, merge_dec_coin_vector, DecCoinVecOp, Operation, multiply_deccoin_vector_with_uint128, deccoin_vec_to_coin_vec};

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
    };
    let state = State {
        next_redelegation_id: 1_u64,
        next_undelegation_id: 1_u64,
    };
    // TODO - GM. What happens when initiate function is called twice. Does it create two contracts?
    validate(&config, &info, &env, vec![Verify::NoFunds]);
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
        ExecuteMsg::AllocateRewards { user_addr } => allocate_rewards(deps, info, env, user_addr),
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

    USER_REGISTRY.update(deps.storage, (&user_addr, U64Key::new(pool_id)), |mut user_info_opt| -> StdResult<_> {
        let mut user_info = user_info_opt.unwrap_or_else(|| UserPoolInfo {
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
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    user_addr: Addr,
    batch_id: u64,
    from_pool: u64,
    to_pool: u64,
    amount: Uint128,
    eta: Option<Timestamp>,
    pool_rewards_pointer: Decimal,
    pool_airdrops_pointer: Vec<DecCoin>,
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

    USER_REGISTRY.save(deps.storage, map_key, &user_meta);

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
    let mut info = info_opt.unwrap();
    if info.amount.ne(&amount) {
        return Err(ContractError::NonMatchingAmount {});
    }
    let others = user_pool_meta.undelegations.into_iter().filter(|x| x.id.ne(&undelegate_id)).collect();
    user_pool_meta.undelegations = others;
    USER_REGISTRY.save(deps.storage, (&user_addr, U64Key::new(pool_id)), &user_pool_meta)?;

    Ok(Response::new()
        .add_message(send_funds_msg(&user_addr, &vec![Coin::new(amount.u128(), config.vault_denom)]))
    )
}

pub fn allocate_rewards(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    user_addr: Addr,
)-> Result<Response<TerraMsgWrapper>, ContractError> {
    Err(ContractError::NotImplemented {})
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::UserPool { user_addr, pool_id } => to_binary(&query_user_pool(deps, user_addr, pool_id)?),
        QueryMsg::User { user_addr } => to_binary(&query_user(deps, user_addr)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<GetConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(GetConfigResponse { config: config })
}

pub fn query_state(deps: Deps) -> StdResult<GetStateResponse> {
    let state: State = STATE.load(deps.storage)?;
    Ok(GetStateResponse { state: state })
}

pub fn query_user_pool(deps: Deps, user_addr: Addr, pool_id: u64) -> StdResult<UserPoolResponse> {
    let user_pool_opt = USER_REGISTRY.may_load(deps.storage, (&user_addr, U64Key::new(pool_id)))?;
    Ok(UserPoolResponse { info: user_pool_opt })
}

pub fn query_user(deps: Deps, user_addr: Addr) -> StdResult<UserResponse> {
    let x = USER_REGISTRY.prefix(&user_addr).range(deps.storage, None, None, Order::Ascending).collect();
    let mut res = vec![];
    for y in x {
        let z = y.unwrap();
        let a = String::from_utf8(z.0).unwrap().parse::<u64>().unwrap();
        let b = z.1;
        res.push(QueryUserInfo {
            pool_id: a,
            pool_info: b
        })
    }
    Ok(UserResponse { info: res })
}
