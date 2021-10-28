use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("Validator-Contract: {0}")]
    Std(#[from] StdError),

    #[error("Validator-Contract: Unauthorized")]
    Unauthorized {},

    #[error("Validator-Contract: Funds not found")]
    NoFunds {},

    #[error("Validator-Contract: Funds found but not expected")]
    FundsNotExpected {},

    #[error("Validator-Contract: Multiple funds found instead of one")]
    MultipleFunds {},

    #[error("Validator-Contract: Funds denom not matching vault denom")]
    InvalidDenom {},

    #[error("Validator-Contract: Validator not discoverable on blockchain")]
    ValidatorNotDiscoverable {},

    #[error("Validator-Contract: Please add validator to contract and retry")]
    ValidatorNotAdded {},

    #[error("Validator-Contract: Validator already exists in contract")]
    ValidatorAlreadyExists {},

    #[error("Validator-Contract: No sufficient funds for transfer")]
    InSufficientFunds {},

    #[error("Validator-Contract: Airdrop is not registered")]
    AirdropNotRegistered {},

    #[error("Validator-Contract: Amount cannot be zero")]
    ZeroAmount {},

    #[error("Validator-Contract: Redelegation has failed for the provided validators")]
    RedelegationFailed {},

    #[error("Validator-Contract: Redelegation event object not found")]
    RedelegationEventNotFound {},

    #[error("Validator-Contract: Not enough slashing funds")]
    NotEnoughSlashingFunds {},

    #[error("Validator-Contract: No Delegation found")]
    DelegationNotFound {},

    #[error("Validator-Contract: Mismatching funds")]
    MismatchingFunds {},

    #[error("Validator-Contract: Reward contract instantiate message failed")]
    RewardInstantiationFailed {},

    #[error("Validator-Contract: Instantiate event from reward contract missing")]
    InstantiateEventNotFound {}, // Add any other custom errors you like here.
                                 // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
