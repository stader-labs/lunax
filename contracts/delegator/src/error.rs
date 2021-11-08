use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Delegator-Contract: not implemented")]
    NotImplemented {},

    #[error("Delegator-Contract: Unauthorized")]
    Unauthorized {},

    #[error("Delegator-Contract: Funds not found")]
    NoFunds {},

    #[error("Delegator-Contract: Multiple funds found instead of one")]
    MultipleFunds {},

    #[error("Delegator-Contract: Funds denom not matching vault denom")]
    InvalidDenom {},

    #[error("Delegator-Contract: Validator not discoverable on blockchain")]
    ValidatorNotDiscoverable {},

    #[error("Delegator-Contract: Please add validator to contract and retry")]
    ValidatorNotAdded {},

    #[error("Delegator-Contract: Validator already exists in contract")]
    ValidatorAlreadyExists {},

    #[error("Delegator-Contract: No sufficient funds for transfer")]
    InSufficientFunds {},

    #[error("Delegator-Contract: Airdrop is not registered")]
    AirdropNotRegistered {},

    #[error("Delegator-Contract: Amount cannot be zero")]
    ZeroAmount {},

    #[error("Delegator-Contract: Redelegation has failed for the provided validators")]
    RedelegationFailed {},

    #[error("Delegator-Contract: Redelegation event object not found")]
    RedelegationEventNotFound {},

    #[error("Delegator-Contract: Pool requested is not found")]
    PoolNotFound {},

    #[error("Delegator-Contract: Pool requested is not active")]
    PoolInactive {},

    #[error("Delegator-Contract: No validators in selected pool")]
    NoValidatorsInPool {},

    #[error("Delegator-Contract: Unexpectedly, no operation was necessary")]
    NoOp {},

    #[error("Delegator-Contract: No user deposit found for the pool")]
    NoUserDeposit {},

    #[error("Delegator-Contract: No user-pool record found")]
    UserNotFound {},

    #[error("Delegator-Contract: Record not found")]
    RecordNotFound {},

    #[error("Delegator-Contract: Non matching amount found")]
    NonMatchingAmount {},

    #[error("Delegator-Contract: Funds not expected to be sent with this request")]
    FundsNotExpected {},

    #[error("Delegator-Contract: Undelegation limit reached - Please withdraw existing funds before undelegation more funds")]
    UndelegationLimitExceeded {},

    #[error("Delegator-Contract: Protocol Fee cannot be more than 100%")]
    ProtocolFeeAboveLimit {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
