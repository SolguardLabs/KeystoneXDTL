use thiserror::Error;

pub type KeystoneResult<T> = Result<T, KeystoneError>;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum KeystoneError {
    #[error("amount overflow")]
    AmountOverflow,
    #[error("amount underflow")]
    AmountUnderflow,
    #[error("division by zero")]
    DivisionByZero,
    #[error("basis points out of range: {0}")]
    InvalidBps(u32),
    #[error("share amount is zero")]
    ZeroShares,
    #[error("amount is zero")]
    ZeroAmount,
    #[error("vault not found")]
    VaultNotFound,
    #[error("loan not found")]
    LoanNotFound,
    #[error("account not found")]
    AccountNotFound,
    #[error("asset not found")]
    AssetNotFound,
    #[error("vault already exists")]
    VaultAlreadyExists,
    #[error("loan already exists")]
    LoanAlreadyExists,
    #[error("asset mismatch")]
    AssetMismatch,
    #[error("vault role mismatch")]
    VaultRoleMismatch,
    #[error("vault is paused")]
    VaultPaused,
    #[error("vault is frozen")]
    VaultFrozen,
    #[error("loan status mismatch")]
    LoanStatusMismatch,
    #[error("loan is not due")]
    LoanNotDue,
    #[error("loan is overdue")]
    LoanOverdue,
    #[error("loan is expired")]
    LoanExpired,
    #[error("loan has no remaining principal")]
    LoanClosed,
    #[error("invalid tenor")]
    InvalidTenor,
    #[error("invalid epoch")]
    InvalidEpoch,
    #[error("invalid collateral")]
    InvalidCollateral,
    #[error("insufficient collateral")]
    InsufficientCollateral,
    #[error("insufficient liquidity")]
    InsufficientLiquidity,
    #[error("insufficient shares")]
    InsufficientShares,
    #[error("insufficient cash")]
    InsufficientCash,
    #[error("limit exceeded")]
    LimitExceeded,
    #[error("risk tier blocked")]
    RiskTierBlocked,
    #[error("price not available")]
    PriceNotAvailable,
    #[error("stale price")]
    StalePrice,
    #[error("borrower is the lender")]
    SelfLoan,
    #[error("repayment exceeds outstanding balance")]
    RepaymentTooLarge,
    #[error("redemption exceeds available cash")]
    RedemptionTooLarge,
    #[error("liquidation is not allowed")]
    LiquidationNotAllowed,
    #[error("journal replay mismatch")]
    JournalReplayMismatch,
    #[error("state digest mismatch")]
    DigestMismatch,
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("scenario not found: {0}")]
    ScenarioNotFound(String),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("invariant violation: {0}")]
    Invariant(String),
}

impl KeystoneError {
    pub fn invariant(message: impl Into<String>) -> Self {
        Self::Invariant(message.into())
    }

    pub fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration(message.into())
    }

    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization(message.into())
    }

    pub fn scenario(name: impl Into<String>) -> Self {
        Self::ScenarioNotFound(name.into())
    }
}

#[cfg(test)]
mod tests {
    use super::KeystoneError;

    #[test]
    fn display_messages_are_stable() {
        assert_eq!(KeystoneError::ZeroAmount.to_string(), "amount is zero");
        assert_eq!(
            KeystoneError::InvalidBps(70_001).to_string(),
            "basis points out of range: 70001"
        );
    }
}
