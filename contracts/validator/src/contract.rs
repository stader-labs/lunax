#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{attr, to_binary, Addr, Attribute, Binary, Coin, ContractResult, Deps, DepsMut, DistributionMsg, Env, Event, MessageInfo, Reply, Response, StakingMsg, StdResult, SubMsg, SubMsgExecutionResponse, Uint128, WasmMsg, CosmosMsg, ReplyOn};

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetConfigResponse, GetStateResponse,
    GetValidatorMetaResponse, InstantiateMsg, MigrateMsg, QueryMsg,
};
use crate::operations::{EVENT_REDELEGATE_ID, EVENT_REDELEGATE_KEY_DST_ADDR, EVENT_REDELEGATE_KEY_SRC_ADDR, EVENT_REDELEGATE_TYPE, MESSAGE_REPLY_REWARD_INST_ID, EVENT_INSTANTIATE_TYPE, EVENT_INSTANTIATE_KEY_CONTRACT_ADDR};
use crate::request_validation::{validate, Verify};
use crate::state::{Config, State, VMeta, CONFIG, STATE, VALIDATOR_REGISTRY};
use cw2::set_contract_version;
use cw20::Cw20ExecuteMsg;
use stader_utils::coin_utils::{
    merge_coin_vector, multiply_coin_with_decimal, CoinVecOp, Operation,
};
use stader_utils::event_constants::{EVENT_KEY_IDENTIFIER, EVENT_SWAP_KEY_AMOUNT, EVENT_SWAP_TYPE};
use stader_utils::helpers::{query_exchange_rates, send_funds_msg};
use terra_cosmwasm::{create_swap_msg, TerraMsgWrapper};

const CONTRACT_NAME: &str = "validator";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

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
        delegator_contract: msg.delegator_contract,
        airdrop_withdraw_contract: msg.airdrop_withdraw_contract,
    };

    let state = State {
        slashing_funds: Uint128::zero(),
    };
    CONFIG.save(deps.storage, &config)?;
    STATE.save(deps.storage, &state)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(
    _deps: DepsMut,
    _env: Env,
    _msg: MigrateMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
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
        ExecuteMsg::SetRewardWithdrawAddress { reward_contract } =>
            set_reward_withdraw_address(deps, info, env, reward_contract),
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
        ExecuteMsg::RedeemAirdropAndTransfer {
            amount,
            claim_msg,
            airdrop_contract,
            cw20_contract
        } => redeem_airdrop_and_transfer(deps, env, info, amount, claim_msg, airdrop_contract, cw20_contract),

        ExecuteMsg::TransferReconciledFunds { amount } => {
            transfer_reconciled_funds(deps, info, env, amount)
        }

        ExecuteMsg::AddSlashingFunds {} => add_slashing_funds(deps, info, env),
        ExecuteMsg::RemoveSlashingFunds { amount } => {
            remove_slashing_funds(deps, info, env, amount)
        }
        ExecuteMsg::UpdateConfig {
            pools_contract,
            delegator_contract,
            airdrop_withdraw_contract
        } => update_config(
            deps,
            info,
            env,
            pools_contract,
            delegator_contract,
            airdrop_withdraw_contract
        ),
    }
}

// TODO - GM. Add tests
pub fn set_reward_withdraw_address(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    reward_contract: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract])?;
    Ok(Response::new().add_message(DistributionMsg::SetWithdrawAddress { address: reward_contract.to_string() }))
}

pub fn add_validator(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract])?;

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

    VALIDATOR_REGISTRY.save(deps.storage, &val_addr, &VMeta { staked: Uint128::zero() })?;

    Ok(Response::default())
}

// Expects the redelegate address to be a validator on the pool
pub fn remove_validator(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
    redelegate_addr: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract])?;

    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
    let redel_val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &redelegate_addr)?;
    if val_meta_opt.is_none() || redel_val_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }
    // If the redelegating validator is jailed, for ex, this tx will fail and will roll back state changes

    let mut src_meta = val_meta_opt.unwrap();
    let mut dst_meta = redel_val_meta_opt.unwrap();
    dst_meta.staked = dst_meta.staked.checked_add(src_meta.staked).unwrap();

    VALIDATOR_REGISTRY.remove(deps.storage, &val_addr);
    VALIDATOR_REGISTRY.save(deps.storage, &redelegate_addr, &dst_meta)?;

    let mut messages = vec![];
    if src_meta.staked.ne(&Uint128::zero()) {
        messages.push(StakingMsg::Redelegate {
            src_validator: val_addr.to_string(),
            dst_validator: redelegate_addr.to_string(),
            amount: Coin::new(src_meta.staked.u128(), config.vault_denom),
        });
    }

    Ok(Response::new().add_messages(messages))
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
        &env,
        vec![Verify::SenderPoolsContract, Verify::NonZeroSingleInfoFund],
    )?;

    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
    if val_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }

    let val_meta = val_meta_opt.unwrap();
    let stake_amount = info.funds[0].clone();

    VALIDATOR_REGISTRY.save(
        deps.storage,
        &val_addr,
        &VMeta {
            staked: val_meta.staked.checked_add(stake_amount.amount).unwrap(),
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
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract])?;

    let mut messages = vec![];
    let mut failed_vals: Vec<String> = vec![];
    let mut logs: Vec<Attribute> = vec![];
    let mut total_slashing_difference = Uint128::zero();
    for val_addr in validators {
        let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;

        // Skip validators that are currently jailed.
        if val_meta_opt.is_none() || deps.querier.query_validator(val_addr.to_string())?.is_none() {
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
// Serves as a mechanism for pool rebalancing and rescuing stake from jailed/tombstoned validators
pub fn redelegate(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    src: Addr,
    dst: Addr,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManagerOrPoolsContract],
    )?;

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

    let mut dst_meta = dst_meta_opt.unwrap();
    dst_meta.staked = dst_meta.staked.checked_add(amount).unwrap();

    VALIDATOR_REGISTRY.save(deps.storage, &src, &src_meta)?;
    VALIDATOR_REGISTRY.save(deps.storage, &dst, &dst_meta)?;

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
        ]))
}

// No need to store undelegated funds here. Pools will store that.
// TODO GM. Why do we need staked in this contract then? For slashing check!
pub fn undelegate(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    val_addr: Addr,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract])?;

    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
    if val_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }
    if amount.is_zero() {
        return Err(ContractError::ZeroAmount {});
    }

    let mut val_meta = val_meta_opt.unwrap();
    if val_meta.staked.lt(&amount) {
        return Err(ContractError::InSufficientFunds {});
    }

    val_meta.staked = val_meta.staked.checked_sub(amount).unwrap();
    VALIDATOR_REGISTRY.save(deps.storage, &val_addr, &val_meta)?;

    Ok(Response::new().add_message(StakingMsg::Undelegate {
        validator: val_addr.to_string(),
        amount: Coin::new(amount.u128(), config.vault_denom),
    }))
}

pub fn redeem_airdrop_and_transfer(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    claim_msg: Binary,
    airdrop_contract: Addr,
    cw20_contract: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract])?;

    if amount.eq(&Uint128::zero()) {
        return Err(ContractError::ZeroAmount {});
    }

    let messages: Vec<WasmMsg> = vec![
        WasmMsg::Execute {
            contract_addr: airdrop_contract.to_string(),
            msg: claim_msg,
            funds: vec![],
        },
        WasmMsg::Execute {
            contract_addr: cw20_contract.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: String::from(config.airdrop_withdraw_contract),
                amount,
            })
            .unwrap(),
            funds: vec![],
        },
    ];

    Ok(Response::new().add_messages(messages))
}

pub fn transfer_reconciled_funds(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract])?;

    if amount.is_zero() {
        return Err(ContractError::ZeroAmount {});
    }

    let vault_denom = config.vault_denom.clone();
    let mut state = STATE.load(deps.storage)?;
    let current_slashing_funds = state.slashing_funds;

    let total_base_funds_in_vault = deps
        .querier
        .query_balance(env.contract.address, vault_denom)
        .unwrap()
        .amount;

    let unaccounted_base_funds = total_base_funds_in_vault
        .checked_sub(current_slashing_funds)
        .unwrap();

    if unaccounted_base_funds.lt(&amount) {
        let slashing_coverage = amount.checked_sub(unaccounted_base_funds).unwrap();
        if state.slashing_funds.lt(&slashing_coverage) {
            return Err(ContractError::NotEnoughSlashingFunds {});
        }
        state.slashing_funds = state.slashing_funds.checked_sub(slashing_coverage).unwrap();
        STATE.save(deps.storage, &state).unwrap();
    }

    Ok(Response::new()
        .add_message(send_funds_msg(
            &config.delegator_contract,
            &[Coin::new(amount.u128(), config.vault_denom)],
        ))
        .add_attribute("slashing_funds", current_slashing_funds.to_string())
        .add_attribute("total_base_funds", total_base_funds_in_vault))
}

pub fn add_slashing_funds(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManager, Verify::NonZeroSingleInfoFund],
    )?;

    let amount = info.funds[0].amount;

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.slashing_funds = state.slashing_funds.checked_add(amount).unwrap();
        Ok(state)
    })?;

    Ok(Response::default())
}

pub fn remove_slashing_funds(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManager, Verify::NoFunds],
    )?;

    if amount.is_zero() {
        return Err(ContractError::ZeroAmount {});
    }

    let caller = info.sender;
    let state = STATE.load(deps.storage)?;
    if amount.gt(&state.slashing_funds) {
        return Err(ContractError::InSufficientFunds {});
    }

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.slashing_funds = state.slashing_funds.checked_sub(amount).unwrap();
        Ok(state)
    })?;

    Ok(Response::new().add_message(send_funds_msg(
        &caller,
        &[Coin::new(amount.u128(), config.vault_denom)],
    )))
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pools_contract: Option<Addr>,
    delegator_contract: Option<Addr>,
    airdrop_withdraw_contract: Option<Addr>
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManager, Verify::NoFunds],
    )?;

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.pools_contract = pools_contract.unwrap_or(config.pools_contract);
        config.airdrop_withdraw_contract = airdrop_withdraw_contract.unwrap_or(config.airdrop_withdraw_contract);
        config.delegator_contract = delegator_contract.unwrap_or(config.delegator_contract);
        Ok(config)
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::ValidatorMeta { val_addr } => to_binary(&query_validator_meta(deps, val_addr)?),
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

pub fn query_validator_meta(deps: Deps, val_addr: Addr) -> StdResult<GetValidatorMetaResponse> {
    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
    Ok(GetValidatorMetaResponse {
        val_meta: val_meta_opt,
    })
}

// #[cfg_attr(not(feature = "library"), entry_point)]
// pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
//     match msg.id {
//         // Called for remove_validator clean up.
//         EVENT_REDELEGATE_ID => reply_remove_validator(deps, env, msg.id, msg.result),
//         // MESSAGE_REPLY_REWARD_INST_ID => reply_reward_contract_inst(deps, env, msg.id, msg.result),
//         _ => panic!("Cannot find operation id {:?}", msg.id),
//     }
// }
//
// // This is called with redelegate response originating from remove_validator.
// pub fn reply_remove_validator(
//     deps: DepsMut,
//     _env: Env,
//     _msg_id: u64,
//     result: ContractResult<SubMsgExecutionResponse>,
// ) -> Result<Response, ContractError> {
//     if result.is_err() {
//         return Err(ContractError::RedelegationFailed {});
//     }
//
//     // TODO - GM. Handle error case as well.
//     let res = result.unwrap();
//     let mut keys: Vec<String> = vec![];
//
//     for event in res.events.clone() {
//         keys.push(event.ty);
//     }
//
//     let event_name = format!("wasm-{}", EVENT_REDELEGATE_TYPE);
//     let event_opt = res.events.into_iter().find(|x| x.ty.eq(&event_name));
//     if event_opt.is_none() {
//         return Err(ContractError::RedelegationEventNotFound {});
//     }
//
//     let attrs = event_opt.unwrap().attributes;
//     let src_attr = attrs
//         .clone()
//         .into_iter()
//         .find(|x| x.key.eq(&EVENT_REDELEGATE_KEY_SRC_ADDR))
//         .unwrap();
//     let dst_attr = attrs
//         .into_iter()
//         .find(|x| x.key.eq(&EVENT_REDELEGATE_KEY_DST_ADDR))
//         .unwrap();
//     let src_val_addr = Addr::unchecked(src_attr.value);
//     let dst_val_addr = Addr::unchecked(dst_attr.value);
//
//     let mut dst_val_meta = VALIDATOR_REGISTRY.load(deps.storage, &dst_val_addr)?;
//
//     VALIDATOR_REGISTRY.save(deps.storage, &dst_val_addr, &dst_val_meta)?;
//     VALIDATOR_REGISTRY.remove(deps.storage, &src_val_addr);
//
//     Ok(Response::new().add_attribute("Removed_val", src_val_addr.to_string()))
// }

// This is called with redelegate response originating from remove_validator.
// pub fn reply_reward_contract_inst(
//     deps: DepsMut,
//     _env: Env,
//     _msg_id: u64,
//     result: ContractResult<SubMsgExecutionResponse>,
// ) -> Result<Response, ContractError> {
//     if result.is_err() {
//         return Err(ContractError::RewardInstantiationFailed {});
//     }
//
//     // TODO - GM. Handle error case as well.
//     let res = result.unwrap();
//     let mut keys: Vec<String> = vec![];
//
//     for event in res.events.clone() {
//         keys.push(event.ty);
//     }
//
//     let event_name = format!("wasm-{}", EVENT_INSTANTIATE_TYPE);
//     let event_opt = res.events.into_iter().find(|x| x.ty.eq(&event_name));
//     if event_opt.is_none() {
//         return Err(ContractError::InstantiateEventNotFound {});
//     }
//
//     let attrs = event_opt.unwrap().attributes;
//     let reward_contract_addr_attr = attrs
//         .clone()
//         .into_iter()
//         .find(|x| x.key.eq(&EVENT_INSTANTIATE_KEY_CONTRACT_ADDR))
//         .unwrap();
//
//     Ok(Response::new().add_attribute("reward_contract_addr", reward_contract_addr_attr.value.to_string()))
// }