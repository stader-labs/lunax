use cosmwasm_std::StdError;
use thiserror::Error;

use crate::msg::ContractResponse;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Contract address already exists")]
    ContractAlreadyExists {},

    #[error("Contract name already exists")]
    NameAlreadyExists {},

    #[error("Contract does not exist")]
    NotFound {},

    #[error("Redundant contract update")]
    Redundant { msg: String },
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
