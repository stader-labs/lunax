#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Attribute, BankMsg, Binary, Coin, Decimal, Deps, DepsMut,
    DistributionMsg, Env, Fraction, MessageInfo, Order, Response, StakingMsg, StdResult, Uint128,
};

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetConfigResponse, GetCurrentUndelegationBatchIdResponse, GetStateResponse,
    GetTotalTokensResponse, GetUndelegationBatchInfoResponse, InstantiateMsg, QueryMsg,
};
use crate::state::{
    BatchUndelegationRecord, Config, StakeQuota, State, CONFIG, STATE, UNDELEGATION_INFO_LEDGER,
    VALIDATORS_TO_STAKED_QUOTA,
};
use crate::utils::{
    decimal_multiplication_in_256, merge_coin, merge_coin_vector, multiply_coin_with_decimal,
    CoinOp, CoinVecOp, Operation,
};
use cw_storage_plus::U64Key;
use std::collections::HashMap;
use std::ops::Add;
use terra_cosmwasm::{create_swap_msg, SwapResponse, TerraMsgWrapper, TerraQuerier};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let mut state = State {
        contract_genesis_block_height: _env.block.height,
        contract_genesis_timestamp: _env.block.time,
        unbonding_period: (21 * 24 * 3600 + 3600),
        current_undelegation_batch_id: 0,
        accumulated_vault_airdrops: vec![],
        validator_pool: msg.initial_validators,
        unswapped_rewards: vec![],
        uninvested_rewards: Coin::new(0_u128, msg.vault_denom.clone()),

        total_staked_tokens: Uint128::zero(),
        total_slashed_amount: Uint128::zero(),
    };
    if msg.unbonding_period.is_some() {
        state.unbonding_period = msg.unbonding_period.unwrap();
    }

    let config = Config {
        manager: info.sender.clone(),
        scc_contract_address: msg.scc_contract_address,
        vault_denom: msg.vault_denom,
    };

    STATE.save(deps.storage, &state)?;
    CONFIG.save(deps.storage, &config)?;

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
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    match msg {
        ExecuteMsg::TransferRewards {} => try_transfer_rewards(deps, _env, info),
        ExecuteMsg::UndelegateRewards { amount } => {
            try_undelegate_rewards(deps, _env, info, amount)
        }
        ExecuteMsg::WithdrawRewards {
            user,
            undelegation_batch_id,
            amount,
        } => try_withdraw_rewards(deps, _env, info, user, undelegation_batch_id, amount),
        ExecuteMsg::ReconcileUndelegationBatch {
            undelegation_batch_id,
        } => try_reconcile_undelegation_batch(deps, _env, info, undelegation_batch_id),
        ExecuteMsg::Reinvest {} => try_reinvest(deps, _env, info),
        ExecuteMsg::RedeemRewards {} => try_redeem_rewards(deps, _env, info),
        ExecuteMsg::Swap {} => try_swap(deps, _env, info),
    }
}

pub fn try_reconcile_undelegation_batch(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    undelegation_batch_id: u64,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    Ok(Response::default())
}

pub fn try_swap(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    if state.unswapped_rewards.is_empty() {
        return Err(ContractError::NoUnswappedRewards {});
    }

    // fetch the swapped money
    let vault_denom = config.vault_denom;
    let mut logs: Vec<Attribute> = vec![];
    let mut swapped_coin: Coin = Coin::new(0_u128, vault_denom.clone());
    let terra_querier = TerraQuerier::new(&deps.querier);
    let mut failed_coins: Vec<Coin> = vec![];
    let mut messages = vec![];
    for reward_coin in state.unswapped_rewards {
        let mut swapped_out_coin = reward_coin.clone();

        if swapped_out_coin.denom.ne(&vault_denom) {
            let coin_swap_wrapped =
                terra_querier.query_swap(reward_coin.clone(), vault_denom.clone());
            // TODO: bchain99 - I think this could mean that there is no swap possible for the pair.
            if coin_swap_wrapped.is_err() {
                // TODO: bchain99 - Check if this is needed. Check the cases when the query_swap can fail.
                logs.push(attr("failed_to_swap", reward_coin.to_string()));
                failed_coins.push(reward_coin);
                continue;
            }

            messages.push(create_swap_msg(reward_coin, vault_denom.clone()));

            let coin_swap: SwapResponse = coin_swap_wrapped.unwrap();
            swapped_out_coin = coin_swap.receive;
        }

        swapped_coin = merge_coin(
            swapped_coin,
            CoinOp {
                fund: swapped_out_coin,
                operation: Operation::Add,
            },
        );
    }

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        // empty out the unstaked rewards after
        state.unswapped_rewards = state
            .unswapped_rewards
            .into_iter()
            .filter(|coin| failed_coins.contains(coin))
            .collect();
        state.uninvested_rewards = merge_coin(
            state.uninvested_rewards,
            CoinOp {
                fund: swapped_coin.clone(),
                operation: Operation::Add,
            },
        );
        Ok(state)
    });

    logs.push(attr("total_swapped_rewards", swapped_coin.to_string()));

    Ok(Response::new().add_messages(messages).add_attributes(logs))
}

pub fn try_transfer_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage).unwrap();
    if info.sender != config.scc_contract_address {
        return Err(ContractError::Unauthorized {});
    }

    // check if any money is being sent
    if info.funds.is_empty() {
        return Err(ContractError::NoFundsSent {});
    }

    // accept only one coin
    if info.funds.len() > 1 {
        return Err(ContractError::MultipleCoins {});
    }

    let transferred_coin = info.funds[0].clone();
    if transferred_coin.denom.ne(&config.vault_denom) {
        return Err(ContractError::WrongDenom(transferred_coin.denom));
    }

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.uninvested_rewards = merge_coin(
            state.uninvested_rewards,
            CoinOp {
                fund: transferred_coin,
                operation: Operation::Add,
            },
        );
        Ok(state)
    });

    Ok(Response::default())
}

// SCC needs to call this when it processes the undelegations.
pub fn try_undelegate_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    Ok(Response::default())
}

pub fn try_withdraw_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    user: Addr,
    undelegation_batch_id: u64,
    amount: Uint128,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    Ok(Response::default())
}

pub fn try_reinvest(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;
    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    if state.uninvested_rewards.amount.is_zero() {
        return Err(ContractError::NoUninvestedRewards {});
    }

    let vault_denom = config.vault_denom;
    let mut current_total_staked_tokens = Coin::new(0_u128, vault_denom.clone());
    let mut validator_to_delegation_map: HashMap<&Addr, Uint128> = HashMap::new();
    for validator in &state.validator_pool {
        let result = deps
            .querier
            .query_delegation(&_env.contract.address, validator)?;
        // TODO: bchain99 - should not happen
        if result.is_none() {
            continue;
        }

        let full_delegation = result.unwrap();

        validator_to_delegation_map.insert(validator, full_delegation.amount.amount);

        current_total_staked_tokens = merge_coin(
            current_total_staked_tokens,
            CoinOp {
                fund: full_delegation.amount,
                operation: Operation::Add,
            },
        );
    }

    let total_slashed_amount = state
        .total_staked_tokens
        .checked_sub(current_total_staked_tokens.amount)
        .unwrap();

    let rewards_to_invest = state.uninvested_rewards.amount;

    let new_current_staked_tokens = current_total_staked_tokens
        .amount
        .checked_add(rewards_to_invest)
        .unwrap();

    let validator_pool_length = state.validator_pool.len();
    let even_split = rewards_to_invest.u128() / validator_pool_length as u128;
    let mut extra_split = rewards_to_invest.u128() % validator_pool_length as u128;
    let mut messages: Vec<StakingMsg> = vec![];
    state.validator_pool.iter().for_each(|v| {
        let delegation_amount = Uint128::new(even_split + extra_split);
        if !delegation_amount.is_zero() {
            messages.push(StakingMsg::Delegate {
                validator: v.to_string(),
                amount: Coin {
                    denom: vault_denom.clone(),
                    amount: delegation_amount,
                },
            });
        }

        let current_validator_staked_amount = *(validator_to_delegation_map.get(v).unwrap());
        let new_validator_staked_amount = current_validator_staked_amount
            .checked_add(delegation_amount)
            .unwrap();
        // validator stake quota will get updated as we are reconciling the validator stake
        let new_validator_stake_quota: StakeQuota = StakeQuota {
            amount: Coin {
                denom: vault_denom.clone(),
                amount: new_validator_staked_amount,
            },
            vault_stake_fraction: Decimal::from_ratio(
                new_validator_staked_amount,
                new_current_staked_tokens,
            ),
        };

        VALIDATORS_TO_STAKED_QUOTA.save(deps.storage, v, &new_validator_stake_quota);
        extra_split = 0_u128;
    });

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.total_staked_tokens = new_current_staked_tokens;
        if total_slashed_amount > Uint128::zero() {
            state.total_slashed_amount = state
                .total_slashed_amount
                .checked_add(total_slashed_amount)
                .unwrap();
        }
        Ok(state)
    })?;

    Ok(Response::new().add_messages(messages))
}

pub fn try_redeem_rewards(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.manager {
        return Err(ContractError::Unauthorized {});
    }

    let mut total_rewards: Vec<Coin> = vec![];
    let mut messages: Vec<DistributionMsg> = vec![];

    for validator in &state.validator_pool {
        let result = deps
            .querier
            .query_delegation(&_env.contract.address, validator)?;
        if result.is_none() {
            continue;
        } else {
            let full_delegation = result.unwrap();
            total_rewards = merge_coin_vector(
                full_delegation.accumulated_rewards,
                CoinVecOp {
                    fund: total_rewards,
                    operation: Operation::Add,
                },
            );
        }

        messages.push(DistributionMsg::WithdrawDelegatorReward {
            validator: validator.to_string(),
        });
    }

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.unswapped_rewards = merge_coin_vector(
            state.unswapped_rewards,
            CoinVecOp {
                fund: total_rewards,
                operation: Operation::Add,
            },
        );

        Ok(state)
    });

    Ok(Response::new().add_messages(messages))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetTotalTokens {} => to_binary(&query_total_tokens(deps, _env)?),
        QueryMsg::GetCurrentUndelegationBatchId {} => {
            to_binary(&query_current_undelegation_batch_id(deps, _env)?)
        }
        QueryMsg::GetState {} => to_binary(&query_state(deps)?),
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::GetUndelegationBatchInfo {
            undelegation_batch_id,
        } => to_binary(&query_undelegation_batch_info(deps, undelegation_batch_id)?),
    }
}

fn query_state(deps: Deps) -> StdResult<GetStateResponse> {
    let state = STATE.may_load(deps.storage).unwrap();

    Ok(GetStateResponse { state })
}

fn query_config(deps: Deps) -> StdResult<GetConfigResponse> {
    let config = CONFIG.may_load(deps.storage).unwrap();

    Ok(GetConfigResponse { config })
}

fn query_total_tokens(deps: Deps, _env: Env) -> StdResult<GetTotalTokensResponse> {
    let state = STATE.load(deps.storage).unwrap();
    Ok(GetTotalTokensResponse {
        total_tokens: Option::from(state.total_staked_tokens),
    })
}

fn query_current_undelegation_batch_id(
    deps: Deps,
    _env: Env,
) -> StdResult<GetCurrentUndelegationBatchIdResponse> {
    let state = STATE.load(deps.storage).unwrap();

    Ok(GetCurrentUndelegationBatchIdResponse {
        current_undelegation_batch_id: state.current_undelegation_batch_id,
    })
}

fn query_undelegation_batch_info(
    deps: Deps,
    undelegation_batch_id: u64,
) -> StdResult<GetUndelegationBatchInfoResponse> {
    let undelegation_batch_info = UNDELEGATION_INFO_LEDGER
        .may_load(deps.storage, U64Key::new(undelegation_batch_id))
        .unwrap();

    Ok(GetUndelegationBatchInfoResponse {
        undelegation_batch_info,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::check_equal_vec;
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
        MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{coins, from_binary, Empty, FullDelegation, OwnedDeps, SubMsg, Validator};

    fn get_validators() -> Vec<Validator> {
        vec![
            Validator {
                address: "valid0001".to_string(),
                commission: Decimal::zero(),
                max_commission: Decimal::zero(),
                max_change_rate: Decimal::zero(),
            },
            Validator {
                address: "valid0002".to_string(),
                commission: Decimal::zero(),
                max_commission: Decimal::zero(),
                max_change_rate: Decimal::zero(),
            },
        ]
    }

    fn get_delegations() -> Vec<FullDelegation> {
        vec![
            FullDelegation {
                delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                validator: "valid0001".to_string(),
                amount: Coin::new(2000, "uluna".to_string()),
                can_redelegate: Coin::new(1000, "uluna".to_string()),
                accumulated_rewards: vec![
                    Coin::new(20, "uluna".to_string()),
                    Coin::new(30, "urew1"),
                ],
            },
            FullDelegation {
                delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                validator: "valid0002".to_string(),
                amount: Coin::new(2000, "uluna".to_string()),
                can_redelegate: Coin::new(0, "uluna".to_string()),
                accumulated_rewards: vec![
                    Coin::new(40, "uluna".to_string()),
                    Coin::new(60, "urew1"),
                ],
            },
        ]
    }

    pub fn instantiate_contract(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
        info: &MessageInfo,
        env: &Env,
        validators: Option<Vec<Addr>>,
        vault_denom: Option<String>,
    ) -> Response<Empty> {
        let default_validator1: Addr = Addr::unchecked("valid0001");
        let default_validator2: Addr = Addr::unchecked("valid0002");
        let scc_contract_address: Addr = Addr::unchecked("scc-contract-address");

        let instantiate_msg = InstantiateMsg {
            scc_contract_address,
            vault_denom: "uluna".to_string(),
            initial_validators: validators
                .unwrap_or_else(|| vec![default_validator1, default_validator2]),
            unbonding_period: None,
        };

        return instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    }

    fn get_scc_contract_address() -> String {
        String::from("scc-contract-address")
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &coins(1000, "earth"));
        let env = mock_env();

        let default_validator1: Addr = Addr::unchecked("valid0001");
        let default_validator2: Addr = Addr::unchecked("valid0002");
        let scc_contract_address: Addr = Addr::unchecked("scc-contract-address");

        // we can just call .unwrap() to assert this was a success
        let res = instantiate_contract(&mut deps, &info, &env, None, None);
        assert_eq!(0, res.messages.len());

        let state = query_state(deps.as_ref()).unwrap().state;
        assert_ne!(state, None);
        assert_eq!(
            state.unwrap(),
            State {
                contract_genesis_block_height: env.block.height,
                contract_genesis_timestamp: env.block.time,
                unbonding_period: (21 * 24 * 3600 + 3600),
                current_undelegation_batch_id: 0,
                accumulated_vault_airdrops: vec![],
                validator_pool: vec![default_validator1, default_validator2],
                unswapped_rewards: vec![],
                uninvested_rewards: Coin::new(0_u128, "uluna".to_string()),
                total_staked_tokens: Uint128::zero(),
                total_slashed_amount: Uint128::zero()
            }
        );

        let config = query_config(deps.as_ref()).unwrap().config;
        assert_ne!(config, None);
        assert_eq!(
            config.unwrap(),
            Config {
                manager: info.sender,
                scc_contract_address,
                vault_denom: "uluna".to_string()
            }
        );
    }

    #[test]
    fn test__try_reinvest__fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoUninvestedRewards {}));
    }

    #[test]
    fn test__try_reinvest__success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        fn get_zero_delegations() -> Vec<FullDelegation> {
            vec![
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0001".to_string(),
                    amount: Coin::new(0, "uluna".to_string()),
                    can_redelegate: Coin::new(1000, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(00, "uluna".to_string()),
                        Coin::new(00, "urew1"),
                    ],
                },
                FullDelegation {
                    delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
                    validator: "valid0002".to_string(),
                    amount: Coin::new(0, "uluna".to_string()),
                    can_redelegate: Coin::new(0, "uluna".to_string()),
                    accumulated_rewards: vec![
                        Coin::new(00, "uluna".to_string()),
                        Coin::new(00, "urew1"),
                    ],
                },
            ]
        }

        let deleg1 = Addr::unchecked("deleg0001".to_string());
        let deleg2 = Addr::unchecked("deleg0002".to_string());
        let deleg3 = Addr::unchecked("deleg0003".to_string());
        let valid1 = Addr::unchecked("valid0001".to_string());
        let valid2 = Addr::unchecked("valid0002".to_string());
        let valid3 = Addr::unchecked("valid0003".to_string());

        /*
           Test - 1. First reinvest
        */
        deps.querier
            .update_staking("test", &*get_validators(), &*get_zero_delegations());

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.uninvested_rewards = Coin::new(1000_u128, "uluna");
            Ok(state)
        });
        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(100_u128, "uluna")]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap();
        let state_response = query_state(deps.as_ref()).unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_staked_tokens, Uint128::new(1000_u128));
        assert_eq!(state.total_slashed_amount, Uint128::zero());
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                }),
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid2.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                })
            ]
        ));
        let valid1_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert_ne!(valid1_staked_quota_option, None);
        let valid2_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid2)
            .unwrap();
        assert_ne!(valid2_staked_quota_option, None);
        let valid1_staked_quota = valid1_staked_quota_option.unwrap();
        assert_eq!(valid1_staked_quota.amount, Coin::new(500_u128, "uluna"));
        assert_eq!(
            valid1_staked_quota.vault_stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );
        let valid2_staked_quota = valid2_staked_quota_option.unwrap();
        assert_eq!(valid2_staked_quota.amount, Coin::new(500_u128, "uluna"));
        assert_eq!(
            valid2_staked_quota.vault_stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );

        /*
           Test - 2. Reinvesting after a few reinvests
        */
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.uninvested_rewards = Coin::new(1000_u128, "uluna");
            state.total_staked_tokens = Uint128::new(4000_u128);
            Ok(state)
        });

        VALIDATORS_TO_STAKED_QUOTA.save(
            deps.as_mut().storage,
            &valid1,
            &StakeQuota {
                amount: Coin::new(2000_u128, "uluna"),
                vault_stake_fraction: Decimal::from_ratio(1_u128, 2_u128),
            },
        );
        VALIDATORS_TO_STAKED_QUOTA.save(
            deps.as_mut().storage,
            &valid1,
            &StakeQuota {
                amount: Coin::new(2000_u128, "uluna"),
                vault_stake_fraction: Decimal::from_ratio(1_u128, 2_u128),
            },
        );

        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(100_u128, "uluna")]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap();
        let state_response = query_state(deps.as_ref()).unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_staked_tokens, Uint128::new(5000_u128));
        assert_eq!(state.total_slashed_amount, Uint128::zero());
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                }),
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid2.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                })
            ]
        ));
        let valid1_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert_ne!(valid1_staked_quota_option, None);
        let valid2_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid2)
            .unwrap();
        assert_ne!(valid2_staked_quota_option, None);
        let valid1_staked_quota = valid1_staked_quota_option.unwrap();
        assert_eq!(valid1_staked_quota.amount, Coin::new(2500_u128, "uluna"));
        assert_eq!(
            valid1_staked_quota.vault_stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );
        let valid2_staked_quota = valid2_staked_quota_option.unwrap();
        assert_eq!(valid2_staked_quota.amount, Coin::new(2500_u128, "uluna"));
        assert_eq!(
            valid2_staked_quota.vault_stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );

        /*
           Test - 3. Slashing
        */
        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.uninvested_rewards = Coin::new(1000_u128, "uluna");
            state.total_staked_tokens = Uint128::new(5000_u128);
            Ok(state)
        });
        VALIDATORS_TO_STAKED_QUOTA.save(
            deps.as_mut().storage,
            &valid1,
            &StakeQuota {
                amount: Coin::new(2500_u128, "uluna"),
                vault_stake_fraction: Decimal::from_ratio(1_u128, 2_u128),
            },
        );
        VALIDATORS_TO_STAKED_QUOTA.save(
            deps.as_mut().storage,
            &valid1,
            &StakeQuota {
                amount: Coin::new(2500_u128, "uluna"),
                vault_stake_fraction: Decimal::from_ratio(1_u128, 2_u128),
            },
        );
        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[Coin::new(100_u128, "uluna")]),
            ExecuteMsg::Reinvest {},
        )
        .unwrap();
        let state_response = query_state(deps.as_ref()).unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.total_staked_tokens, Uint128::new(5000_u128));
        assert_eq!(state.total_slashed_amount, Uint128::new(1000_u128));
        assert_eq!(res.messages.len(), 2);
        assert!(check_equal_vec(
            res.messages,
            vec![
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid1.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                }),
                SubMsg::new(StakingMsg::Delegate {
                    validator: valid2.to_string(),
                    amount: Coin::new(500_u128, "uluna")
                })
            ]
        ));
        let valid1_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid1)
            .unwrap();
        assert_ne!(valid1_staked_quota_option, None);
        let valid2_staked_quota_option = VALIDATORS_TO_STAKED_QUOTA
            .may_load(deps.as_mut().storage, &valid2)
            .unwrap();
        assert_ne!(valid2_staked_quota_option, None);
        let valid1_staked_quota = valid1_staked_quota_option.unwrap();
        assert_eq!(valid1_staked_quota.amount, Coin::new(2500_u128, "uluna"));
        assert_eq!(
            valid1_staked_quota.vault_stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );
        let valid2_staked_quota = valid2_staked_quota_option.unwrap();
        assert_eq!(valid2_staked_quota.amount, Coin::new(2500_u128, "uluna"));
        assert_eq!(
            valid2_staked_quota.vault_stake_fraction,
            Decimal::from_ratio(1_u128, 2_u128)
        );
    }

    #[test]
    fn test__try_transfer_rewards__fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-scc-contract", &[]),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoFundsSent {}));

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[]),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoFundsSent {}));

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[Coin::new(10_u128, "abc"), Coin::new(100_u128, "def")],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::MultipleCoins {}));

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(&*get_scc_contract_address(), &[Coin::new(10_u128, "abc")]),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::WrongDenom { .. }));
    }

    #[test]
    fn test__try_transfer_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        /*
           Test - 1. First reinvest
        */
        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[Coin::new(100_u128, "uluna")],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();

        let state_response = query_state(deps.as_ref()).unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.uninvested_rewards, Coin::new(100_u128, "uluna"));

        /*
           Test - 2. Reinvest with existing uninvested_rewards
        */
        STATE.update(deps.as_mut().storage, |mut state| -> StdResult<_> {
            state.uninvested_rewards = Coin::new(1000_u128, "uluna");
            Ok(state)
        });

        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                &*get_scc_contract_address(),
                &[Coin::new(100_u128, "uluna")],
            ),
            ExecuteMsg::TransferRewards {},
        )
        .unwrap();

        let state_response = query_state(deps.as_ref()).unwrap();
        assert_ne!(state_response.state, None);
        let state = state_response.state.unwrap();
        assert_eq!(state.uninvested_rewards, Coin::new(1100_u128, "uluna"));
    }

    #[test]
    fn test__try_redeem_rewards_fail() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        let mut err = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("not-creator", &[]),
            ExecuteMsg::RedeemRewards {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test__try_redeem_rewards_success() {
        let mut deps = mock_dependencies(&[]);
        let info = mock_info("creator", &[]);
        let env = mock_env();

        let res = instantiate_contract(
            &mut deps,
            &info,
            &env,
            Some(
                get_validators()
                    .iter()
                    .map(|f| Addr::unchecked(&f.address))
                    .collect(),
            ),
            Option::from("uluna".to_string()),
        );

        deps.querier
            .update_staking("test", &*get_validators(), &*get_delegations());

        let mut res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            ExecuteMsg::RedeemRewards {},
        )
        .unwrap();
        assert_eq!(res.messages.len(), 2);
        assert_eq!(
            res.messages,
            vec![
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0001".to_string(),
                }),
                SubMsg::new(DistributionMsg::WithdrawDelegatorReward {
                    validator: "valid0002".to_string(),
                })
            ]
        );
        let mut state = STATE.load(deps.as_mut().storage).unwrap();
        assert!(check_equal_vec(
            state.unswapped_rewards,
            vec![Coin::new(90, "urew1"), Coin::new(60, "uluna")]
        ));
    }
}
