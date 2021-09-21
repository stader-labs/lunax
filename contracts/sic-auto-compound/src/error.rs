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

    #[error("sic-ac: Undelegation batch does not exist")]
    NonExistentUndelegationBatch {},

    #[error("sic-ac: Undelegation batch does not have enough funds '{0}'")]
    InsufficientFundsInUndelegationBatch(u64),

    #[error("sic-ac: Undelegation batch is still in unbonding period '{0}'")]
    UndelegationBatchInUnbondingPeriod(u64),

    #[error("sic-ac: undelegation batch '{0}' has not been checked for slashing")]
    SlashingNotChecked(u64),

    #[error("sic-ac: Deposit can only be withdrawn after unbonding period is over")]
    DepositInUnbondingPeriod {},

    #[error("sic-ac: No undelegation batch for id '{0}'")]
    NoUndelegationBatch(u64),

    #[error("sic-ac: Not enough airdrops to withdraw '{0}'")]
    NotEnoughAirdrops(String),

    #[error("sic-ac: validator already exists in pool")]
    ValidatorAlreadyExistsInPool {},

    #[error("sic-ac: validator does not exist in blockchain")]
    ValidatorDoesNotExist {},

    #[error("sic-ac: validator does not exist in pool")]
    ValidatorNotInPool {},

    #[error("sic-ac: validator pool size is at minimum. Cannot remove more validators")]
    CannotRemoveMoreValidators {},
}
