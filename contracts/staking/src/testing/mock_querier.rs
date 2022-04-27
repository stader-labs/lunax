use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, Binary, Coin, ContractResult, Decimal,
    FullDelegation, OwnedDeps, Querier, QuerierResult, QueryRequest, SystemError, SystemResult,
    Uint128, Validator, WasmQuery,
};
use std::collections::HashMap;

use airdrops_registry::msg::{GetAirdropContractsResponse, QueryMsg as AirdropsQueryMsg};
use airdrops_registry::state::AirdropRegistryInfo;
use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage};
use cw20::{BalanceResponse, TokenInfoResponse};
use stader_utils::coin_utils::{decimal_multiplication_in_256, u128_from_decimal};
use terra_cosmwasm::{
    SwapResponse, TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute,
};

pub const MOCK_CONTRACT_ADDR: &str = "cosmos2contract";

pub fn mock_dependencies(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let contract_addr = MOCK_CONTRACT_ADDR;
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(MockQuerier::new(&[(contract_addr, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

#[derive(Clone, Default)]
pub struct TaxQuerier {
    rate: Decimal,
    caps: HashMap<String, Uint128>,
}

impl TaxQuerier {
    pub fn _new(rate: Decimal, caps: &[(&String, &Uint128)]) -> Self {
        TaxQuerier {
            rate,
            caps: _caps_to_map(caps),
        }
    }
}

pub fn _caps_to_map(caps: &[(&String, &Uint128)]) -> HashMap<String, Uint128> {
    let mut owner_map: HashMap<String, Uint128> = HashMap::new();
    for (denom, cap) in caps.iter() {
        owner_map.insert(denom.to_string(), **cap);
    }
    owner_map
}

pub struct WasmMockQuerier {
    base: MockQuerier<TerraQueryWrapper>,
    tax_querier: TaxQuerier,
    stader_querier: StaderQuerier,
    swap_querier: SwapQuerier,
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<TerraQueryWrapper> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(&request)
    }
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<TerraQueryWrapper>) -> QuerierResult {
        match &request {
            QueryRequest::Custom(TerraQueryWrapper { route, query_data }) => {
                if &TerraRoute::Treasury == route {
                    match query_data {
                        TerraQuery::TaxRate {} => {
                            let res = TaxRateResponse {
                                rate: self.tax_querier.rate,
                            };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        TerraQuery::TaxCap { denom } => {
                            let cap = self
                                .tax_querier
                                .caps
                                .get(denom)
                                .copied()
                                .unwrap_or_default();
                            let res = TaxCapResponse { cap };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        _ => panic!("Terra Treasury route query not implemented!"),
                    }
                } else if &TerraRoute::Market == route {
                    match query_data {
                        TerraQuery::Swap {
                            offer_coin,
                            ask_denom,
                        } => {
                            let offer_coin = offer_coin.clone();
                            let ask_denom = ask_denom.clone();
                            let coin_swap_rate_opt =
                                self.swap_querier.swap_rates.iter().find(|x| {
                                    x.offer_denom.eq(&offer_coin.denom)
                                        && x.ask_denom.eq(&ask_denom)
                                });
                            let swap_res: SwapResponse = if let Some(coin_swap_rate) =
                                coin_swap_rate_opt
                            {
                                let swap_amount = u128_from_decimal(decimal_multiplication_in_256(
                                    Decimal::from_ratio(offer_coin.amount, 1_u128),
                                    coin_swap_rate.swap_rate,
                                ));

                                SwapResponse {
                                    receive: Coin::new(swap_amount, ask_denom),
                                }
                            } else {
                                return SystemResult::Err(SystemError::InvalidRequest {
                                    error: "swap not found".to_string(),
                                    request: Default::default(),
                                });
                            };

                            SystemResult::Ok(ContractResult::from(to_binary(&swap_res)))
                        }
                        _ => {
                            panic!("Terra Market route query not implemented!")
                        }
                    }
                } else {
                    panic!("Terra route not implemented!")
                }
            }
            QueryRequest::Wasm(WasmQuery::Raw {
                contract_addr: _,
                key: _,
            }) => {
                panic!("WASMQUERY::RAW not implemented!")
            }
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                if contract_addr.eq("airdrop_registry_contract") {
                    match from_binary(msg).unwrap() {
                        AirdropsQueryMsg::GetAirdropContracts { token } => {
                            let res: GetAirdropContractsResponse;
                            if token.eq(&String::from("unreg_token")) {
                                res = GetAirdropContractsResponse { contracts: None };
                            } else {
                                res = GetAirdropContractsResponse {
                                    contracts: Some(AirdropRegistryInfo {
                                        token: token.clone(),
                                        airdrop_contract: Addr::unchecked(format!(
                                            "{}_airdrop_contract",
                                            token.clone()
                                        )),
                                        cw20_contract: Addr::unchecked(format!(
                                            "{}_cw20_contract",
                                            token.clone()
                                        )),
                                    }),
                                };
                            }
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        _ => {
                            let out = Binary::default();
                            SystemResult::Ok(ContractResult::from(to_binary(&out)))
                        }
                    }
                } else {
                    match from_binary(msg).unwrap() {
                        cw20::Cw20QueryMsg::TokenInfo {} => {
                            let res = TokenInfoResponse {
                                name: "goose luna".to_string(),
                                symbol: "gluna".to_string(),
                                decimals: 6,
                                total_supply: self.stader_querier.total_minted_tokens,
                            };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        cw20::Cw20QueryMsg::Balance { address } => {
                            let res = BalanceResponse {
                                balance: *self
                                    .stader_querier
                                    .user_to_tokens
                                    .get(&Addr::unchecked(address))
                                    .unwrap_or(&Uint128::zero()),
                            };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        _ => {
                            let out = Binary::default();
                            SystemResult::Ok(ContractResult::from(to_binary(&out)))
                        }
                    }
                }
            }
            _ => self.base.handle_query(request),
        }
    }
    pub fn update_staking(
        &mut self,
        denom: &str,
        validators: &[Validator],
        delegations: &[FullDelegation],
    ) {
        self.base.update_staking(denom, validators, delegations);
    }

    pub fn update_balance(&mut self, addr: Addr, balances: Vec<Coin>) -> Option<Vec<Coin>> {
        self.base.update_balance(addr.to_string(), balances)
    }
}

#[derive(Clone, Default)]
pub struct SwapRates {
    pub offer_denom: String,
    pub ask_denom: String,
    pub swap_rate: Decimal,
}

#[derive(Clone, Default)]
struct SwapQuerier {
    pub swap_rates: Vec<SwapRates>,
}

impl SwapQuerier {
    fn default() -> Self {
        SwapQuerier { swap_rates: vec![] }
    }
}

#[derive(Clone, Default)]
struct StaderQuerier {
    pub total_minted_tokens: Uint128,
    pub user_to_tokens: HashMap<Addr, Uint128>,
}

impl StaderQuerier {
    fn default() -> Self {
        StaderQuerier {
            total_minted_tokens: Uint128::zero(),
            user_to_tokens: HashMap::default(),
        }
    }
    fn new(
        total_minted_tokens: Option<Uint128>,
        user_to_tokens: Option<HashMap<Addr, Uint128>>,
    ) -> Self {
        StaderQuerier {
            total_minted_tokens: total_minted_tokens.unwrap_or_default(),
            user_to_tokens: user_to_tokens.unwrap_or_default(),
        }
    }
}

impl WasmMockQuerier {
    pub fn new(base: MockQuerier<TerraQueryWrapper>) -> Self {
        WasmMockQuerier {
            base,
            tax_querier: TaxQuerier::default(),
            stader_querier: StaderQuerier::default(),
            swap_querier: SwapQuerier::default(),
        }
    }

    pub fn update_stader_balances(
        &mut self,
        total_reward_tokens: Option<Uint128>,
        user_to_tokens: Option<HashMap<Addr, Uint128>>,
    ) {
        self.stader_querier = StaderQuerier::new(total_reward_tokens, user_to_tokens);
    }

    // configure the tax mock querier
    pub fn _with_tax(&mut self, rate: Decimal, caps: &[(&String, &Uint128)]) {
        self.tax_querier = TaxQuerier::_new(rate, caps);
    }
}
