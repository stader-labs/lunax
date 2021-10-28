#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Binary, Coin, Deps, DepsMut, DistributionMsg, Env, MessageInfo,
    QuerierWrapper, Response, StakingMsg, StdResult, SubMsg, Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetConfigResponse, GetValidatorMetaResponse, InstantiateMsg, MigrateMsg, QueryMsg,
};
use crate::request_validation::{validate, Verify};
use crate::state::{Config, CONFIG, VALIDATOR_REGISTRY};
use cw2::set_contract_version;
use cw20::Cw20ExecuteMsg;
use stader_utils::helpers::send_funds_msg;
use terra_cosmwasm::TerraMsgWrapper;

const CONTRACT_NAME: &str = "validator";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
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

    CONFIG.save(deps.storage, &config)?;

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
        ExecuteMsg::SetRewardWithdrawAddress { reward_contract } => {
            set_reward_withdraw_address(deps, info, env, reward_contract)
        }
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
            cw20_contract,
        } => redeem_airdrop_and_transfer(
            deps,
            env,
            info,
            amount,
            claim_msg,
            airdrop_contract,
            cw20_contract,
        ),

        ExecuteMsg::TransferReconciledFunds { amount } => {
            transfer_reconciled_funds(deps, info, env, amount)
        }

        ExecuteMsg::UpdateConfig {
            pools_contract,
            delegator_contract,
            airdrop_withdraw_contract,
        } => update_config(
            deps,
            info,
            env,
            pools_contract,
            delegator_contract,
            airdrop_withdraw_contract,
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
    Ok(
        Response::new().add_message(DistributionMsg::SetWithdrawAddress {
            address: reward_contract.to_string(),
        }),
    )
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

    VALIDATOR_REGISTRY.save(deps.storage, &val_addr, &true)?;

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

    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr.clone())?;
    let redel_val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &redelegate_addr)?;
    if val_meta_opt.is_none() || redel_val_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }
    // If the redelegating validator is jailed, for ex, this tx will fail and will roll back state changes

    let src_val_delegation_opt = deps
        .querier
        .query_delegation(env.contract.address, val_addr.clone())?;
    if src_val_delegation_opt.is_none() {
        return Err(ContractError::ZeroAmount {});
    }

    VALIDATOR_REGISTRY.remove(deps.storage, &val_addr.clone());
    let src_val_delegation_amount = src_val_delegation_opt.unwrap().amount.amount;

    let mut messages = vec![];
    if src_val_delegation_amount.ne(&Uint128::zero()) {
        messages.push(StakingMsg::Redelegate {
            src_validator: val_addr.to_string(),
            dst_validator: redelegate_addr.to_string(),
            amount: Coin::new(src_val_delegation_amount.u128(), config.vault_denom),
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

    let stake_amount = info.funds[0].clone();

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
    for val_addr in validators {
        let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;

        // Skip validators that are currently jailed.
        if val_meta_opt.is_none()
            || deps
                .querier
                .query_validator(val_addr.to_string())?
                .is_none()
        {
            failed_vals.push(val_addr.to_string());
            continue;
        }

        messages.push(SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
            validator: val_addr.to_string(),
        }));
    }

    Ok(Response::new()
        .add_submessages(messages)
        .add_attribute("failed_validators", failed_vals.join(",")))
}

// Make the caller handle errors with redelegation constraints
// Serves as a mechanism for pool rebalancing and rescuing stake from jailed/tombstoned validators
// Used by pools contract
pub fn redelegate(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    src: Addr,
    dst: Addr,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    validate(&config, &info, &env, vec![Verify::SenderPoolsContract])?;

    let src_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &src.clone())?;
    let dst_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &dst.clone())?;
    if src_meta_opt.is_none() || dst_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }

    let src_val_delegation = deps
        .querier
        .query_delegation(env.contract.address, src.clone())?;
    if src_val_delegation.is_none() || src_val_delegation.unwrap().amount.amount.lt(&amount) {
        return Err(ContractError::InSufficientFunds {});
    }

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

    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr.clone())?;
    if val_meta_opt.is_none() {
        return Err(ContractError::ValidatorNotAdded {});
    }
    if amount.is_zero() {
        return Err(ContractError::ZeroAmount {});
    }

    let val_delegation = deps
        .querier
        .query_delegation(env.contract.address, val_addr.clone())?;
    if val_delegation.is_none() || val_delegation.unwrap().amount.amount.lt(&amount) {
        return Err(ContractError::InSufficientFunds {});
    }

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

// This amount is passed in by pools contract after querying this contract's balance.
pub fn transfer_reconciled_funds(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    validate(&config, &info, &env, vec![Verify::SenderPoolsContract])?;

    // Sanity check. Because reconciling funds cannot be 0. Means slashing cannot be 100%
    if amount.is_zero() {
        return Err(ContractError::ZeroAmount {});
    }

    // Unaccounted funds are just the contract balance for now.
    let unaccounted_base_funds = get_unaccounted_base_funds(config.clone(), env, deps.querier)?;

    Ok(Response::new()
        .add_message(send_funds_msg(
            &config.delegator_contract,
            &[Coin::new(amount.u128(), config.vault_denom)],
        ))
        .add_attribute(
            "unaccounted_base_funds",
            unaccounted_base_funds.amount.to_string(),
        ))
}

pub fn get_unaccounted_base_funds(
    config: Config,
    env: Env,
    querier: QuerierWrapper,
) -> StdResult<Coin> {
    let vault_denom = config.vault_denom.clone();

    let total_base_funds_in_vault = querier
        .query_balance(env.contract.address, vault_denom)
        .unwrap()
        .amount;

    Ok(Coin::new(
        total_base_funds_in_vault.u128(),
        config.vault_denom,
    ))
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pools_contract: Option<Addr>,
    delegator_contract: Option<Addr>,
    airdrop_withdraw_contract: Option<Addr>,
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
        config.airdrop_withdraw_contract =
            airdrop_withdraw_contract.unwrap_or(config.airdrop_withdraw_contract);
        config.delegator_contract = delegator_contract.unwrap_or(config.delegator_contract);
        Ok(config)
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::ValidatorMeta { val_addr } => to_binary(&query_validator_meta(deps, val_addr)?),
        QueryMsg::GetUnaccountedBaseFunds {} => {
            to_binary(&query_unaccounted_base_funds(deps, env)?)
        }
    }
}

pub fn query_config(deps: Deps) -> StdResult<GetConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(GetConfigResponse { config })
}

pub fn query_validator_meta(deps: Deps, val_addr: Addr) -> StdResult<GetValidatorMetaResponse> {
    let val_meta_opt = VALIDATOR_REGISTRY.may_load(deps.storage, &val_addr)?;
    Ok(GetValidatorMetaResponse {
        val_meta: val_meta_opt.is_some(),
    })
}

pub fn query_unaccounted_base_funds(deps: Deps, env: Env) -> StdResult<Coin> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(get_unaccounted_base_funds(config, env, deps.querier)?)
}
