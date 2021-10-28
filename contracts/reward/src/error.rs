use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("Reward-Contract: {0}")]
    Std(#[from] StdError),

    #[error("Reward-Contract: Unauthorized")]
    Unauthorized {},

    #[error("Reward-Contract: No sufficient funds for transfer")]
    InSufficientFunds {},

    #[error("Reward-Contract: Amount cannot be zero")]
    ZeroAmount {},
}
