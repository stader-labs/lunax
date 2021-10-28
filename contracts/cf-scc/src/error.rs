use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Amount cannot be zero")]
    AmountZero {},

    #[error("User info does not exist")]
    UserInfoDoesNotExist {},

    #[error("Cw20 contract not registered `{0}`")]
    Cw20ContractNotRegistered(String),
}
