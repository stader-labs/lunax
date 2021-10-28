use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("Pools-Contract: {0}")]
    Std(#[from] StdError),

    #[error("Pools-Contract: Unauthorized")]
    Unauthorized {},

    #[error("Pools-Contract: Funds not found")]
    NoFunds {},

    #[error("Pools-Contract: Multiple funds found instead of one")]
    MultipleFunds {},

    #[error("Pools-Contract: Funds denom not matching vault denom")]
    InvalidDenom {},

    #[error("Pools-Contract: Validator not discoverable on blockchain")]
    ValidatorNotDiscoverable {},

    #[error("Pools-Contract: Please add validator to contract and retry")]
    ValidatorNotAdded {},

    #[error("Pools-Contract: Validator already associated to a pool")]
    ValidatorAssociatedToPool {},

    #[error("Pools-Contract: No sufficient funds for transfer")]
    InSufficientFunds {},

    #[error("Pools-Contract: Airdrop is not registered")]
    AirdropNotRegistered {},

    #[error("Pools-Contract: Amount cannot be zero")]
    ZeroAmount {},

    #[error("Pools-Contract: Redelegation has failed for the provided validators")]
    RedelegationFailed {},

    #[error("Pools-Contract: Submessage event object not found")]
    EventNotFound {},

    #[error("Pools-Contract: Pool requested is not found")]
    PoolNotFound {},

    #[error("Pools-Contract: Pool requested is not active")]
    PoolInactive {},

    #[error("Pools-Contract: No validators in selected pool")]
    NoValidatorsInPool {},

    #[error("Pools-Contract: Swap failed with validator contract")]
    SwapFailed {},

    #[error("Pools-Contract: Unexpectedly, no operation was required")]
    NoOp {},

    #[error("Pools-Contract: Undelegation batch not found")]
    UndelegationBatchNotFound {},

    #[error("Pools-Contract: Undelegation batch not reconciled yet")]
    UndelegationBatchNotReconciled {},

    #[error("Pools-Contract: Mismatching amounts provided")]
    MismatchingAmounts {},

    #[error("Pools-Contract: Funds not expected with request")]
    FundsNotExpected {},

    #[error("Pools-Contract: Deposit amount cannot be greater than max deposit amount")]
    MaxDeposit {},

    #[error("Pools-Contract: Deposit amount cannot be less than min deposit amount")]
    MinDeposit {},

    #[error("Pools-Contract: Provided validator contract is in use for another pool")]
    ValidatorContractInUse {},

    #[error("Pools-Contract: Provided reward contract is in use for another pool")]
    RewardContractInUse {},

    #[error("Pools-Contract: All validators in the pool are inactive/jailed")]
    AllValidatorsJailed {},

    #[error("Pools-Contract: Expected rewards to be non-zero for transfer to SCC")]
    ZeroRewards {},

    #[error("Pools-Contract: Validator to redelegate should be different from source validator")]
    ValidatorsCannotBeSame {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
