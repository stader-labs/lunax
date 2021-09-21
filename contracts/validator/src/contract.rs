#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Attribute, Binary, Coin, ContractResult, Deps, DepsMut, DistributionMsg,
    Env, Event, MessageInfo, Reply, Response, StakingMsg, StdResult, SubMsg,
    SubMsgExecutionResponse, Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetAirdropMetaResponse, GetConfigResponse, GetStateResponse,
    GetValidatorMetaResponse, InstantiateMsg, MigrateMsg, QueryMsg,
};
use crate::operations::{
    EVENT_REDELEGATE_ID, EVENT_REDELEGATE_KEY_DST_ADDR, EVENT_REDELEGATE_KEY_SRC_ADDR,
    EVENT_REDELEGATE_TYPE,
};
use crate::request_validation::{validate, Verify};
use crate::state::{
    AirdropRegistryInfo, Config, State, VMeta, AIRDROP_REGISTRY, CONFIG, STATE, VALIDATOR_REGISTRY,
};
use cw2::set_contract_version;
use cw20::Cw20ExecuteMsg;
use stader_utils::coin_utils::{
    merge_coin_vector, multiply_coin_with_decimal, CoinVecOp, Operation,
};
use stader_utils::helpers::{query_exchange_rates, send_funds_msg};
use terra_cosmwasm::{create_swap_msg, TerraMsgWrapper};
use stader_utils::event_constants::{EVENT_SWAP_TYPE, EVENT_SWAP_KEY_AMOUNT, EVENT_KEY_IDENTIFIER};

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
        scc_contract: msg.scc_contract,
        delegator_contract: msg.delegator_contract,
    };
    validate(&config, &info, &env, vec![Verify::NoFunds])?;
    let state = State {
        slashing_funds: Uint128::zero(),
        unswapped_rewards: vec![],
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
            airdrop_token,
            amount,
            claim_msg,
        } => redeem_airdrop_and_transfer(deps, env, info, airdrop_token, amount, claim_msg),
        ExecuteMsg::SwapAndTransfer {
            validators,
            identifier,
        } => swap_and_transfer(deps, info, env, validators, identifier),

        ExecuteMsg::TransferReconciledFunds { amount } => {
            transfer_reconciled_funds(deps, info, env, amount)
        }

        ExecuteMsg::UpdateAirdropRegistry {
            denom,
            airdrop_contract,
            token_contract,
        } => update_airdrop_registry(deps, info, env, denom, airdrop_contract, token_contract),
        ExecuteMsg::AddSlashingFunds {} => add_slashing_funds(deps, info, env),
        ExecuteMsg::RemoveSlashingFunds { amount } => {
            remove_slashing_funds(deps, info, env, amount)
        }
        ExecuteMsg::UpdateConfig { pools_contract, scc_contract, delegator_contract } => update_config(deps, info, env, pools_contract, scc_contract, delegator_contract),
    }
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
    env: Env,
    val_addr: Addr,
    redelegate_addr: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

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
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::Redelegate {
                    src: val_addr.clone(),
                    dst: redelegate_addr.clone(),
                    amount: val_meta.staked,
                })
                .unwrap(),
                funds: vec![],
            },
            EVENT_REDELEGATE_ID,
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
        &env,
        vec![Verify::SenderPoolsContract, Verify::NonZeroSingleInfoFund],
    )?;

    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
    if val_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }

    let val_meta = val_meta_opt.unwrap();
    let stake_amount = info.funds[0].clone();

    let full_delegation = deps
        .querier
        .query_delegation(&env.contract.address, &val_addr)?;
    let accrued_rewards: Vec<Coin> = if full_delegation.is_some() {
        full_delegation.unwrap().accumulated_rewards
    } else {
        vec![]
    };

    VALIDATOR_REGISTRY.save(
        deps.storage,
        &val_addr,
        &VMeta {
            staked: val_meta
                .staked
                .checked_add(stake_amount.amount.clone())
                .unwrap(),
            accrued_rewards: merge_coin_vector(
                &val_meta.accrued_rewards,
                CoinVecOp {
                    fund: accrued_rewards.clone(),
                    operation: Operation::Add,
                },
            ),
        },
    )?;

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.unswapped_rewards = merge_coin_vector(
            &state.unswapped_rewards,
            CoinVecOp {
                fund: accrued_rewards,
                operation: Operation::Add,
            },
        );
        Ok(state)
    })?;

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
    let mut total_rewards = vec![];
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
        total_rewards = merge_coin_vector(
            &total_rewards,
            CoinVecOp {
                fund: full_delegation.accumulated_rewards.clone(),
                operation: Operation::Add,
            },
        );
        val_meta.accrued_rewards = merge_coin_vector(
            &val_meta.accrued_rewards.clone(),
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
        state.unswapped_rewards = merge_coin_vector(
            &state.unswapped_rewards,
            CoinVecOp {
                fund: total_rewards,
                operation: Operation::Add,
            },
        );
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

    validate(
        &config,
        &info,
        &env,
        vec![Verify::SenderManagerOrPoolsContractOrSelf],
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
    let src_delegation_opt = deps.querier.query_delegation(&env.contract.address, &src)?;
    let src_rewards = if src_delegation_opt.is_none() {
        vec![]
    } else {
        src_delegation_opt.unwrap().accumulated_rewards
    };
    src_meta.accrued_rewards = merge_coin_vector(
        &src_meta.accrued_rewards,
        CoinVecOp {
            operation: Operation::Add,
            fund: src_rewards.clone(),
        },
    );

    let mut dst_meta = dst_meta_opt.unwrap();
    dst_meta.staked = dst_meta.staked.checked_add(amount).unwrap();
    let dst_delegation_opt = deps.querier.query_delegation(&env.contract.address, &dst)?;
    let dst_rewards = if dst_delegation_opt.is_none() {
        vec![]
    } else {
        dst_delegation_opt.unwrap().accumulated_rewards
    };
    dst_meta.accrued_rewards = merge_coin_vector(
        &dst_meta.accrued_rewards,
        CoinVecOp {
            operation: Operation::Add,
            fund: dst_rewards.clone(),
        },
    );

    VALIDATOR_REGISTRY.save(deps.storage, &src, &src_meta)?;
    VALIDATOR_REGISTRY.save(deps.storage, &dst, &dst_meta)?;

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        let total_redeemed_rewards = merge_coin_vector(
            &src_rewards,
            CoinVecOp {
                operation: Operation::Add,
                fund: dst_rewards,
            },
        );
        state.unswapped_rewards = merge_coin_vector(
            &state.unswapped_rewards,
            CoinVecOp {
                operation: Operation::Add,
                fund: total_redeemed_rewards,
            },
        );
        Ok(state)
    })?;

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
            Event::new(EVENT_REDELEGATE_TYPE)
                .add_attribute(EVENT_REDELEGATE_KEY_SRC_ADDR, &src.to_string())
                .add_attribute(EVENT_REDELEGATE_KEY_DST_ADDR, &dst.to_string()),
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
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract])?;

    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
    if val_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }

    let mut val_meta = val_meta_opt.unwrap();
    if val_meta.staked.lt(&amount) {
        return Err(ContractError::InSufficientFunds {});
    }

    let full_delegation_opt = deps
        .querier
        .query_delegation(&env.contract.address, &val_addr)?;

    let acc_rewards = if full_delegation_opt.is_some() {
        full_delegation_opt.unwrap().accumulated_rewards
    } else {
        vec![]
    };
    val_meta.staked = val_meta.staked.checked_sub(amount).unwrap();
    val_meta.accrued_rewards = merge_coin_vector(
        &val_meta.accrued_rewards,
        CoinVecOp {
            operation: Operation::Add,
            fund: acc_rewards.clone(),
        },
    );
    VALIDATOR_REGISTRY.save(deps.storage, &val_addr, &val_meta)?;

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.unswapped_rewards = merge_coin_vector(
            &state.unswapped_rewards.clone(),
            CoinVecOp {
                fund: acc_rewards,
                operation: Operation::Add,
            },
        );
        Ok(state)
    })?;

    Ok(Response::new().add_message(StakingMsg::Undelegate {
        validator: val_addr.to_string(),
        amount: Coin::new(amount.u128(), config.vault_denom.to_string()),
    }))
}

pub fn update_airdrop_registry(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    denom: String,
    airdrop_contract: Addr,
    token_contract: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;
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

pub fn redeem_airdrop_and_transfer(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    denom: String,
    amount: Uint128,
    claim_msg: Binary,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager])?;

    if amount.eq(&Uint128::zero()) {
        return Err(ContractError::ZeroAmount {});
    }

    let airdrop_contract_opt = AIRDROP_REGISTRY
        .may_load(deps.storage, denom.clone())
        .unwrap();
    if airdrop_contract_opt.is_none() {
        return Err(ContractError::AirdropNotRegistered {});
    }

    let airdrop_registry_info = airdrop_contract_opt.unwrap();

    let messages: Vec<WasmMsg> = vec![
        WasmMsg::Execute {
            contract_addr: airdrop_registry_info.airdrop_contract.to_string(),
            msg: claim_msg,
            funds: vec![],
        },
        WasmMsg::Execute {
            contract_addr: airdrop_registry_info.token_contract.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: String::from(config.scc_contract.clone()),
                amount: amount,
            })
            .unwrap(),
            funds: vec![],
        },
    ];

    Ok(Response::new().add_messages(messages))
}

// TODO: GM. Add tests for this.
// Swap & Transfer in one tx ensures that delegators cannot deposit in between and get rewards for epoch.
// Also simplifies one source of luna in validator contract.
// Now there's only undelegated funds, rewards base, (deposits are sent to val in same tx) at any point.
pub fn swap_and_transfer(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    validators: Vec<Addr>,
    identifier: String,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract])?;
    let mut logs: Vec<Attribute> = vec![];
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
            &total_rewards,
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
    logs.push(attr("exhcnage_rate_size", exchange_rates.len().to_string()));
    let mut failed_denoms: Vec<String> = denoms
        .into_iter()
        .filter(|item| !exchange_rates.contains_key(item))
        .collect();
    logs.push(attr("failed_denoms", failed_denoms.join(",")));
    let mut total_transfer_amount = 0_u128;
    let mut rewards_swapped = vec![]; // Coins that will be swapped.

    for coin in total_rewards {
        if coin.amount.eq(&Uint128::zero()) {
            // don't need to be in failed coins or rewards swapped because 0 reward denoms can be cleared out.
            continue;
        } else if coin.denom.eq(&config.vault_denom) {
            rewards_swapped = merge_coin_vector(
                &rewards_swapped,
                CoinVecOp {
                    operation: Operation::Add,
                    fund: vec![Coin::new(coin.amount.u128(), coin.denom.clone())], // Coin does not have copy
                },
            );
            total_transfer_amount = total_transfer_amount + coin.amount.u128();
            logs.push(attr(format!("coin-{}", coin.denom), 1_u128.to_string()));
            logs.push(attr(
                format!("converted-coin-{}", coin.denom),
                coin.amount.to_string(),
            ));
        } else if exchange_rates.contains_key(&coin.denom) {
            let exchange_rate = *exchange_rates.get(&coin.denom).unwrap();
            let coin_converted = multiply_coin_with_decimal(&coin, exchange_rate)
                .amount
                .u128();
            logs.push(attr(
                format!("coin-{}", coin.denom),
                exchange_rate.to_string(),
            ));
            logs.push(attr(
                format!("converted-coin-{}", coin.denom),
                coin_converted.to_string(),
            ));

            if coin_converted == 0_u128 {
                failed_denoms.push(coin.denom.clone());
                continue;
            }
            total_transfer_amount = total_transfer_amount + coin_converted;
            messages.push(create_swap_msg(
                Coin::new(coin.amount.u128(), coin.denom.clone()),
                config.vault_denom.clone(),
            ));

            rewards_swapped = merge_coin_vector(
                &rewards_swapped,
                CoinVecOp {
                    operation: Operation::Add,
                    fund: vec![Coin::new(coin.amount.u128(), coin.denom.clone())], // Coin does not have copy
                },
            );
        }
    }

    if total_transfer_amount == 0_u128 {
        return Ok(Response::new().add_attributes(logs));
    }

    for val_addr in validators {
        let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
        if val_meta_opt.is_none() {
            // Don't add to failed_vals again.
            continue;
        }
        let mut val_meta = val_meta_opt.unwrap();
        val_meta.accrued_rewards = val_meta
            .clone()
            .accrued_rewards
            .into_iter()
            .filter(|coin| failed_denoms.contains(&coin.denom))
            .collect();

        VALIDATOR_REGISTRY.save(deps.storage, &val_addr, &val_meta)?;
    }

    // TODO - GM. This should be a map entry with a pool_id as key.
    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.unswapped_rewards = merge_coin_vector(
            &state.unswapped_rewards,
            CoinVecOp {
                operation: Operation::Sub,
                fund: rewards_swapped,
            },
        );
        Ok(state)
    })?;

    logs.push(attr("failed_denoms", failed_denoms.join(",")));

    Ok(Response::new()
        .add_messages(messages)
        .add_message(send_funds_msg(
            &config.scc_contract,
            &vec![Coin::new(total_transfer_amount, config.vault_denom)],
        ))
        .add_event(
            Event::new(EVENT_SWAP_TYPE)
                .add_attribute(EVENT_SWAP_KEY_AMOUNT, total_transfer_amount.to_string())
                .add_attribute(EVENT_KEY_IDENTIFIER, identifier),
        )
        .add_attributes(logs)
        .add_attribute("failed_validators", failed_vals.join(","))
        .add_attribute("failed_denoms", failed_denoms.join(","))
        .add_attribute("transfer_amount", total_transfer_amount.to_string()))
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
    let base_funds_from_unswapped_rewards = state
        .unswapped_rewards
        .iter()
        .find(|&x| x.denom.eq(&vault_denom))
        .cloned()
        .unwrap_or_else(|| Coin::new(0, vault_denom.clone()))
        .amount;
    let total_base_funds_in_vault = deps
        .querier
        .query_balance(env.contract.address, vault_denom.clone())
        .unwrap()
        .amount;

    let unaccounted_base_funds = total_base_funds_in_vault
        .checked_sub(base_funds_from_unswapped_rewards)
        .unwrap()
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
            &vec![Coin::new(amount.u128(), config.vault_denom)],
        ))
        .add_attribute("slashing_funds", current_slashing_funds.to_string())
        .add_attribute(
            "base_rewards_funds",
            base_funds_from_unswapped_rewards.to_string(),
        )
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
        &vec![Coin::new(amount.u128(), config.vault_denom)],
    )))
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pools_contract: Option<Addr>,
    scc_contract: Option<Addr>,
    delegator_contract: Option<Addr>,
)-> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderManager, Verify::NoFunds])?;

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.pools_contract = pools_contract.unwrap_or(config.pools_contract.clone());
        config.scc_contract = scc_contract.unwrap_or(config.scc_contract.clone());
        config.delegator_contract = delegator_contract.unwrap_or(config.delegator_contract.clone());
        Ok(config)
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::ValidatorMeta { val_addr } => {
            to_binary(&query_validator_meta(deps, val_addr)?)
        }
        QueryMsg::AirdropMeta { token } => to_binary(&query_airdrop_meta(deps, token)?),
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

pub fn query_validator_meta(deps: Deps, val_addr: Addr) -> StdResult<GetValidatorMetaResponse> {
    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
    Ok(GetValidatorMetaResponse {
        val_meta: val_meta_opt,
    })
}

pub fn query_airdrop_meta(deps: Deps, token: String) -> StdResult<GetAirdropMetaResponse> {
    let airdrop_meta_opt = AIRDROP_REGISTRY.may_load(deps.storage, token)?;
    Ok(GetAirdropMetaResponse {
        airdrop_meta: airdrop_meta_opt,
    })
}

/**
    SubMessage Signals
*/

#[cfg_attr(not(feature = "library"), entry_point)]
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
    _msg_id: u64,
    result: ContractResult<SubMsgExecutionResponse>,
) -> Result<Response, ContractError> {
    if result.is_err() {
        return Err(ContractError::RedelegationFailed {});
    }

    // TODO - GM. Handle error case as well.
    let res = result.unwrap();
    let mut keys: Vec<String> = vec![];

    for event in res.events.clone() {
        keys.push(event.ty);
    }

    let event_name = format!("wasm-{}", EVENT_REDELEGATE_TYPE);
    let event_opt = res
        .events
        .clone()
        .into_iter()
        .find(|x| x.ty.eq(&event_name));
    if event_opt.is_none() {
        return Err(ContractError::RedelegationEventNotFound {});
    }

    let attrs = event_opt.unwrap().attributes;
    let src_attr = attrs
        .clone()
        .into_iter()
        .find(|x| x.key.eq(&EVENT_REDELEGATE_KEY_SRC_ADDR))
        .unwrap();
    let dst_attr = attrs
        .into_iter()
        .find(|x| x.key.eq(&EVENT_REDELEGATE_KEY_DST_ADDR))
        .unwrap();
    let src_val_addr = Addr::unchecked(src_attr.value);
    let dst_val_addr = Addr::unchecked(dst_attr.value);

    let src_val_meta = VALIDATOR_REGISTRY.load(deps.storage, &src_val_addr)?;
    let mut dst_val_meta = VALIDATOR_REGISTRY.load(deps.storage, &dst_val_addr)?;

    // Staked fields would be taken care of redelegate message. Update accrued rewards field as well.
    dst_val_meta.accrued_rewards = merge_coin_vector(
        &dst_val_meta.accrued_rewards.clone(),
        CoinVecOp {
            fund: src_val_meta.accrued_rewards.clone(),
            operation: Operation::Add,
        },
    );

    VALIDATOR_REGISTRY.save(deps.storage, &dst_val_addr, &dst_val_meta)?;
    VALIDATOR_REGISTRY.remove(deps.storage, &src_val_addr);

    Ok(Response::new().add_attribute("Removed_val", src_val_addr.to_string()))
}
