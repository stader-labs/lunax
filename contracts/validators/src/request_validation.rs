use crate::state::Config;
use crate::ContractError;
use cosmwasm_std::MessageInfo;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Verify {
    // If info.sender != manager, throw error
    SenderManager,

    SenderPoolsContract,

    //Info.funds is expected to be one
    NonZeroSingleInfoFund,
    // If info.funds are empty or zero
    // NonEmptyInfoFunds,
}

pub fn validate(
    state: &Config, info: &MessageInfo, checks: Vec<Verify>,
) -> Result<(), ContractError> {
    for check in checks {
        println!("Check| {:?}", check.clone());
        match check {
            Verify::SenderManager => {
                if info.sender != state.manager {
                    return Err(ContractError::Unauthorized {});
                }
            },
            Verify::SenderPoolsContract => {
                if info.sender != state.pools_contract_addr {
                    return Err(ContractError::Unauthorized {});
                }
            },
            Verify::NonZeroSingleInfoFund => {
                println!("Info_Funds|{:?}", &info.funds);
                if info.funds.is_empty() || info.funds[0].amount.is_zero() {
                    return Err(ContractError::NoFunds {});
                }
                if info.funds.len() > 1 {
                    return Err(ContractError::MultipleFunds {});
                }
                if info.funds[0].denom != state.vault_denom {
                    return Err(ContractError::InvalidDenom {});
                }
            },
        }
    }

    Ok(())
}
