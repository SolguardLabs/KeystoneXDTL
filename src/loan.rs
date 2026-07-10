use serde::{Deserialize, Serialize};

use crate::{
    Amount, Bps, CollateralPolicy, Digest, Epoch, KeystoneError, KeystoneResult, LoanId, Schedule,
    VaultId,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoanStatus {
    Proposed,
    Active,
    Paid,
    Defaulted,
    Liquidating,
    Liquidated,
    WrittenOff,
}

impl LoanStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            LoanStatus::Paid | LoanStatus::Liquidated | LoanStatus::WrittenOff
        )
    }

    pub fn can_repay(self) -> bool {
        matches!(self, LoanStatus::Active | LoanStatus::Defaulted)
    }

    pub fn can_default(self) -> bool {
        matches!(self, LoanStatus::Active)
    }

    pub fn can_liquidate(self) -> bool {
        matches!(self, LoanStatus::Defaulted | LoanStatus::Liquidating)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepaymentMode {
    Scheduled,
    Early,
    Partial,
    Recovery,
}

impl RepaymentMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RepaymentMode::Scheduled => "scheduled",
            RepaymentMode::Early => "early",
            RepaymentMode::Partial => "partial",
            RepaymentMode::Recovery => "recovery",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoanTermsInput {
    pub network_id: u32,
    pub lender: VaultId,
    pub borrower: VaultId,
    pub principal: Amount,
    pub collateral: Amount,
    pub annual_rate_bps: Bps,
    pub schedule: Schedule,
    pub policy_digest: Digest,
    pub nonce: u64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoanTerms {
    pub network_id: u32,
    pub lender: VaultId,
    pub borrower: VaultId,
    pub principal: Amount,
    pub collateral: Amount,
    pub annual_rate_bps: Bps,
    pub schedule: Schedule,
    pub policy_digest: Digest,
    pub nonce: u64,
}

impl LoanTerms {
    pub fn new(input: LoanTermsInput) -> KeystoneResult<Self> {
        if input.lender == input.borrower {
            return Err(KeystoneError::SelfLoan);
        }
        if input.principal.is_zero() {
            return Err(KeystoneError::ZeroAmount);
        }
        if input.collateral.is_zero() {
            return Err(KeystoneError::InvalidCollateral);
        }
        Ok(Self {
            network_id: input.network_id,
            lender: input.lender,
            borrower: input.borrower,
            principal: input.principal,
            collateral: input.collateral,
            annual_rate_bps: input.annual_rate_bps,
            schedule: input.schedule,
            policy_digest: input.policy_digest,
            nonce: input.nonce,
        })
    }

    pub fn loan_id(self) -> LoanId {
        LoanId::derive(
            self.lender,
            self.borrower,
            self.nonce,
            self.principal.raw(),
            self.schedule.start.raw(),
        )
    }

    pub fn digest(self) -> KeystoneResult<Digest> {
        let bytes = serde_json::to_vec(&self)
            .map_err(|error| KeystoneError::serialization(error.to_string()))?;
        Ok(Digest::from_parts("keystonexdtl-loan-terms-v1", &[&bytes]))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentQuote {
    pub loan: LoanId,
    pub mode: RepaymentMode,
    pub principal: Amount,
    pub interest: Amount,
    pub borrower_interest_reduction: Amount,
    pub lender_projection_reduction: Amount,
    pub collateral_release: Amount,
    pub closes_loan: bool,
}

impl PaymentQuote {
    pub fn total(self) -> KeystoneResult<Amount> {
        self.principal.checked_add(self.interest)
    }

    pub fn is_zero(self) -> bool {
        self.principal.is_zero() && self.interest.is_zero()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoanSnapshot {
    pub loan: LoanId,
    pub lender: VaultId,
    pub borrower: VaultId,
    pub status: LoanStatus,
    pub principal: Amount,
    pub remaining_principal: Amount,
    pub scheduled_interest: Amount,
    pub interest_paid: Amount,
    pub borrower_interest_released: Amount,
    pub collateral_locked: Amount,
    pub collateral_released: Amount,
    pub start_epoch: Epoch,
    pub maturity_epoch: Epoch,
    pub due_epoch: Epoch,
    pub annual_rate_bps: Bps,
    pub terms_digest: Digest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoanState {
    id: LoanId,
    terms: LoanTerms,
    terms_digest: Digest,
    status: LoanStatus,
    remaining_principal: Amount,
    scheduled_interest: Amount,
    interest_paid: Amount,
    borrower_interest_released: Amount,
    collateral_locked: Amount,
    collateral_released: Amount,
    default_epoch: Option<Epoch>,
}

impl LoanState {
    pub fn activate(terms: LoanTerms, scheduled_interest: Amount) -> KeystoneResult<Self> {
        let id = terms.loan_id();
        Ok(Self {
            id,
            terms,
            terms_digest: terms.digest()?,
            status: LoanStatus::Active,
            remaining_principal: terms.principal,
            scheduled_interest,
            interest_paid: Amount::ZERO,
            borrower_interest_released: Amount::ZERO,
            collateral_locked: terms.collateral,
            collateral_released: Amount::ZERO,
            default_epoch: None,
        })
    }

    pub fn id(&self) -> LoanId {
        self.id
    }

    pub fn terms(&self) -> LoanTerms {
        self.terms
    }

    pub fn lender(&self) -> VaultId {
        self.terms.lender
    }

    pub fn borrower(&self) -> VaultId {
        self.terms.borrower
    }

    pub fn principal(&self) -> Amount {
        self.terms.principal
    }

    pub fn remaining_principal(&self) -> Amount {
        self.remaining_principal
    }

    pub fn scheduled_interest(&self) -> Amount {
        self.scheduled_interest
    }

    pub fn interest_paid(&self) -> Amount {
        self.interest_paid
    }

    pub fn borrower_interest_released(&self) -> Amount {
        self.borrower_interest_released
    }

    pub fn collateral_locked(&self) -> Amount {
        self.collateral_locked
    }

    pub fn collateral_released(&self) -> Amount {
        self.collateral_released
    }

    pub fn status(&self) -> LoanStatus {
        self.status
    }

    pub fn default_epoch(&self) -> Option<Epoch> {
        self.default_epoch
    }

    pub fn remaining_scheduled_interest(&self) -> KeystoneResult<Amount> {
        self.scheduled_interest
            .checked_sub(self.borrower_interest_released)
    }

    pub fn debt_due_for_borrower(&self) -> KeystoneResult<Amount> {
        self.remaining_principal
            .checked_add(self.remaining_scheduled_interest()?)
    }

    pub fn lender_receivable(&self) -> KeystoneResult<Amount> {
        self.remaining_principal
            .checked_add(self.scheduled_interest.checked_sub(self.interest_paid)?)
    }

    pub fn accrued_interest(&self, now: Epoch, epochs_per_year: u64) -> KeystoneResult<Amount> {
        let elapsed = self.terms.schedule.elapsed(now)?;
        let accrued = Bps::annualized_for_epochs_ceil(
            self.terms.annual_rate_bps,
            self.terms.principal,
            elapsed,
            epochs_per_year,
        )?;
        Ok(accrued.min(self.scheduled_interest))
    }

    pub fn earned_interest_remaining(
        &self,
        now: Epoch,
        epochs_per_year: u64,
    ) -> KeystoneResult<Amount> {
        self.accrued_interest(now, epochs_per_year)?.checked_sub(
            self.interest_paid
                .min(self.accrued_interest(now, epochs_per_year)?),
        )
    }

    pub fn quote_scheduled_repayment(&self) -> KeystoneResult<PaymentQuote> {
        if !self.status.can_repay() {
            return Err(KeystoneError::LoanStatusMismatch);
        }
        let interest = self.remaining_scheduled_interest()?;
        Ok(PaymentQuote {
            loan: self.id,
            mode: RepaymentMode::Scheduled,
            principal: self.remaining_principal,
            interest,
            borrower_interest_reduction: interest,
            lender_projection_reduction: interest,
            collateral_release: self.collateral_locked,
            closes_loan: true,
        })
    }

    pub fn quote_early_repayment(
        &self,
        now: Epoch,
        epochs_per_year: u64,
    ) -> KeystoneResult<PaymentQuote> {
        if !self.status.can_repay() {
            return Err(KeystoneError::LoanStatusMismatch);
        }
        let accrued = self.accrued_interest(now, epochs_per_year)?;
        let interest_due = accrued.checked_sub(self.interest_paid.min(accrued))?;
        let borrower_interest_reduction = self.remaining_scheduled_interest()?;
        Ok(PaymentQuote {
            loan: self.id,
            mode: RepaymentMode::Early,
            principal: self.remaining_principal,
            interest: interest_due,
            borrower_interest_reduction,
            lender_projection_reduction: Amount::ZERO,
            collateral_release: self.collateral_locked,
            closes_loan: true,
        })
    }

    pub fn quote_partial_repayment(
        &self,
        now: Epoch,
        principal: Amount,
        epochs_per_year: u64,
        policy: CollateralPolicy,
    ) -> KeystoneResult<PaymentQuote> {
        if !self.status.can_repay() {
            return Err(KeystoneError::LoanStatusMismatch);
        }
        if principal.is_zero() {
            return Err(KeystoneError::ZeroAmount);
        }
        if principal > self.remaining_principal {
            return Err(KeystoneError::RepaymentTooLarge);
        }
        if principal == self.remaining_principal {
            return self.quote_early_repayment(now, epochs_per_year);
        }
        let accrued = self.accrued_interest(now, epochs_per_year)?;
        let earned_remaining = accrued.checked_sub(self.interest_paid.min(accrued))?;
        let interest = earned_remaining
            .proportion_ceil(principal.as_u128(), self.remaining_principal.as_u128())?;
        let scheduled_remaining = self.remaining_scheduled_interest()?;
        let borrower_interest_reduction = scheduled_remaining
            .proportion_ceil(principal.as_u128(), self.remaining_principal.as_u128())?;
        let remaining_after = self.remaining_principal.checked_sub(principal)?;
        let retained_collateral = self.collateral_locked.proportion_ceil(
            remaining_after.as_u128(),
            self.remaining_principal.as_u128(),
        )?;
        let raw_release = self.collateral_locked.checked_sub(retained_collateral)?;
        let release = raw_release.min(policy.releasable_after_repay(
            self.collateral_locked,
            self.remaining_principal,
            remaining_after,
        )?);
        Ok(PaymentQuote {
            loan: self.id,
            mode: RepaymentMode::Partial,
            principal,
            interest,
            borrower_interest_reduction,
            lender_projection_reduction: Amount::ZERO,
            collateral_release: release,
            closes_loan: false,
        })
    }

    pub fn apply_repayment(&mut self, quote: PaymentQuote) -> KeystoneResult<()> {
        if quote.loan != self.id {
            return Err(KeystoneError::LoanNotFound);
        }
        if quote.principal > self.remaining_principal {
            return Err(KeystoneError::RepaymentTooLarge);
        }
        if quote.interest > self.remaining_scheduled_interest()? {
            return Err(KeystoneError::RepaymentTooLarge);
        }
        if quote.collateral_release > self.collateral_locked {
            return Err(KeystoneError::InsufficientCollateral);
        }
        self.remaining_principal = self.remaining_principal.checked_sub(quote.principal)?;
        self.interest_paid = self.interest_paid.checked_add(quote.interest)?;
        self.borrower_interest_released = self
            .borrower_interest_released
            .checked_add(quote.borrower_interest_reduction)?;
        self.collateral_locked = self
            .collateral_locked
            .checked_sub(quote.collateral_release)?;
        self.collateral_released = self
            .collateral_released
            .checked_add(quote.collateral_release)?;
        if self.remaining_principal.is_zero() && quote.closes_loan {
            self.status = LoanStatus::Paid;
            self.borrower_interest_released = self.scheduled_interest;
        }
        Ok(())
    }

    pub fn mark_defaulted(&mut self, now: Epoch) -> KeystoneResult<u64> {
        if !self.status.can_default() {
            return Err(KeystoneError::LoanStatusMismatch);
        }
        if !self.terms.schedule.is_overdue(now)? {
            return Err(KeystoneError::LoanNotDue);
        }
        let due = self.terms.schedule.due_epoch()?;
        let overdue = now.checked_sub(due)?;
        self.status = LoanStatus::Defaulted;
        self.default_epoch = Some(now);
        Ok(overdue)
    }

    pub fn begin_liquidation(&mut self) -> KeystoneResult<()> {
        if !self.status.can_liquidate() {
            return Err(KeystoneError::LiquidationNotAllowed);
        }
        self.status = LoanStatus::Liquidating;
        Ok(())
    }

    pub fn complete_liquidation(
        &mut self,
        principal_closed: Amount,
        interest_closed: Amount,
        collateral_seized: Amount,
    ) -> KeystoneResult<()> {
        if !matches!(self.status, LoanStatus::Liquidating | LoanStatus::Defaulted) {
            return Err(KeystoneError::LiquidationNotAllowed);
        }
        if principal_closed > self.remaining_principal {
            return Err(KeystoneError::RepaymentTooLarge);
        }
        if interest_closed > self.remaining_scheduled_interest()? {
            return Err(KeystoneError::RepaymentTooLarge);
        }
        if collateral_seized > self.collateral_locked {
            return Err(KeystoneError::InsufficientCollateral);
        }
        self.remaining_principal = self.remaining_principal.checked_sub(principal_closed)?;
        self.borrower_interest_released = self
            .borrower_interest_released
            .checked_add(interest_closed)?;
        self.collateral_locked = self.collateral_locked.checked_sub(collateral_seized)?;
        if self.remaining_principal.is_zero() {
            self.status = LoanStatus::Liquidated;
            self.borrower_interest_released = self.scheduled_interest;
        } else {
            self.status = LoanStatus::Defaulted;
        }
        Ok(())
    }

    pub fn write_off(&mut self) -> KeystoneResult<(Amount, Amount)> {
        if !matches!(self.status, LoanStatus::Defaulted | LoanStatus::Liquidating) {
            return Err(KeystoneError::LoanStatusMismatch);
        }
        let principal = self.remaining_principal;
        let interest = self.remaining_scheduled_interest()?;
        self.remaining_principal = Amount::ZERO;
        self.borrower_interest_released = self.scheduled_interest;
        self.collateral_locked = Amount::ZERO;
        self.status = LoanStatus::WrittenOff;
        Ok((principal, interest))
    }

    pub fn snapshot(&self) -> KeystoneResult<LoanSnapshot> {
        Ok(LoanSnapshot {
            loan: self.id,
            lender: self.terms.lender,
            borrower: self.terms.borrower,
            status: self.status,
            principal: self.terms.principal,
            remaining_principal: self.remaining_principal,
            scheduled_interest: self.scheduled_interest,
            interest_paid: self.interest_paid,
            borrower_interest_released: self.borrower_interest_released,
            collateral_locked: self.collateral_locked,
            collateral_released: self.collateral_released,
            start_epoch: self.terms.schedule.start,
            maturity_epoch: self.terms.schedule.maturity,
            due_epoch: self.terms.schedule.due_epoch()?,
            annual_rate_bps: self.terms.annual_rate_bps,
            terms_digest: self.terms_digest,
        })
    }

    pub fn digest(&self) -> KeystoneResult<Digest> {
        let snapshot = self.snapshot()?;
        let bytes = serde_json::to_vec(&snapshot)
            .map_err(|error| KeystoneError::serialization(error.to_string()))?;
        Ok(Digest::from_parts("keystonexdtl-loan-state-v1", &[&bytes]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetId, ProtocolPolicy};

    fn loan() -> LoanState {
        let asset = AssetId::native();
        let lender = VaultId::named("lender", asset);
        let borrower = VaultId::named("borrower", asset);
        let policy = ProtocolPolicy::default_prime().unwrap();
        let schedule = Schedule::new(Epoch(10), 90, 3, 360).unwrap();
        let terms = LoanTerms::new(LoanTermsInput {
            network_id: 42,
            lender,
            borrower,
            principal: Amount(10_000),
            collateral: Amount(12_000),
            annual_rate_bps: Bps::strict(1_200).unwrap(),
            schedule,
            policy_digest: policy.digest().unwrap(),
            nonce: 1,
        })
        .unwrap();
        LoanState::activate(terms, Amount(300)).unwrap()
    }

    #[test]
    fn scheduled_repayment_closes_balance() {
        let mut loan = loan();
        let quote = loan.quote_scheduled_repayment().unwrap();
        loan.apply_repayment(quote).unwrap();
        assert_eq!(loan.status(), LoanStatus::Paid);
        assert_eq!(loan.remaining_principal(), Amount::ZERO);
    }

    #[test]
    fn partial_repayment_reduces_principal() {
        let mut loan = loan();
        let policy = CollateralPolicy::for_tier(crate::RiskTier::Prime).unwrap();
        let quote = loan
            .quote_partial_repayment(Epoch(20), Amount(2_000), 360, policy)
            .unwrap();
        loan.apply_repayment(quote).unwrap();
        assert_eq!(loan.remaining_principal(), Amount(8_000));
    }

    #[test]
    fn overdue_loan_can_default() {
        let mut loan = loan();
        let overdue = loan.mark_defaulted(Epoch(104)).unwrap();
        assert_eq!(overdue, 1);
        assert_eq!(loan.status(), LoanStatus::Defaulted);
    }
}
