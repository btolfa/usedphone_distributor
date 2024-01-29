use anchor_lang::prelude::*;

#[error_code]
pub enum DistributorError {
    InvalidParameters,
    ThresholdNotMet,
    MissingRemainingAccounts,
    InvalidAssociatedTokenAccount,
}
