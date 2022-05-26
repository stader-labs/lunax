use airdrops_registry::msg::{GetAirdropContractsResponse, QueryMsg as AirdropsQueryMsg};
use airdrops_registry::state::AirdropRegistryInfo;
use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, Binary, Coin, ContractResult, Empty, FullDelegation,
    OwnedDeps, Querier, QuerierResult, QueryRequest, SystemError, SystemResult, Uint128, Validator,
    WasmQuery,
};
use cw20::{BalanceResponse, TokenInfoResponse};
use std::collections::HashMap;
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
        custom_query_type: Default::default(),
    }
}

pub struct WasmMockQuerier {
    base: MockQuerier<Empty>,
    stader_querier: StaderQuerier,
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
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
    pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        match &request {
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
    pub fn new(base: MockQuerier<Empty>) -> Self {
        WasmMockQuerier {
            base,
            stader_querier: StaderQuerier::default(),
        }
    }

    pub fn update_stader_balances(
        &mut self,
        total_reward_tokens: Option<Uint128>,
        user_to_tokens: Option<HashMap<Addr, Uint128>>,
    ) {
        self.stader_querier = StaderQuerier::new(total_reward_tokens, user_to_tokens);
    }
}
