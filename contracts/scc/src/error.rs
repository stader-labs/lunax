use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
    #[error("Strategy info already exists")]
    StrategyInfoAlreadyExists {},

    #[error("Strategy metadata already exists")]
    StrategyMetadataAlreadyExists {},

    #[error("Strategy info does not exist")]
    StrategyInfoDoesNotExist {},

    #[error("Strategy metadata does not exist")]
    StrategyMetadataDoesNotExist {},

    #[error("Airdrop not registered")]
    AirdropNotRegistered {},

    #[error("The strategy does not support the airdrop")]
    StrategyDoesNotSupportAirdrop {},

    #[error("The user reward info does not exist")]
    UserRewardInfoDoesNotExist {}
}
