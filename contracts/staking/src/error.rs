use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("Staking-Contract: {0}")]
    Std(#[from] StdError),

    #[error("Staking-Contract: Unauthorized")]
    Unauthorized {},

    #[error("Staking-Contract: Funds not found")]
    NoFunds {},

    #[error("Staking-Contract: Multiple funds found instead of one")]
    MultipleFunds {},

    #[error("Staking-Contract: Funds denom not matching vault denom")]
    InvalidDenom {},

    #[error("Staking-Contract: Validator not discoverable on blockchain")]
    ValidatorNotDiscoverable {},

    #[error("Staking-Contract: Please add validator to contract and retry")]
    ValidatorNotAdded {},

    #[error("Staking-Contract: Validator already exists")]
    ValidatorAlreadyAdded {},

    #[error("Staking-Contract: No sufficient funds for transfer")]
    InSufficientFunds {},

    #[error("Staking-Contract: Airdrop is not registered `{0}`")]
    AirdropNotRegistered(String),

    #[error("Staking-Contract: Amount cannot be zero")]
    ZeroAmount {},

    #[error("Staking-Contract: Redelegation has failed for the provided validators")]
    RedelegationFailed {},

    #[error("Staking-Contract: Pool requested is not found")]
    PoolNotFound {},

    #[error("Staking-Contract: Operation has been paused '{0}")]
    OperationPaused(String),

    #[error("Staking-Contract: No validators in selected pool")]
    NoValidatorsInPool {},

    #[error("Staking-Contract: Swap failed with validator contract")]
    SwapFailed {},

    #[error("Staking-Contract: Unexpectedly, no operation was required")]
    NoOp {},

    #[error("Staking-Contract: Undelegation entry not found")]
    UndelegationEntryNotFound {},

    #[error("Staking-Contract: Undelegation batch not found")]
    UndelegationBatchNotFound {},

    #[error("Staking-Contract: Undelegation batch not reconciled yet")]
    UndelegationBatchNotReconciled {},

    #[error("Staking-Contract: Mismatching amounts provided")]
    MismatchingAmounts {},

    #[error("Staking-Contract: Funds not expected with request")]
    FundsNotExpected {},

    #[error("Staking-Contract: Deposit amount cannot be greater than max deposit amount")]
    MaxDeposit {},

    #[error("Staking-Contract: Deposit amount cannot be less than min deposit amount")]
    MinDeposit {},

    #[error("Staking-Contract: All validators in the pool are inactive/jailed")]
    AllValidatorsJailed {},

    #[error("Staking-Contract: Expected rewards to be non-zero for transfer to SCC")]
    ZeroRewards {},

    #[error("Staking-Contract: Validator to redelegate should be different from source validator")]
    ValidatorsCannotBeSame {},

    #[error("Staking-Contract: Redelegation in progress. Cannot remove validator")]
    RedelegationInProgress {},

    #[error("Staking-Contract: Protocol Fee cannot be more than 100%")]
    ProtocolFeeAboveLimit {},

    #[error("Staking-Contract: Undelegation cannot be performed because of cooldown constraint")]
    UndelegationInCooldown {},

    #[error("Staking-Contract: Swap is in cooldown")]
    SwapInCooldown {},

    #[error("Staking-Contract: Reinvest is in cooldown")]
    ReinvestInCooldown {},
}
