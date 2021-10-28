#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use crate::msg::{
    ExecuteMsg, GetConfigResponse, GetStateResponse, InstantiateMsg, QueryMsg, UserPoolResponse,
    UserResponse,
};
use crate::request_validation::{update_user_pointers, validate, Verify};
use crate::state::{
    Config, DepositInfo, PoolPointerInfo, State, UndelegationInfo, UserPoolInfo, CONFIG, STATE,
    USER_REGISTRY,
};
use crate::ContractError;
use cosmwasm_std::{
    attr, to_binary, Addr, Binary, Coin, Decimal, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw_storage_plus::U64Key;
use scc::msg::{ExecuteMsg as SccMsg, UpdateUserAirdropsRequest, UpdateUserRewardsRequest};
use stader_utils::coin_utils::{decimal_division_in_256, multiply_u128_with_decimal, DecCoin};
use stader_utils::helpers::send_funds_msg;
use std::collections::HashMap;
use std::result::Result::Err;
use terra_cosmwasm::TerraMsgWrapper;

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
        protocol_fee_contract: msg.protocol_fee_contract,
    };
    let state = State {
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
        ExecuteMsg::Deposit {
            user_addr,
            pool_id,
            amount,
            pool_rewards_pointer,
            pool_airdrops_pointer,
            pool_slashing_pointer,
        } => deposit(
            deps,
            info,
            env,
            user_addr,
            pool_id,
            amount,
            pool_rewards_pointer,
            pool_airdrops_pointer,
            pool_slashing_pointer,
        ),
        ExecuteMsg::Undelegate {
            user_addr,
            from_pool,
            batch_id,
            amount,
            pool_rewards_pointer,
            pool_airdrops_pointer,
            pool_slashing_pointer,
        } => undelegate(
            deps,
            info,
            env,
            user_addr,
            batch_id,
            from_pool,
            amount,
            pool_rewards_pointer,
            pool_airdrops_pointer,
            pool_slashing_pointer,
        ),
        ExecuteMsg::WithdrawFunds {
            user_addr,
            pool_id,
            undelegate_id, // Undelegate ID is unique to this contract. So we don't need a batch Id for cross check.
            undelegation_batch_slashing_pointer,
            undelegation_batch_unbonding_slashing_ratio,
        } => withdraw_funds(
            deps,
            info,
            env,
            user_addr,
            pool_id,
            undelegate_id,
            undelegation_batch_slashing_pointer,
            undelegation_batch_unbonding_slashing_ratio,
        ),
        ExecuteMsg::AllocateRewards {
            user_addrs,
            pool_pointers,
        } => allocate_rewards_and_airdrops(deps, info, env, user_addrs, pool_pointers),

        ExecuteMsg::UpdateConfig {
            pools_contract,
            scc_contract,
            protocol_fee,
            protocol_fee_contract,
        } => update_config(
            deps,
            info,
            env,
            pools_contract,
            scc_contract,
            protocol_fee_contract,
            protocol_fee,
        ),
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
    pool_airdrops_pointer: Vec<DecCoin>,
    pool_slashing_pointer: Decimal,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderPoolsContract, Verify::NoFunds],
    )?;

    if amount == Uint128::zero() {
        return Err(ContractError::ZeroAmount {});
    }

    USER_REGISTRY.update(
        deps.storage,
        (&user_addr, U64Key::new(pool_id)),
        |user_info_opt| -> StdResult<_> {
            let mut user_info = user_info_opt.unwrap_or_else(|| UserPoolInfo {
                pool_id,
                deposit: DepositInfo {
                    staked: Uint128::zero(),
                },
                airdrops_pointer: pool_airdrops_pointer.clone(),
                pending_airdrops: vec![],
                rewards_pointer: pool_rewards_pointer,
                pending_rewards: Uint128::zero(),
                slashing_pointer: pool_slashing_pointer,
                undelegations: vec![],
            });
            update_user_pointers(
                &mut user_info,
                pool_airdrops_pointer,
                pool_rewards_pointer,
                pool_slashing_pointer,
            );
            user_info.deposit.staked = user_info.deposit.staked.checked_add(amount).unwrap();

            Ok(user_info)
        },
    )?;

    Ok(Response::new().add_attributes(vec![
        attr("deposit_amount", amount.to_string()),
        attr("user_addr", user_addr.to_string()),
        attr("deposit_pool", pool_id.to_string()),
    ]))
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
    pool_airdrops_pointer: Vec<DecCoin>,
    pool_slashing_pointer: Decimal,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderPoolsContract, Verify::NoFunds],
    )?;

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
        return Err(ContractError::InSufficientFunds {});
    }
    let mut state = STATE.load(deps.storage)?;

    update_user_pointers(
        &mut user_meta,
        pool_airdrops_pointer,
        pool_rewards_pointer,
        pool_slashing_pointer,
    );
    user_meta.undelegations.push(UndelegationInfo {
        batch_id,
        id: state.next_undelegation_id,
        amount,
        pool_id: from_pool,
        slashing_pointer: pool_slashing_pointer,
    });
    user_meta.deposit.staked = user_meta.deposit.staked.checked_sub(amount).unwrap();

    USER_REGISTRY.save(deps.storage, map_key, &user_meta)?;

    state.next_undelegation_id += 1;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_attributes(vec![
        attr("undelegate_amount", amount.to_string()),
        attr("from_pool", from_pool.to_string()),
        attr("user_addr", user_addr.to_string()),
    ]))
}

// Don't need to update slashing pointers for the user as this is pure bookkeeping + send money out.
// User deposits do not change.
pub fn withdraw_funds(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    user_addr: Addr,
    pool_id: u64,
    undelegate_id: u64,
    undelegation_batch_slashing_pointer: Decimal,
    undelegation_batch_unbonding_slashing_ratio: Decimal,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderPoolsContract, Verify::NoFunds],
    )?;

    let user_pool_meta_opt =
        USER_REGISTRY.may_load(deps.storage, (&user_addr, U64Key::new(pool_id)))?;
    if user_pool_meta_opt.is_none() {
        return Err(ContractError::UserNotFound {});
    }
    let mut user_pool_meta = user_pool_meta_opt.unwrap();

    let funds = compute_withdrawable_funds(
        &mut user_pool_meta,
        undelegate_id,
        undelegation_batch_slashing_pointer,
        undelegation_batch_unbonding_slashing_ratio,
    )?;

    let others: Vec<UndelegationInfo> = user_pool_meta
        .clone()
        .undelegations
        .into_iter()
        .filter(|x| x.id.ne(&undelegate_id))
        .collect();

    user_pool_meta.undelegations = others;
    USER_REGISTRY.save(
        deps.storage,
        (&user_addr, U64Key::new(pool_id)),
        &user_pool_meta,
    )?;

    let mut msgs = vec![];
    let mut logs = vec![];

    let withdrawable_amount = funds.1;
    let protocol_fee_amount =
        multiply_u128_with_decimal(withdrawable_amount.u128(), config.protocol_fee);

    let user_amount = withdrawable_amount.u128() - protocol_fee_amount;
    if protocol_fee_amount != 0 {
        logs.push(attr("protocol_fee", protocol_fee_amount.to_string()));
        msgs.push(send_funds_msg(
            &config.protocol_fee_contract,
            &[Coin::new(protocol_fee_amount, config.vault_denom.clone())],
        ));
    }
    logs.push(attr("user_withdrawal", user_amount.to_string()));
    if user_amount != 0 {
        msgs.push(send_funds_msg(
            &user_addr,
            &[Coin::new(user_amount, config.vault_denom)],
        ));
    }
    Ok(Response::new().add_messages(msgs).add_attributes(logs))
}

// Provides for (actual_undelegation_amount, actual_withdrawable_amount) as result.
pub fn compute_withdrawable_funds(
    user_pool_meta: &mut UserPoolInfo,
    undelegate_id: u64,
    undelegation_slashing_pointer: Decimal, // Slashing pointer at the time of actual undelegation from pool
    undelegation_batch_slashing_ratio: Decimal, // Slashing ratio indicating slashing during the 21 day period.
) -> Result<(Uint128, Uint128), ContractError> {
    let info_opt = user_pool_meta
        .undelegations
        .clone()
        .into_iter()
        .find(|x| x.id.eq(&undelegate_id));
    if info_opt.is_none() {
        return Err(ContractError::RecordNotFound {});
    }
    let info = info_opt.unwrap();

    // info.slashing_pointer points to the slashing pointer of the pool when user has put in a request for undelegation.
    // Typically info.slashing_pointer >= undelegation_slashing_pointer >= slashing_pointer_for_pool_now

    // Amount that has been undelegated (A) is (info.amount * (new_slashing_pointer)) / (info.slashing_pointer)
    // Amount that can be withdrawn is (A) * batch_unbonding_slashing_ratio

    let mut actual_undelegated_amount = Uint128::new(multiply_u128_with_decimal(
        info.amount.u128(),
        undelegation_slashing_pointer,
    ));
    actual_undelegated_amount = Uint128::new(multiply_u128_with_decimal(
        1_u128,
        decimal_division_in_256(
            Decimal::from_ratio(actual_undelegated_amount.u128(), 1_u128),
            info.slashing_pointer,
        ),
    ));

    let withdrawable_amount = Uint128::new(multiply_u128_with_decimal(
        actual_undelegated_amount.u128(),
        undelegation_batch_slashing_ratio,
    ));
    Ok((actual_undelegated_amount, withdrawable_amount))
}

pub fn allocate_rewards_and_airdrops(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    user_addrs: Vec<Addr>,
    pool_pointers: Vec<PoolPointerInfo>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManager, Verify::NoFunds],
    )?;

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
            update_user_pointers(
                &mut user_pool_info,
                pool_pointer_info.airdrops_pointer,
                pool_pointer_info.rewards_pointer,
                pool_pointer_info.slashing_pointer,
            );
            scc_user_reward_requests.push(UpdateUserRewardsRequest {
                user: user_addr.clone(),
                funds: user_pool_info.pending_rewards,
                strategy_id: None,
            });
            logs.push(attr(
                user_addr.to_string(),
                user_pool_info.pending_rewards.to_string(),
            ));
            scc_user_airdrop_requests.push(UpdateUserAirdropsRequest {
                user: user_addr.clone(),
                pool_airdrops: user_pool_info.pending_airdrops.clone(),
            });
            logs.push(attr(
                user_addr.to_string(),
                user_pool_info
                    .pending_airdrops
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<String>>()
                    .join(","),
            ));
            user_pool_info.pending_airdrops = vec![];
            user_pool_info.pending_rewards = Uint128::zero();
            USER_REGISTRY.save(
                deps.storage,
                (&user_addr, U64Key::new(pool_id)),
                &user_pool_info,
            )?;
        }
    }

    messages.push(WasmMsg::Execute {
        contract_addr: config.scc_contract.to_string(),
        msg: to_binary(&SccMsg::UpdateUserRewards {
            update_user_rewards_requests: scc_user_reward_requests.clone(),
        })
        .unwrap(),
        funds: vec![],
    });
    messages.push(WasmMsg::Execute {
        contract_addr: config.scc_contract.to_string(),
        msg: to_binary(&SccMsg::UpdateUserAirdrops {
            update_user_airdrops_requests: scc_user_airdrop_requests,
        })
        .unwrap(),
        funds: vec![],
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
    protocol_fee: Option<Decimal>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManager, Verify::NoFunds],
    )?;

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.pools_contract = pools_contract.unwrap_or_else(|| config.pools_contract.clone());
        config.scc_contract = scc_contract.unwrap_or_else(|| config.scc_contract.clone());
        config.protocol_fee_contract =
            protocol_fee_contract.unwrap_or_else(|| config.protocol_fee_contract.clone());
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
        QueryMsg::UserPool { user_addr, pool_id } => {
            to_binary(&query_user_pool(deps, user_addr, pool_id)?)
        }
        QueryMsg::User { user_addr } => to_binary(&query_user(deps.storage, user_addr)?),
        QueryMsg::ComputeUndelegationAmounts {
            user_addr,
            pool_id,
            undelegate_id,
            undelegation_slashing_pointer: pool_slashing_pointer,
            batch_slashing_ratio,
        } => to_binary(&query_withdrawable_funds(
            deps,
            user_addr,
            pool_id,
            undelegate_id,
            pool_slashing_pointer,
            batch_slashing_ratio,
        )?),
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
    Ok(UserPoolResponse {
        info: user_pool_opt,
    })
}

pub fn query_user(storage: &dyn Storage, user_addr: Addr) -> StdResult<UserResponse> {
    let mut res = vec![];
    USER_REGISTRY
        .prefix(&user_addr)
        .range(storage, None, None, Order::Ascending)
        .for_each(|x| {
            res.push(x.unwrap().1);
        });
    Ok(UserResponse { info: res })
}

pub fn query_withdrawable_funds(
    deps: Deps,
    user_addr: Addr,
    pool_id: u64,
    undelegate_id: u64,
    pool_slashing_pointer: Decimal,
    batch_slashing_ratio: Decimal,
) -> StdResult<(Uint128, Uint128)> {
    let user_pool_meta_opt = USER_REGISTRY
        .may_load(deps.storage, (&user_addr, U64Key::new(pool_id)))
        .unwrap();
    if user_pool_meta_opt.is_none() {
        return Err(StdError::generic_err("user info not found"));
    }
    let mut user_pool_meta = user_pool_meta_opt.unwrap();
    let x = compute_withdrawable_funds(
        &mut user_pool_meta,
        undelegate_id,
        pool_slashing_pointer,
        batch_slashing_ratio,
    );
    if x.is_err() {
        return Err(StdError::generic_err("Undelegation record not found"));
    }
    Ok(x.unwrap())
}
