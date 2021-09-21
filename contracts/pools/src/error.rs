use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Funds not found")]
    NoFunds {},

    #[error("Multiple funds found instead of one")]
    MultipleFunds {},

    #[error("Funds denom not matching vault denom")]
    InvalidDenom {},

    #[error("Validator not discoverable on blockchain")]
    ValidatorNotDiscoverable {},

    #[error("Please add validator to contract and retry")]
    ValidatorNotAdded {},

    #[error("Validator already associated to a pool")]
    ValidatorAssociatedToPool {},

    #[error("No sufficient funds for transfer")]
    InSufficientFunds {},

    #[error("Airdrop is not registered")]
    AirdropNotRegistered {},

    #[error("Amount cannot be zero")]
    ZeroAmount {},

    #[error("Redelegation has failed for the provided validators")]
    RedelegationFailed {},

    #[error("Submessage event object not found")]
    EventNotFound {},

    #[error("Pool requested is not found")]
    PoolNotFound {},

    #[error("Pool requested is not active")]
    PoolInactive {},

    #[error("No validators in selected pool")]
    NoValidatorsInPool {},

    #[error("Swap failed with validator contract")]
    SwapFailed {},

    #[error("Unexpectedly, no operation was required")]
    NoOp {},

    #[error("Undelegation batch not found")]
    UndelegationBatchNotFound {},

    #[error("Undelegation request not ready to be withdrawn")]
    UndelegationNotWithdrawable {},

    #[error("Mismatching amounts provided")]
    MismatchingAmounts {},

    #[error("Funds not expected with request")]
    FundsNotExpected {},

    #[error("Deposit amount cannot be greater than max deposit amount")]
    MaxDeposit {},

    #[error("Deposit amount cannot be less than min deposit amount")]
    MinDeposit {},

// Add any other custom errors you like here.
                                  // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
