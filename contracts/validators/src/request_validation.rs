use crate::state::State;
use crate::ContractError;
use cosmwasm_std::MessageInfo;

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
    state: &State, info: &MessageInfo, checks: Vec<Verify>,
) -> Result<(), ContractError> {
    for check in checks {
        match check {
            Verify::SenderManager => {
                if info.sender != state.manager {
                    return Err(ContractError::Unauthorized {});
                }
            }
            Verify::SenderPoolsContract => {
                if info.sender != state.pools_contract_addr {
                    return Err(ContractError::Unauthorized {});
                }
            }
            Verify::NonZeroSingleInfoFund => {
                if info.funds.is_empty() || info.funds[0].amount.is_zero() {
                    return Err(ContractError::NoFunds {});
                }
                if info.funds[0].denom != state.vault_denom {
                    return Err(ContractError::InvalidDenom {});
                }
            }
        }
    }

    Ok(())
}
