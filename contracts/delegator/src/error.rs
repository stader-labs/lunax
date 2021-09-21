use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("not implemented")]
    NotImplemented {},

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

    #[error("Validator already exists in contract")]
    ValidatorAlreadyExists {},

    #[error("No sufficient funds for transfer")]
    InSufficientFunds {},

    #[error("Airdrop is not registered")]
    AirdropNotRegistered {},

    #[error("Amount cannot be zero")]
    ZeroAmount {},

    #[error("Redelegation has failed for the provided validators")]
    RedelegationFailed {},

    #[error("Redelegation event object not found")]
    RedelegationEventNotFound {},

    #[error("Pool requested is not found")]
    PoolNotFound {},

    #[error("Pool requested is not active")]
    PoolInactive {},

    #[error("No validators in selected pool")]
    NoValidatorsInPool {},

    #[error("Unexpectedly, no operation was necessary")]
    NoOp {},

    #[error("No user deposit found for the pool")]
    NoUserDeposit {},

    #[error("No user-pool record found")]
    UserNotFound {},

    #[error("Record not found")]
    RecordNotFound {},

    #[error("Non matching amount found")]
    NonMatchingAmount {},

    #[error("Funds not expected to be sent with this request")]
    FundsNotExpected {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
