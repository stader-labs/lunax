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
    #[error("There are no uninvested rewards currently")]
    NoUninvestedRewards {},

    #[error("There are no unswapped rewards")]
    NoUnswappedRewards {},

    #[error("No funds have been sent")]
    NoFundsSent {},

    #[error("Sent more than one coin")]
    MultipleCoins {},

    #[error("Wrong denom has been sent `{0}`")]
    WrongDenom(String),

    #[error("Cannot undelegate 0 coins")]
    ZeroUndelegation {},

    #[error("undelegation batch does not exist")]
    NonExistentUndelegationBatch {},

    #[error("undelegation batch '{0}' has not been checked for slashing")]
    SlashingNotChecked(u64),

    #[error("Deposit can only be withdrawn after unbonding period is over")]
    DepositInUnbondingPeriod {},
}
