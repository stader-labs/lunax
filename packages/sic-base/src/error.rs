use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("sic-base: Unauthorized")]
    Unauthorized {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
    #[error("sic-base: No funds have been sent")]
    NoFundsSent {},

    #[error("sic-base: Insufficient funds in contract")]
    InSufficientFunds {},

    #[error("sic-base: Multiple coins have been sent")]
    MultipleCoinsSent {},

    #[error("sic-base: Cannot withdraw zero amount of money")]
    ZeroWithdrawal {},

    #[error("sic-base: Wrong denom has been sent `{0}`")]
    WrongDenom(String),
}
