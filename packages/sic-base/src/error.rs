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
    #[error("No funds have been sent")]
    NoFundsSent {},

    #[error("Multiple coins have been sent")]
    MultipleCoinsSent {},

    #[error("Cannot withdraw zero amount of money")]
    ZeroWithdrawal {},

    #[error("The coin denom does not match the strategy denom")]
    DenomDoesNotMatchStrategyDenom {},
}
