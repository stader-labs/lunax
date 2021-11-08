use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("scc: Unauthorized")]
    Unauthorized {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
    #[error("scc: Strategy info already exists")]
    StrategyInfoAlreadyExists {},

    #[error("scc: Strategy info does not exist")]
    StrategyInfoDoesNotExist {},

    #[error("scc: Strategy does not support airdrops")]
    StrategyDoesNotSupportAirdrop {},

    #[error("scc: Stratey has no pending undelegations")]
    NoPendingUndelegations {},

    #[error("scc: Airdrop not registered")]
    AirdropNotRegistered {},

    #[error("scc: Cannot undelegate zero funds")]
    CannotUndelegateZeroFunds {},

    #[error("scc: User does not have rewards in the strategy")]
    UserNotInStrategy {},

    #[error("scc: User does not have enough rewards to undelegate")]
    UserDoesNotHaveEnoughRewards {},

    #[error("scc: Undelegation record not found")]
    UndelegationRecordNotFound {},

    #[error("scc: Undelegation batch not found")]
    UndelegationBatchNotFound {},

    #[error("scc: Undelegation in unbonding period")]
    UndelegationInUnbondingPeriod {},

    #[error("scc: Undelegation batch in unbonding period `{0}`")]
    UndelegationBatchInUnbondingPeriod(u64),

    #[error("scc: Undelegation batch has not been released yet")]
    UndelegationBatchNotReleased {},

    #[error("scc: User reward info does not exist")]
    UserRewardInfoDoesNotExist {},

    #[error("scc: User undelegations record limit exceeded")]
    UserUndelegationRecordLimitExceeded {},

    #[error("scc: User portfolio fraction is greater than one")]
    InvalidPortfolioDepositFraction {},

    #[error("scc: SIC failed to return a result")]
    SICFailedToReturnResult {},

    #[error("scc: No funds were sent")]
    NoFundsSent {},

    #[error("scc: Multiple coins sent, only luna is accepted")]
    MultipleCoinsSent {},

    #[error("scc: SCC can accept only luna")]
    WrongDenomSent {},

    #[error("scc: User portfolio is invalid")]
    InvalidUserPortfolio {},

    #[error("scc: Zero amount sent")]
    ZeroAmount {},

    #[error("scc: Previous undelegation is in cooldown")]
    UndelegationInCooldown {},
}
