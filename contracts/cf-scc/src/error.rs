use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("CFSCC-Contract: {0}")]
    Std(#[from] StdError),

    #[error("CFSCC-Contract: Unauthorized")]
    Unauthorized {},

    #[error("CFSCC-Contract: Amount cannot be zero")]
    AmountZero {},

    #[error("User info does not exist")]
    UserInfoDoesNotExist {},

    #[error("Cw20 contract not registered `{0}`")]
    Cw20ContractNotRegistered(String),
}
