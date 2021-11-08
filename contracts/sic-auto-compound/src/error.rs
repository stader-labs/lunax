use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("sic-ac: Unauthorized")]
    Unauthorized {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
    #[error("sic-ac: There are no uninvested rewards currently")]
    NoUninvestedRewards {},

    #[error("sic-ac: There are no unswapped rewards")]
    NoUnswappedRewards {},

    #[error("sic-ac: No funds have been sent")]
    NoFundsSent {},

    #[error("sic-ac: Sent more than one coin")]
    MultipleCoins {},

    #[error("sic-ac: Wrong denom has been sent `{0}`")]
    WrongDenom(String),

    #[error("sic-ac: Cannot undelegate 0 coins")]
    ZeroUndelegation {},

    #[error("sic-ac: Cannot withdraw 0 coins")]
    ZeroWithdrawal {},

    #[error("sic-ac: not enough funds staked to undelegate")]
    NotEnoughFundsToUndelegate {},

    #[error("sic-ac: Not enough airdrops to withdraw '{0}'")]
    NotEnoughAirdrops(String),

    #[error("sic-ac: validator already exists in pool")]
    ValidatorAlreadyExistsInPool {},

    #[error("sic-ac: validator does not exist in blockchain")]
    ValidatorDoesNotExist {},

    #[error("sic-ac: validator does not exist in pool")]
    ValidatorNotInPool {},

    #[error("sic-ac: no validatos in pool")]
    NoValidatorsInPool {},

    #[error("sic-ac: all validators are jailed")]
    AllValidatorsJailed {},

    #[error("sic-ac: validator pool size is at minimum. Cannot remove more validators")]
    CannotRemoveMoreValidators {},

    #[error("sic-ac: not enough manager funds have been sent")]
    NotEnoughManagerFundsSent {},

    #[error("sic-ac: redelegation to validator is still in progress")]
    RedelegationInProgress {},
}
