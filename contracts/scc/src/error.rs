use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
    #[error("Strategy info already exists")]
    StrategyInfoAlreadyExists {},

    #[error("Strategy info does not exist `{0}`")]
    StrategyInfoDoesNotExist(String),

    #[error("User reward info does not exist")]
    UserRewardInfoDoesNotExist {},
}
