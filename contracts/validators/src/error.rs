use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Funds not found")]
    NoFunds {},

    #[error("Funds denom not matching vault denom")]
    InvalidDenom {},

    #[error("Validator not discoverable on blockchain")]
    ValidatorNotDiscoverable {},

    #[error("Please add validator to contract and retry")]
    ValidatorNotAdded {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
