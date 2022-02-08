use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("AirdropRegistry-Contract: {0}")]
    Std(#[from] StdError),

    #[error("AirdropRegistry-Contract: Unauthorized")]
    Unauthorized {},

    #[error("AirdropRegistry-Contract: Airdrop token cannot be empty")]
    TokenEmpty {},

    #[error("AirdropRegistry-Contract: Temp manager store is empty")]
    TmpManagerStoreEmpty {},
}
