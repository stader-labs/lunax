#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Attribute, BankMsg, Binary, Coin, ContractResult, CosmosMsg, Deps,
    DepsMut, DistributionMsg, Env, Event, MessageInfo, Reply, Response, StakingMsg, StdResult,
    SubMsg, SubMsgExecutionResponse, Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, GetConfigResponse, InstantiateMsg, QueryMsg};
use crate::operations::{
    OPERATION_ZERO_DST_ADDR, OPERATION_ZERO_ID, OPERATION_ZERO_SRC_ADDR, OPERATION_ZERO_TAG,
};
use crate::request_validation::{validate, Verify};
use crate::state::{
    AirdropRegistryInfo, Config, State, VMeta, AIRDROP_REGISTRY, CONFIG, STATE, VALIDATOR_REGISTRY,
};
use cw20::Cw20ExecuteMsg;
use stader_utils::coin_utils::{
    decimal_multiplication_in_256, merge_coin, merge_coin_vector, merge_dec_coin_vector,
    multiply_coin_with_decimal, CoinOp, CoinVecOp, DecCoin, DecCoinVecOp, Operation,
};
use stader_utils::helpers::query_exchange_rates;
use terra_cosmwasm::{create_swap_msg, TerraMsgWrapper};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = Config {
        manager: info.sender,
        vault_denom: msg.vault_denom,
        pools_contract_addr: msg.pools_contract_addr,
        scc_contract_addr: msg.scc_contract_addr,
    };
    let state = State {
        airdrops: vec![],
        swapped_amount: Default::default(),
        slashing_funds: Default::default(),
    };
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
        ExecuteMsg::AddValidator { val_addr } => add_validator(deps, info, env, val_addr),
        ExecuteMsg::RemoveValidator {
            val_addr,
            redelegate_addr,
        } => remove_validator(deps, info, env, val_addr, redelegate_addr),
        ExecuteMsg::Stake { val_addr } => stake_to_validator(deps, info, env, val_addr),
        ExecuteMsg::RedeemRewards { validators } => redeem_rewards(deps, info, env, validators),
        ExecuteMsg::Redelegate { src, dst, amount } => {
            redelegate(deps, info, env, src, dst, amount)
        }
        ExecuteMsg::Undelegate { val_addr, amount } => {
            undelegate(deps, info, env, val_addr, amount)
        }
        ExecuteMsg::RedeemAirdrop {
            airdrop_token,
            amount,
            claim_msg,
        } => redeem_airdrop(deps, env, info, airdrop_token, amount, claim_msg),
        ExecuteMsg::Swap { validators } => swap(deps, info, env, validators),

        ExecuteMsg::TransferRewards { amount } => transfer_rewards(deps, info, env, amount),
        ExecuteMsg::TransferAirdrops {} => transfer_airdrops(deps, info, env),

        ExecuteMsg::UpdateAirdropRegistry {
            denom,
            airdrop_contract,
            token_contract,
        } => update_airdrop_registry(deps, info, env, denom, airdrop_contract, token_contract),
        ExecuteMsg::UpdateSlashingFunds { amount } => {
            update_slashing_funds(deps, info, env, amount)
        }
    }
}

pub fn add_validator(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    val_addr: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, vec![Verify::SenderManager])?;

    if VALIDATOR_REGISTRY
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

    VALIDATOR_REGISTRY.save(
        deps.storage,
        &val_addr,
        &VMeta {
            staked: Uint128::zero(),
            accrued_rewards: vec![],
        },
    )?;

    Ok(Response::default())
}

// TODO - GM. Check if this handing off message works. Accrued rewards for validator should be zero.
pub fn remove_validator(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    val_addr: Addr,
    redelegate_addr: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, vec![Verify::SenderManager])?;

    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
    let redel_val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &redelegate_addr)?;
    if val_meta_opt.is_none() || redel_val_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }
    let mut messages = vec![];
    let val_meta = val_meta_opt.unwrap();
    if val_meta.staked.ne(&Uint128::zero()) {
        messages.push(SubMsg::reply_always(
            WasmMsg::Execute {
                contract_addr: _env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::Redelegate {
                    src: val_addr.clone(),
                    dst: redelegate_addr.clone(),
                    amount: val_meta.staked,
                })
                .unwrap(),
                funds: vec![],
            },
            0,
        ));
    }

    Ok(Response::new().add_submessages(messages))
}

// stake_to_validator can be called for each users message rather than a batch.
pub fn stake_to_validator(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        vec![Verify::SenderPoolsContract, Verify::NonZeroSingleInfoFund],
    )?;

    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
    if val_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }

    let val_meta = val_meta_opt.unwrap();
    let stake_amount = info.funds[0].clone();

    let mut accrued_rewards: Vec<Coin> = vec![];
    let full_delegation = deps
        .querier
        .query_delegation(&env.contract.address, &val_addr)?;
    if full_delegation.is_some() {
        accrued_rewards = full_delegation.unwrap().accumulated_rewards
    }

    VALIDATOR_REGISTRY.save(
        deps.storage,
        &val_addr,
        &VMeta {
            staked: val_meta
                .staked
                .checked_add(stake_amount.amount.clone())
                .unwrap(),
            accrued_rewards: merge_coin_vector(
                val_meta.accrued_rewards,
                CoinVecOp {
                    fund: accrued_rewards,
                    operation: Operation::Add,
                },
            ),
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
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    validators: Vec<Addr>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, vec![Verify::SenderPoolsContract])?;

    let mut messages = vec![];
    let mut failed_vals: Vec<String> = vec![];
    let mut logs: Vec<Attribute> = vec![];
    let mut total_slashing_difference = Uint128::zero();
    for val_addr in validators {
        let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
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

        let full_delegation = full_delegation_opt.unwrap();
        let mut val_meta = val_meta_opt.unwrap();

        if full_delegation.amount.amount.lt(&val_meta.staked) {
            let difference = val_meta
                .staked
                .checked_sub(full_delegation.amount.amount)
                .unwrap();
            total_slashing_difference = total_slashing_difference.checked_add(difference).unwrap();
            let slashing_val_str = "slashing-";
            slashing_val_str.to_owned().push_str(&val_addr.to_string());
            logs.push(attr(slashing_val_str, difference.to_string()));
            messages.push(SubMsg::new(StakingMsg::Delegate {
                validator: val_addr.to_string(),
                amount: Coin::new(difference.u128(), config.vault_denom.clone()),
            }));
        }

        val_meta.accrued_rewards = merge_coin_vector(
            val_meta.accrued_rewards,
            CoinVecOp {
                fund: full_delegation.accumulated_rewards,
                operation: Operation::Add,
            },
        );

        VALIDATOR_REGISTRY.save(deps.storage, &val_addr, &val_meta)?;
        // Don't need explicit withdrawal msg for those validators to whom deposits are being added to compensate slasshing.
        messages.push(SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
            validator: val_addr.to_string(),
        }));
    }

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.slashing_funds = state
            .slashing_funds
            .checked_sub(total_slashing_difference)
            .unwrap();
        Ok(state)
    })?;

    Ok(Response::new()
        .add_submessages(messages)
        .add_attributes(logs)
        .add_attribute("failed_validators", failed_vals.join(",")))
}

// Make the caller handle errors with redelegation constraints
// TODO- GM. Make this callable by manager too.
pub fn redelegate(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    src: Addr,
    dst: Addr,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    // Should the sender be manager or this contract for the submessage from remove_validator towards this message?
    validate(&config, &info, vec![Verify::SenderManagerOrPoolsContract])?;

    let src_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &src)?;
    let dst_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &dst)?;
    if src_meta_opt.is_none() || dst_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }

    let mut src_meta = src_meta_opt.unwrap();
    if src_meta.staked.lt(&amount) {
        return Err(ContractError::InSufficientFunds {});
    }
    src_meta.staked = src_meta.staked.checked_sub(amount).unwrap();
    let src_delegation_opt = deps.querier.query_delegation(&env.contract.address, &src)?;
    src_meta.accrued_rewards = merge_coin_vector(
        src_meta.accrued_rewards,
        CoinVecOp {
            operation: Operation::Add,
            fund: if src_delegation_opt.is_none() {
                vec![]
            } else {
                src_delegation_opt.unwrap().accumulated_rewards
            },
        },
    );

    let mut dst_meta = dst_meta_opt.unwrap();
    dst_meta.staked = dst_meta.staked.checked_add(amount).unwrap();
    let dst_delegation_opt = deps.querier.query_delegation(&env.contract.address, &dst)?;
    dst_meta.accrued_rewards = merge_coin_vector(
        dst_meta.accrued_rewards,
        CoinVecOp {
            operation: Operation::Add,
            fund: if dst_delegation_opt.is_none() {
                vec![]
            } else {
                dst_delegation_opt.unwrap().accumulated_rewards
            },
        },
    );

    VALIDATOR_REGISTRY.save(deps.storage, &src, &src_meta)?;
    VALIDATOR_REGISTRY.save(deps.storage, &dst, &dst_meta)?;

    // TODO - GM. Do we need both attr and events?
    Ok(Response::new()
        .add_message(StakingMsg::Redelegate {
            src_validator: src.to_string(),
            dst_validator: dst.to_string(),
            amount: Coin::new(amount.u128(), config.vault_denom),
        })
        .add_attributes(vec![
            attr("Redelgation_src", &src.to_string()),
            attr("Redelgation_dst", &dst.to_string()),
            attr("Redelgation_amount", amount.to_string()),
        ])
        .add_event(
            Event::new(OPERATION_ZERO_TAG)
                .add_attribute(OPERATION_ZERO_SRC_ADDR, &src.to_string())
                .add_attribute(OPERATION_ZERO_DST_ADDR, &dst.to_string()),
        ))
}

// No need to store undelegated funds here. Pools will store that.
// TODO GM. Why do we need staked in this contract then? Sanity? \_(^_^)_/
pub fn undelegate(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, vec![Verify::SenderPoolsContract])?;

    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
    if val_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }

    let mut val_meta = val_meta_opt.unwrap();
    if val_meta.staked.lt(&amount) {
        return Err(ContractError::InSufficientFunds {});
    }

    let full_delegation = deps
        .querier
        .query_delegation(&env.contract.address, &val_addr)?;

    val_meta.staked = val_meta.staked.checked_sub(amount).unwrap();
    val_meta.accrued_rewards = merge_coin_vector(
        val_meta.accrued_rewards,
        CoinVecOp {
            operation: Operation::Add,
            fund: if full_delegation.is_some() {
                full_delegation.unwrap().accumulated_rewards
            } else {
                vec![]
            },
        },
    );

    VALIDATOR_REGISTRY.save(deps.storage, &val_addr, &val_meta)?;

    Ok(Response::new().add_message(StakingMsg::Undelegate {
        validator: val_addr.to_string(),
        amount: Coin::new(amount.u128(), config.vault_denom.to_string()),
    }))
}

pub fn update_airdrop_registry(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    denom: String,
    airdrop_contract: Addr,
    token_contract: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, vec![Verify::SenderManager])?;
    AIRDROP_REGISTRY.save(
        deps.storage,
        denom.clone(),
        &AirdropRegistryInfo {
            airdrop_contract,
            token_contract,
        },
    )?;

    Ok(Response::default())
}

pub fn redeem_airdrop(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    denom: String,
    amount: Uint128,
    claim_msg: Binary,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, vec![Verify::SenderManager])?;

    let airdrop_contract_opt = AIRDROP_REGISTRY
        .may_load(deps.storage, denom.clone())
        .unwrap();
    if airdrop_contract_opt.is_none() {
        return Err(ContractError::AirdropNotRegistered {});
    }

    let airdrop_registry_info = airdrop_contract_opt.unwrap();

    let messages: Vec<WasmMsg> = vec![WasmMsg::Execute {
        contract_addr: airdrop_registry_info.airdrop_contract.to_string(),
        msg: claim_msg,
        funds: vec![],
    }];

    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.airdrops = merge_coin_vector(
            state.airdrops,
            CoinVecOp {
                fund: vec![Coin::new(amount.u128(), denom)],
                operation: Operation::Add,
            },
        );
        Ok(state)
    })?;

    Ok(Response::new().add_messages(messages))
}

// TODO: GM. Add tests for this.
pub fn swap(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    validators: Vec<Addr>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, vec![Verify::SenderPoolsContract])?;

    let mut messages = vec![];
    let mut failed_vals: Vec<String> = vec![];
    let mut total_rewards = vec![];
    for val_addr in validators.clone() {
        let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
        if val_meta_opt.is_none() {
            failed_vals.push(val_addr.to_string());
            continue;
        }

        let val_meta = val_meta_opt.unwrap();
        total_rewards = merge_coin_vector(
            total_rewards,
            CoinVecOp {
                fund: val_meta.accrued_rewards,
                operation: Operation::Add,
            },
        );
    }

    let denoms: Vec<String> = total_rewards
        .iter()
        .map(|item| item.denom.clone())
        .collect();
    let exchange_rates = query_exchange_rates(deps.querier, "uluna".to_string(), &denoms);
    let failed_denoms: Vec<String> = denoms
        .into_iter()
        .filter(|item| !exchange_rates.contains_key(item))
        .collect();
    let mut total_base_denom = 0_u128;
    for coin in total_rewards {
        if exchange_rates.contains_key(&coin.denom) {
            total_base_denom = total_base_denom
                + multiply_coin_with_decimal(&coin, *exchange_rates.get(&coin.denom).unwrap())
                    .amount
                    .u128();
            messages.push(create_swap_msg(coin, config.vault_denom.clone()));
        }
    }

    for val_addr in validators {
        let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
        if val_meta_opt.is_none() {
            failed_vals.push(val_addr.to_string());
            continue;
        }
        let mut val_meta = val_meta_opt.unwrap();
        val_meta.accrued_rewards = val_meta
            .accrued_rewards
            .into_iter()
            .filter(|coin| !exchange_rates.contains_key(&coin.denom))
            .collect();

        VALIDATOR_REGISTRY.save(deps.storage, &val_addr, &val_meta)?;
    }

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.swapped_amount = state
            .swapped_amount
            .checked_add(Uint128::new(total_base_denom))
            .unwrap();
        Ok(state)
    })?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("failed_validators", failed_vals.join(","))
        .add_attribute("failed_denoms", failed_denoms.join(","))
        .add_attribute("swapped", total_base_denom.to_string()))
}

// Sends the swapped luna (called by pool) to SCC.
// Or we could make this be called once for all pools together by manager.
pub fn transfer_rewards(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, vec![Verify::SenderPoolsContract])?;

    // No need to check for amount < swapped_amount because this call will fail.
    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.swapped_amount = state.swapped_amount.checked_sub(amount).unwrap();
        Ok(state)
    })?;

    Ok(Response::new().add_message(BankMsg::Send {
        to_address: config.scc_contract_addr.to_string(),
        amount: vec![Coin::new(amount.u128(), config.vault_denom)],
    }))
}

// Meant to be called by pools contract all pools together.
pub fn transfer_airdrops(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, vec![Verify::SenderPoolsContract])?;

    let state = STATE.load(deps.storage)?;
    let mut failed_airdrops: Vec<String> = vec![];
    let mut failed_denoms: Vec<String> = vec![];
    let mut messages = vec![];
    for airdrop in state.airdrops {
        let airdrop_info_opt = AIRDROP_REGISTRY
            .may_load(deps.storage, airdrop.denom.clone())
            .unwrap();
        if airdrop_info_opt.is_none() {
            failed_airdrops.push(airdrop.to_string());
            failed_denoms.push(airdrop.denom);
            continue;
        }
        let airdrop_info = airdrop_info_opt.unwrap();

        messages.push(WasmMsg::Execute {
            contract_addr: String::from(airdrop_info.token_contract),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: String::from(config.scc_contract_addr.clone()),
                amount: airdrop.amount,
            })
            .unwrap(),
            funds: vec![],
        });
    }

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.airdrops = state
            .airdrops
            .into_iter()
            .filter(|airdrop| failed_denoms.contains(&airdrop.denom.to_string()))
            .collect();
        Ok(state)
    })?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("failed-airdrops", failed_airdrops.join(",")))
}

// Can be used to withdraw / add slashing funds
pub fn update_slashing_funds(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    amount: i64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, vec![Verify::SenderManager])?;

    let state = STATE.load(deps.storage)?;
    if amount < 0 && Uint128::new((-1_i64 * amount) as u128).gt(&state.slashing_funds) {
        return Err(ContractError::InSufficientFunds {});
    }

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.slashing_funds = if amount < 0 {
            state
                .slashing_funds
                .checked_sub(Uint128::new((-1_i64 * amount) as u128))
                .unwrap()
        } else {
            state
                .slashing_funds
                .checked_add(Uint128::new(amount as u128))
                .unwrap()
        };
        Ok(state)
    })?;

    Ok(Response::default())
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

/**
    SubMessage Signals
*/

#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        // Called for remove_validator clean up.
        0 => reply_remove_validator(deps, env, msg.id.into(), msg.result),
        _ => panic!("Cannot find operation id {:?}", msg.id),
    }
}

// This is called with redelegate response originating from remove_validator.
pub fn reply_remove_validator(
    deps: DepsMut,
    _env: Env,
    msg_id: u64,
    result: ContractResult<SubMsgExecutionResponse>,
) -> Result<Response, ContractError> {
    assert_eq!(msg_id, OPERATION_ZERO_ID);
    let event = result
        .unwrap()
        .events
        .into_iter()
        .find(|x| x.ty.eq(&OPERATION_ZERO_TAG))
        .unwrap();
    let attrs = event.attributes;

    let src_attr = attrs
        .clone()
        .into_iter()
        .find(|x| x.key.eq(&OPERATION_ZERO_SRC_ADDR))
        .unwrap();
    let dst_attr = attrs
        .into_iter()
        .find(|x| x.key.eq(&OPERATION_ZERO_DST_ADDR))
        .unwrap();
    let src_val_addr = Addr::unchecked(src_attr.value);
    let dst_val_addr = Addr::unchecked(dst_attr.value);

    let src_val_meta = VALIDATOR_REGISTRY.load(deps.storage, &src_val_addr)?;
    let mut dst_val_meta = VALIDATOR_REGISTRY.load(deps.storage, &dst_val_addr)?;

    // Staked fields would be taken care of redelegate message. Update accrued rewards field as well.
    dst_val_meta.accrued_rewards = merge_coin_vector(
        dst_val_meta.accrued_rewards.clone(),
        CoinVecOp {
            fund: src_val_meta.accrued_rewards.clone(),
            operation: Operation::Add,
        },
    );

    // TODO - GM. Should both of them be performed after message is executed because redel might cause more reward accrual at val_addr side?
    VALIDATOR_REGISTRY.save(deps.storage, &dst_val_addr, &dst_val_meta)?;
    VALIDATOR_REGISTRY.remove(deps.storage, &src_val_addr);

    Ok(Response::new().add_attribute("Removed", src_val_addr.to_string()))
}
