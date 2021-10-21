use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("No sufficient funds for transfer")]
    InSufficientFunds {},

    #[error("Amount cannot be zero")]
    ZeroAmount {},
}
