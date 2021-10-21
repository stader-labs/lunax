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

    #[error("Funds found but not expected")]
    FundsNotExpected {},

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

    #[error("Not enough slashing funds")]
    NotEnoughSlashingFunds {},

    #[error("No Delegation found")]
    DelegationNotFound {},

    #[error("Mismatching funds")]
    MismatchingFunds {},

    #[error("Reward contract instantiate message failed")]
    RewardInstantiationFailed {},

    #[error("Instantiate event from reward contract missing")]
    InstantiateEventNotFound {}
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
