use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, ValidatorAprResponse};
use crate::state::{Config, State, ValidatorMetrics, CONFIG, METRICS_HISTORY, STATE};
use crate::util::{
    clamp, compute_apr, decimal_multiplication_in_256, decimal_summation_in_256, uint128_to_decimal,
};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Order, Response, StakingMsg,
    StdError, StdResult, Storage,
};
use cosmwasm_std::{Decimal, FullDelegation};
use cw_storage_plus::{Bound, U64Key};
use std::collections::HashMap;
use std::convert::TryInto;
use std::ops::Add;
use terra_cosmwasm::TerraQuerier;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        vault_denom: msg.vault_denom.clone(),
        validators: vec![],
        cron_timestamps: vec![],
        validator_index_for_next_cron: 0,
    };
    STATE.save(deps.storage, &state)?;

    let config = Config {
        manager: info.sender.clone(),
        amount_to_stake_per_validator: msg.amount_to_stake_per_validator,
        batch_size: msg.batch_size,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("manager", info.sender)
        .add_attribute("time", _env.block.time.seconds().to_string())
        .add_attribute(
            "amount_to_stake_per_validator",
            msg.amount_to_stake_per_validator,
        )
        .add_attribute("vault_denom", msg.vault_denom.clone().to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RecordMetrics { timestamp } => {
            record_validator_metrics(deps, env, info, timestamp)
        }
        ExecuteMsg::AddValidator { addr } => add_validator(deps, info, addr),
        ExecuteMsg::UpdateConfig { batch_size } => update_config(deps, info, batch_size),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetState {} => to_binary(&query_state(deps)?),
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::GetAllTimestamps {} => to_binary(&query_timestamps(deps)?),
        QueryMsg::GetAllAprsByInterval {
            timestamp1,
            timestamp2,
            from,
            to,
        } => to_binary(&query_validators_aprs_by_interval(
            deps, timestamp1, timestamp2, from, to,
        )?),
        QueryMsg::GetAprByValidator {
            timestamp1,
            timestamp2,
            addr,
        } => to_binary(&query_validator_apr(deps, timestamp1, timestamp2, addr)?),
        QueryMsg::GetAllValidatorMetrics { addr } => {
            to_binary(&query_all_validator_metrics(deps, addr)?)
        }
        QueryMsg::GetValidatorMetricsByTimestamp { addr, timestamp } => to_binary(
            &query_validator_metrics_by_timestamp(deps, addr, timestamp)?,
        ),
        QueryMsg::GetValidatorsMetricsByTimestamp {
            timestamp,
            from,
            to,
        } => to_binary(&query_validators_metrics_by_timestamp(
            deps, timestamp, from, to,
        )?),
        QueryMsg::GetValidatorMetricsBtwTimestamps {
            addr,
            timestamp1,
            timestamp2,
        } => to_binary(&query_all_validator_metrics_btw_timestamps(
            deps, addr, timestamp1, timestamp2,
        )?),
    }
}

fn query_timestamps(deps: Deps) -> StdResult<Vec<u64>> {
    Ok(STATE.load(deps.storage)?.cron_timestamps)
}

fn query_validator_apr(
    deps: Deps,
    timestamp1: u64,
    timestamp2: u64,
    addr: Addr,
) -> StdResult<ValidatorAprResponse> {
    if timestamp1.ge(&timestamp2) {
        return Err(StdError::GenericErr {
            msg: "timestamp1 cannot be greater than or equal to timestamp2".to_string(),
        });
    }

    let h1 = METRICS_HISTORY.load(deps.storage, (&addr, U64Key::new(timestamp1)))?;

    let h2 = METRICS_HISTORY.load(deps.storage, (&addr, U64Key::new(timestamp2)))?;

    return Ok(ValidatorAprResponse {
        addr,
        apr: compute_apr(&h1, &h2, timestamp2 - timestamp1),
    });
}

fn query_validators_aprs_by_interval(
    deps: Deps,
    timestamp1: u64,
    timestamp2: u64,
    from: u32,
    to: u32,
) -> StdResult<Vec<ValidatorAprResponse>> {
    if timestamp1.ge(&timestamp2) {
        return Err(StdError::GenericErr {
            msg: "timestamp1 cannot be greater than or equal to timestamp2".to_string(),
        });
    }
    let validators = STATE.load(deps.storage)?.validators;

    let total_validators: u32 = validators.len().try_into().unwrap();

    if to.ge(&total_validators) || from > to {
        return Err(StdError::GenericErr {
            msg: "Invalid indexes!".to_string(),
        });
    }

    let t1 = U64Key::new(timestamp1);
    let t2 = U64Key::new(timestamp2);

    let mut response: Vec<ValidatorAprResponse> = vec![];
    let mut start = from;

    while start.le(&to) {
        let validator_addr = &validators[start as usize];
        let h1_opt = METRICS_HISTORY.may_load(deps.storage, (&validator_addr, t1.clone()));
        let h2_opt = METRICS_HISTORY.may_load(deps.storage, (&validator_addr, t2.clone()));
        if let (Ok(Some(h1)), Ok(Some(h2))) = (h1_opt, h2_opt) {
            let apr = compute_apr(&h1, &h2, timestamp2 - timestamp1);
            response.push(ValidatorAprResponse { addr: h2.addr, apr });
        };
        start = start + 1;
    }

    Ok(response)
}

fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    batch_size: u32,
) -> Result<Response, ContractError> {
    if batch_size == 0 {
        return Err(ContractError::BatchSizeCannotBeZero {});
    }

    let manager = CONFIG.load(deps.storage)?.manager;
    // can only be updated by manager
    if info.sender != manager {
        return Err(ContractError::Unauthorized {});
    }

    CONFIG.update(deps.storage, |mut conf| -> StdResult<_> {
        conf.batch_size = batch_size;
        Ok(conf)
    })?;

    Ok(Response::new().add_attribute("method", "update_config"))
}

fn add_validator(
    deps: DepsMut,
    info: MessageInfo,
    validator_addr: Addr,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    let vault_denom = state.vault_denom;
    let amount_to_stake_per_validator = config.amount_to_stake_per_validator;

    // can only be called by manager
    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    // check if the validator exists in the blockchain
    if deps
        .querier
        .query_validator(validator_addr.clone())?
        .is_none()
    {
        return Err(ContractError::ValidatorDoesNotExist {});
    }

    // Validator should not be already recorded
    if state.validators.iter().any(|addr| addr.eq(&validator_addr)) {
        return Err(ContractError::ValidatorAlreadyExists {});
    }

    let funds = info.funds.first();
    if funds.is_none() {
        return Err(ContractError::NoFundsFound {});
    }

    if funds.unwrap().amount.lt(&amount_to_stake_per_validator) {
        return Err(ContractError::InsufficientFunds {});
    }

    let msg = StakingMsg::Delegate {
        validator: validator_addr.to_string(),
        amount: Coin {
            denom: vault_denom.clone(),
            amount: amount_to_stake_per_validator,
        },
    };

    STATE.update(deps.storage, |mut s| -> StdResult<_> {
        s.validators.push(validator_addr.clone());
        Ok(s)
    })?;

    Ok(Response::new()
        .add_messages(vec![msg])
        .add_attribute("method", "add_validator"))
}

pub fn record_validator_metrics(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    timestamp: u64,
) -> Result<Response, ContractError> {
    let manager = CONFIG.load(deps.storage)?.manager;
    // can only be called by manager
    if info.sender != manager {
        return Err(ContractError::Unauthorized {});
    }

    // [10]
    let validators_to_record = get_validators_to_record(deps.storage, timestamp)?;

    if validators_to_record.is_empty() {
        return Ok(Response::new()
            .add_attribute("method", "record_validator_metrics")
            .add_attribute("msg", "All validators are recorded for the given cron time"));
    }

    let current_validators_metrics =
        compute_current_metrics(&deps, env, &validators_to_record, timestamp)?;

    let t = U64Key::new(timestamp);
    for metric in current_validators_metrics {
        METRICS_HISTORY.save(deps.storage, (&metric.addr, t.clone()), &metric)?;
    }

    Ok(Response::new()
        .add_attribute("method", "record_validator_metrics")
        .add_attribute(
            "msg",
            format!(
                "Updated {} validators for the given time",
                validators_to_record.len()
            ),
        ))
}

fn compute_current_metrics(
    deps: &DepsMut,
    env: Env,
    validators: &Vec<Addr>,
    timestamp: u64,
) -> Result<Vec<ValidatorMetrics>, ContractError> {
    let state = STATE.load(deps.storage)?;
    let vault_denom = state.vault_denom;

    let mut exchange_rates_map: HashMap<String, Decimal> = HashMap::new();
    exchange_rates_map.insert(vault_denom.clone(), Decimal::one());
    let querier = TerraQuerier::new(&deps.querier);

    let mut current_metrics: Vec<ValidatorMetrics> = vec![];

    for validator_addr in validators {
        let delegation_opt = deps
            .querier
            .query_delegation(&env.contract.address, validator_addr)?;

        if delegation_opt.is_none() {
            return Err(ContractError::NoDelegationFound {
                manager: env.contract.address.clone(),
                validator: validator_addr.clone(),
            });
        }

        let validator = deps.querier.query_validator(validator_addr)?.unwrap();
        let delegation = delegation_opt.unwrap();
        let current_rewards = get_total_rewards_in_vault_denom(
            &delegation,
            &vault_denom,
            &mut exchange_rates_map,
            &querier,
        );

        // This is the new Delegated amount after slashing Ex: (10 => 9.8 etc.,)
        let current_delegated_amount = delegation.amount.amount.clone();

        current_metrics.push(ValidatorMetrics {
            addr: validator_addr.clone(),
            rewards: current_rewards,
            delegated_amount: current_delegated_amount,
            commission: validator.commission,
            max_commission: validator.max_commission,
            timestamp,
        });
    }
    Ok(current_metrics)
}

fn get_total_rewards_in_vault_denom(
    delegation: &FullDelegation,
    vault_denom: &String,
    exchange_rates_map: &mut HashMap<String, Decimal>,
    querier: &TerraQuerier,
) -> Decimal {
    let accumulated_rewards = &delegation.accumulated_rewards;
    let mut current_rewards: Decimal = Decimal::zero();
    for coin in accumulated_rewards {
        // Tries to find the exchange rate in the hashmap,
        // If not present we fetch the exchange rate and add it to the map before calculating reward
        let reward_for_coin =
            get_amount_in_vault_denom(coin, vault_denom, exchange_rates_map, querier);
        if reward_for_coin.is_some() {
            current_rewards = decimal_summation_in_256(reward_for_coin.unwrap(), current_rewards);
        } // If exchange rate is not fetchable then we skip such reward ?
    }
    current_rewards
}

fn get_validators_to_record(
    storage: &mut dyn Storage,
    timestamp: u64,
) -> Result<Vec<Addr>, ContractError> {
    let batch_size = CONFIG.load(storage)?.batch_size;
    let state = STATE.load(storage)?;
    let last_cron_time_opt = state.cron_timestamps.last();
    let validators = state.validators;
    let total_validators: u32 = validators.len().try_into().unwrap();
    let mut validator_index_for_next_cron = state.validator_index_for_next_cron;

    // If the Cron time is completely New (Update State)
    if last_cron_time_opt.is_none() || !last_cron_time_opt.unwrap().eq(&timestamp) {
        STATE.update(storage, |mut s| -> StdResult<_> {
            s.cron_timestamps.push(timestamp);
            s.validator_index_for_next_cron = 0;
            Ok(s)
        })?;

        validator_index_for_next_cron = 0
    }

    if validator_index_for_next_cron.ge(&total_validators) {
        return Ok(vec![]);
    }

    let mut min = clamp(0, validator_index_for_next_cron, total_validators);
    let max = clamp(
        0,
        validator_index_for_next_cron + batch_size,
        total_validators,
    );
    // Examples
    // len = 12, batch_size = 5
    // (=0 <5) 5 (=5 <10) 10 (=10 <12)
    // len = 2, batch_size = 1
    // (=0 <1) 1 (=1 <2)

    let mut validators_batch: Vec<Addr> = vec![];
    while min < max {
        validators_batch.push(validators[min as usize].clone());
        min = min.add(1);
    }

    STATE.update(storage, |mut s| -> StdResult<_> {
        s.validator_index_for_next_cron = max;
        Ok(s)
    })?;

    Ok(validators_batch)
}

fn get_amount_in_vault_denom(
    coin: &Coin,
    vault_denom: &String,
    exchange_rates_map: &mut HashMap<String, Decimal>, // Try to bring it outside (As we are mutating a func param)
    querier: &TerraQuerier,
) -> Option<Decimal> {
    if exchange_rates_map.contains_key(&coin.denom) {
        let exchange_rate = exchange_rates_map.get(&coin.denom).unwrap();
        return Some(convert_amount_to_valut_denom(coin, *exchange_rate)); // Not sure how this * works!
    } else {
        let rate_opt = query_exchange_rate(querier, vault_denom, &coin.denom);
        if rate_opt.is_none() {
            return None;
        }
        let exchange_rate = rate_opt.unwrap();
        exchange_rates_map.insert(coin.denom.clone(), exchange_rate);
        return Some(convert_amount_to_valut_denom(coin, exchange_rate));
    }
}

fn convert_amount_to_valut_denom(coin: &Coin, exchange_rate: Decimal) -> Decimal {
    let amount = uint128_to_decimal(coin.amount);
    let amount_in_vault_denom = decimal_multiplication_in_256(amount, exchange_rate);
    amount_in_vault_denom
}

fn query_exchange_rate(
    querier: &TerraQuerier,
    vault_denom: &String,
    coin_denom: &String,
) -> Option<Decimal> {
    let result = querier.query_exchange_rates(vault_denom, vec![coin_denom]);
    if result.is_err() {
        return None;
    }
    let exchange_rate = result
        .unwrap()
        .exchange_rates
        .first()
        .unwrap()
        .exchange_rate;

    Some(exchange_rate)
}

fn query_state(deps: Deps) -> StdResult<State> {
    let state = STATE.load(deps.storage)?;
    Ok(state)
}

fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config)
}

fn query_all_validator_metrics(
    deps: Deps,
    addr: Addr,
) -> StdResult<Vec<(Vec<u8>, ValidatorMetrics)>> {
    METRICS_HISTORY
        .prefix(&addr)
        .range(deps.storage, None, None, Order::Ascending)
        .collect()
}

fn query_all_validator_metrics_btw_timestamps(
    deps: Deps,
    addr: Addr,
    timestamp1: u64,
    timestamp2: u64,
) -> StdResult<Vec<(Vec<u8>, ValidatorMetrics)>> {
    if timestamp1.ge(&timestamp2) {
        return Err(StdError::GenericErr {
            msg: "timestamp1 cannot be greater than or equal to timestamp2".to_string(),
        });
    }
    let from = Some(Bound::Inclusive(U64Key::new(timestamp1).into()));
    let to = Some(Bound::Inclusive(U64Key::new(timestamp2).into()));

    METRICS_HISTORY
        .prefix(&addr)
        .range(deps.storage, from, to, Order::Ascending)
        .collect()
}

fn query_validator_metrics_by_timestamp(
    deps: Deps,
    addr: Addr,
    timestamp: u64,
) -> StdResult<ValidatorMetrics> {
    METRICS_HISTORY.load(deps.storage, (&addr, U64Key::new(timestamp)))
}

fn query_validators_metrics_by_timestamp(
    deps: Deps,
    timestamp: u64,
    from: u32,
    to: u32,
) -> StdResult<Vec<ValidatorMetrics>> {
    let validators = STATE.load(deps.storage)?.validators;

    let total_validators: u32 = validators.len().try_into().unwrap();

    if to.ge(&total_validators) || from > to {
        return Err(StdError::GenericErr {
            msg: "Invalid indexes!".to_string(),
        });
    }

    let mut start = from;
    let mut res: Vec<ValidatorMetrics> = vec![];
    while start.le(&to) {
        res.push(METRICS_HISTORY.load(deps.storage, (&validators[start as usize], U64Key::new(timestamp)))?);
        start = start + 1;
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, Uint128};

    #[test]
    fn easy_flow() {
        let mut deps = mock_dependencies(&coins(2, "token"));

        let msg = InstantiateMsg {
            amount_to_stake_per_validator: Uint128::new(10),
            vault_denom: "luna".to_string(),
            batch_size: 10,
        };
        let info = mock_info("creator", &coins(2, "token"));
        let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Add Validator
        // Invoke Record metrics here
    }
}
