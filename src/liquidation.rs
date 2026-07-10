use serde::{Deserialize, Serialize};

use crate::{
    Amount, Bps, KeystoneError, KeystoneResult, LoanId, LoanState, ProtocolPolicy, VaultId,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidationPlan {
    pub loan: LoanId,
    pub lender: VaultId,
    pub borrower: VaultId,
    pub debt: Amount,
    pub close_principal: Amount,
    pub close_interest: Amount,
    pub collateral_available: Amount,
    pub collateral_to_seize: Amount,
    pub liquidation_bonus: Amount,
    pub shortfall: Amount,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidationResult {
    pub loan: LoanId,
    pub seized: Amount,
    pub principal_closed: Amount,
    pub interest_closed: Amount,
    pub reserve_recovery: Amount,
    pub shortfall: Amount,
    pub remaining_principal: Amount,
}

impl LiquidationPlan {
    pub fn build(loan: &LoanState, policy: ProtocolPolicy) -> KeystoneResult<Self> {
        if !loan.status().can_liquidate() {
            return Err(KeystoneError::LiquidationNotAllowed);
        }
        let debt = loan.debt_due_for_borrower()?;
        let close_principal = policy
            .liquidation_close_factor_bps
            .apply_ceil(loan.remaining_principal())?
            .min(loan.remaining_principal());
        let interest_due = loan.remaining_scheduled_interest()?;
        let close_interest = policy
            .liquidation_close_factor_bps
            .apply_ceil(interest_due)?
            .min(interest_due);
        let close_debt = close_principal.checked_add(close_interest)?;
        let bonus_rate = policy.collateral.tier.liquidation_bonus()?;
        let liquidation_bonus = bonus_rate.apply_ceil(close_debt)?;
        let target_seizure = close_debt.checked_add(liquidation_bonus)?;
        let collateral_available = loan.collateral_locked();
        let collateral_to_seize = target_seizure.min(collateral_available);
        let shortfall = target_seizure.saturating_sub(collateral_to_seize);
        Ok(Self {
            loan: loan.id(),
            lender: loan.lender(),
            borrower: loan.borrower(),
            debt,
            close_principal,
            close_interest,
            collateral_available,
            collateral_to_seize,
            liquidation_bonus,
            shortfall,
        })
    }

    pub fn principal_covered(self) -> KeystoneResult<Amount> {
        if self.collateral_to_seize >= self.close_interest {
            self.collateral_to_seize
                .checked_sub(self.close_interest)?
                .min(self.close_principal)
                .pipe(Ok)
        } else {
            Ok(Amount::ZERO)
        }
    }

    pub fn interest_covered(self) -> Amount {
        self.collateral_to_seize.min(self.close_interest)
    }

    pub fn reserve_recovery(self) -> KeystoneResult<Amount> {
        let covered_debt = self
            .principal_covered()?
            .checked_add(self.interest_covered())?;
        Ok(self.collateral_to_seize.saturating_sub(covered_debt))
    }

    pub fn result(self, remaining_principal: Amount) -> KeystoneResult<LiquidationResult> {
        Ok(LiquidationResult {
            loan: self.loan,
            seized: self.collateral_to_seize,
            principal_closed: self.principal_covered()?,
            interest_closed: self.interest_covered(),
            reserve_recovery: self.reserve_recovery()?,
            shortfall: self.shortfall,
            remaining_principal,
        })
    }

    pub fn health_after(self, remaining_collateral: Amount) -> KeystoneResult<Bps> {
        let remaining_debt = self
            .debt
            .checked_sub(self.close_principal.checked_add(self.close_interest)?)?;
        if remaining_debt.is_zero() {
            return Bps::strict(10_000);
        }
        crate::amount::checked_ratio_bps(remaining_collateral, remaining_debt)
    }
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetId, Epoch, LoanTerms, LoanTermsInput, RiskTier, Schedule};

    #[test]
    fn liquidation_plan_seizes_collateral_with_bonus() {
        let asset = AssetId::native();
        let policy = ProtocolPolicy::for_tier(RiskTier::Standard).unwrap();
        let terms = LoanTerms::new(LoanTermsInput {
            network_id: 1,
            lender: VaultId::named("lender", asset),
            borrower: VaultId::named("borrower", asset),
            principal: Amount(10_000),
            collateral: Amount(14_500),
            annual_rate_bps: Bps::strict(1_200).unwrap(),
            schedule: Schedule::new(Epoch(0), 30, 1, 360).unwrap(),
            policy_digest: policy.digest().unwrap(),
            nonce: 9,
        })
        .unwrap();
        let mut loan = LoanState::activate(terms, Amount(100)).unwrap();
        loan.mark_defaulted(Epoch(32)).unwrap();
        let plan = LiquidationPlan::build(&loan, policy).unwrap();
        assert!(plan.collateral_to_seize.raw() > plan.close_principal.raw());
    }
}
