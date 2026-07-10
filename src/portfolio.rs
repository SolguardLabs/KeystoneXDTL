use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    Amount, Bps, EngineSnapshot, Epoch, KeystoneResult, LoanStatus, VaultId,
    amount::checked_ratio_bps,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaturityBucketKind {
    Closed,
    Current,
    Short,
    Medium,
    Long,
    Overdue,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaturityBucket {
    pub principal: Amount,
    pub interest: Amount,
    pub count: usize,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExposureRow {
    pub vault: VaultId,
    pub lent_principal: Amount,
    pub borrowed_principal: Amount,
    pub expected_interest: Amount,
    pub locked_collateral: Amount,
    pub realized_interest: Amount,
    pub net_creditor_exposure: Amount,
    pub net_debtor_exposure: Amount,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioConcentration {
    pub largest_lender: Option<VaultId>,
    pub largest_lender_bps: Bps,
    pub largest_borrower: Option<VaultId>,
    pub largest_borrower_bps: Bps,
    pub total_lent: Amount,
    pub total_borrowed: Amount,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CounterpartyExposure {
    pub lender: VaultId,
    pub borrower: VaultId,
    pub open_principal: Amount,
    pub open_interest: Amount,
    pub collateral: Amount,
    pub loan_count: usize,
    pub overdue_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExposureMatrix {
    pub rows: Vec<CounterpartyExposure>,
    pub total_open_principal: Amount,
    pub total_open_interest: Amount,
    pub total_collateral: Amount,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioAnalytics {
    pub epoch: Epoch,
    pub buckets: BTreeMap<MaturityBucketKind, MaturityBucket>,
    pub exposures: BTreeMap<VaultId, ExposureRow>,
    pub concentration: PortfolioConcentration,
    pub matrix: ExposureMatrix,
}

impl MaturityBucket {
    pub fn add(&mut self, principal: Amount, interest: Amount) -> KeystoneResult<()> {
        self.principal = self.principal.checked_add(principal)?;
        self.interest = self.interest.checked_add(interest)?;
        self.count += 1;
        Ok(())
    }

    pub fn total(self) -> KeystoneResult<Amount> {
        self.principal.checked_add(self.interest)
    }
}

impl ExposureRow {
    pub fn new(vault: VaultId) -> Self {
        Self {
            vault,
            lent_principal: Amount::ZERO,
            borrowed_principal: Amount::ZERO,
            expected_interest: Amount::ZERO,
            locked_collateral: Amount::ZERO,
            realized_interest: Amount::ZERO,
            net_creditor_exposure: Amount::ZERO,
            net_debtor_exposure: Amount::ZERO,
        }
    }

    pub fn add_lending(
        &mut self,
        principal: Amount,
        expected_interest: Amount,
    ) -> KeystoneResult<()> {
        self.lent_principal = self.lent_principal.checked_add(principal)?;
        self.expected_interest = self.expected_interest.checked_add(expected_interest)?;
        self.recompute_net()
    }

    pub fn add_borrowing(&mut self, principal: Amount, collateral: Amount) -> KeystoneResult<()> {
        self.borrowed_principal = self.borrowed_principal.checked_add(principal)?;
        self.locked_collateral = self.locked_collateral.checked_add(collateral)?;
        self.recompute_net()
    }

    pub fn add_realized_interest(&mut self, amount: Amount) -> KeystoneResult<()> {
        self.realized_interest = self.realized_interest.checked_add(amount)?;
        Ok(())
    }

    pub fn recompute_net(&mut self) -> KeystoneResult<()> {
        if self.lent_principal >= self.borrowed_principal {
            self.net_creditor_exposure =
                self.lent_principal.checked_sub(self.borrowed_principal)?;
            self.net_debtor_exposure = Amount::ZERO;
        } else {
            self.net_debtor_exposure = self.borrowed_principal.checked_sub(self.lent_principal)?;
            self.net_creditor_exposure = Amount::ZERO;
        }
        Ok(())
    }

    pub fn collateralization_bps(self) -> KeystoneResult<Bps> {
        if self.borrowed_principal.is_zero() {
            return Bps::strict(10_000);
        }
        checked_ratio_bps(self.locked_collateral, self.borrowed_principal)
    }
}

impl PortfolioConcentration {
    pub fn from_exposures(exposures: &BTreeMap<VaultId, ExposureRow>) -> KeystoneResult<Self> {
        let total_lent = Amount::checked_sum(exposures.values().map(|row| row.lent_principal))?;
        let total_borrowed =
            Amount::checked_sum(exposures.values().map(|row| row.borrowed_principal))?;
        let mut largest_lender = None;
        let mut largest_lender_amount = Amount::ZERO;
        let mut largest_borrower = None;
        let mut largest_borrower_amount = Amount::ZERO;
        for (vault, row) in exposures {
            if row.lent_principal > largest_lender_amount {
                largest_lender = Some(*vault);
                largest_lender_amount = row.lent_principal;
            }
            if row.borrowed_principal > largest_borrower_amount {
                largest_borrower = Some(*vault);
                largest_borrower_amount = row.borrowed_principal;
            }
        }
        let largest_lender_bps = if total_lent.is_zero() {
            Bps::strict(0)?
        } else {
            checked_ratio_bps(largest_lender_amount, total_lent)?
        };
        let largest_borrower_bps = if total_borrowed.is_zero() {
            Bps::strict(0)?
        } else {
            checked_ratio_bps(largest_borrower_amount, total_borrowed)?
        };
        Ok(Self {
            largest_lender,
            largest_lender_bps,
            largest_borrower,
            largest_borrower_bps,
            total_lent,
            total_borrowed,
        })
    }
}

impl CounterpartyExposure {
    pub fn new(lender: VaultId, borrower: VaultId) -> Self {
        Self {
            lender,
            borrower,
            open_principal: Amount::ZERO,
            open_interest: Amount::ZERO,
            collateral: Amount::ZERO,
            loan_count: 0,
            overdue_count: 0,
        }
    }

    pub fn add_loan(
        &mut self,
        principal: Amount,
        interest: Amount,
        collateral: Amount,
        overdue: bool,
    ) -> KeystoneResult<()> {
        self.open_principal = self.open_principal.checked_add(principal)?;
        self.open_interest = self.open_interest.checked_add(interest)?;
        self.collateral = self.collateral.checked_add(collateral)?;
        self.loan_count += 1;
        if overdue {
            self.overdue_count += 1;
        }
        Ok(())
    }

    pub fn total_debt(self) -> KeystoneResult<Amount> {
        self.open_principal.checked_add(self.open_interest)
    }

    pub fn collateralization_bps(self) -> KeystoneResult<Bps> {
        let debt = self.total_debt()?;
        if debt.is_zero() {
            return Bps::strict(10_000);
        }
        checked_ratio_bps(self.collateral, debt)
    }

    pub fn has_open_risk(self) -> bool {
        !self.open_principal.is_zero() || !self.open_interest.is_zero()
    }
}

impl ExposureMatrix {
    pub fn from_loans(loans: &[crate::loan::LoanSnapshot], now: Epoch) -> KeystoneResult<Self> {
        let mut rows = BTreeMap::<(VaultId, VaultId), CounterpartyExposure>::new();
        let mut matrix = ExposureMatrix::default();
        for loan in loans {
            if loan.status.is_terminal() {
                continue;
            }
            let key = (loan.lender, loan.borrower);
            let remaining_interest = loan.scheduled_interest.saturating_sub(loan.interest_paid);
            let overdue = now > loan.due_epoch || matches!(loan.status, LoanStatus::Defaulted);
            rows.entry(key)
                .or_insert_with(|| CounterpartyExposure::new(loan.lender, loan.borrower))
                .add_loan(
                    loan.remaining_principal,
                    remaining_interest,
                    loan.collateral_locked,
                    overdue,
                )?;
            matrix.total_open_principal = matrix
                .total_open_principal
                .checked_add(loan.remaining_principal)?;
            matrix.total_open_interest =
                matrix.total_open_interest.checked_add(remaining_interest)?;
            matrix.total_collateral = matrix
                .total_collateral
                .checked_add(loan.collateral_locked)?;
        }
        matrix.rows = rows.into_values().collect();
        Ok(matrix)
    }

    pub fn pair(&self, lender: VaultId, borrower: VaultId) -> Option<CounterpartyExposure> {
        self.rows
            .iter()
            .copied()
            .find(|row| row.lender == lender && row.borrower == borrower)
    }

    pub fn pair_count(&self) -> usize {
        self.rows.len()
    }

    pub fn overdue_pair_count(&self) -> usize {
        self.rows.iter().filter(|row| row.overdue_count > 0).count()
    }

    pub fn largest_pair(&self) -> Option<CounterpartyExposure> {
        self.rows
            .iter()
            .copied()
            .max_by_key(|row| row.open_principal.raw())
    }

    pub fn collateralization_bps(&self) -> KeystoneResult<Bps> {
        let total_debt = self
            .total_open_principal
            .checked_add(self.total_open_interest)?;
        if total_debt.is_zero() {
            return Bps::strict(10_000);
        }
        checked_ratio_bps(self.total_collateral, total_debt)
    }

    pub fn concentration_bps(&self) -> KeystoneResult<Bps> {
        if self.total_open_principal.is_zero() {
            return Bps::strict(0);
        }
        let largest = self
            .largest_pair()
            .map(|row| row.open_principal)
            .unwrap_or(Amount::ZERO);
        checked_ratio_bps(largest, self.total_open_principal)
    }
}

impl PortfolioAnalytics {
    pub fn from_snapshot(snapshot: &EngineSnapshot) -> KeystoneResult<Self> {
        let mut buckets = BTreeMap::from([
            (MaturityBucketKind::Closed, MaturityBucket::default()),
            (MaturityBucketKind::Current, MaturityBucket::default()),
            (MaturityBucketKind::Short, MaturityBucket::default()),
            (MaturityBucketKind::Medium, MaturityBucket::default()),
            (MaturityBucketKind::Long, MaturityBucket::default()),
            (MaturityBucketKind::Overdue, MaturityBucket::default()),
        ]);
        let mut exposures = BTreeMap::new();
        for vault in &snapshot.vaults {
            let mut row = ExposureRow::new(vault.id);
            row.add_lending(vault.outstanding_principal, vault.expected_interest)?;
            row.add_borrowing(vault.debt_principal, vault.locked_collateral)?;
            row.add_realized_interest(vault.realized_interest)?;
            exposures.insert(vault.id, row);
        }
        for loan in &snapshot.loans {
            let kind = bucket_for_loan(loan, snapshot.epoch);
            let remaining_interest = loan.scheduled_interest.saturating_sub(loan.interest_paid);
            buckets
                .entry(kind)
                .or_default()
                .add(loan.remaining_principal, remaining_interest)?;
        }
        let concentration = PortfolioConcentration::from_exposures(&exposures)?;
        let matrix = ExposureMatrix::from_loans(&snapshot.loans, snapshot.epoch)?;
        Ok(Self {
            epoch: snapshot.epoch,
            buckets,
            exposures,
            concentration,
            matrix,
        })
    }

    pub fn from_engine(engine: &crate::KeystoneEngine) -> KeystoneResult<Self> {
        let snapshot = engine.snapshot()?;
        Self::from_snapshot(&snapshot)
    }

    pub fn bucket(&self, kind: MaturityBucketKind) -> MaturityBucket {
        self.buckets.get(&kind).copied().unwrap_or_default()
    }

    pub fn bucket_count(&self, kind: MaturityBucketKind) -> usize {
        self.bucket(kind).count
    }

    pub fn open_bucket_count(&self) -> usize {
        self.buckets
            .iter()
            .filter(|(kind, bucket)| **kind != MaturityBucketKind::Closed && bucket.count > 0)
            .count()
    }

    pub fn total_bucket_debt(&self, kind: MaturityBucketKind) -> KeystoneResult<Amount> {
        self.bucket(kind).total()
    }

    pub fn has_overdue(&self) -> bool {
        self.bucket(MaturityBucketKind::Overdue).count > 0 || self.matrix.overdue_pair_count() > 0
    }

    pub fn open_loan_count(&self) -> usize {
        self.buckets
            .iter()
            .filter(|(kind, _)| **kind != MaturityBucketKind::Closed)
            .map(|(_, bucket)| bucket.count)
            .sum()
    }

    pub fn closed_loan_count(&self) -> usize {
        self.bucket(MaturityBucketKind::Closed).count
    }

    pub fn has_active_credit(&self) -> bool {
        self.open_loan_count() > 0
    }

    pub fn has_concentrated_pair(&self, threshold: Bps) -> KeystoneResult<bool> {
        Ok(self.largest_pair_concentration_bps()? > threshold)
    }

    pub fn total_matrix_debt(&self) -> KeystoneResult<Amount> {
        self.matrix
            .total_open_principal
            .checked_add(self.matrix.total_open_interest)
    }

    pub fn total_open_principal(&self) -> KeystoneResult<Amount> {
        Amount::checked_sum(
            self.buckets
                .iter()
                .filter(|(kind, _)| **kind != MaturityBucketKind::Closed)
                .map(|(_, bucket)| bucket.principal),
        )
    }

    pub fn total_open_interest(&self) -> KeystoneResult<Amount> {
        Amount::checked_sum(
            self.buckets
                .iter()
                .filter(|(kind, _)| **kind != MaturityBucketKind::Closed)
                .map(|(_, bucket)| bucket.interest),
        )
    }

    pub fn exposure(&self, vault: VaultId) -> Option<ExposureRow> {
        self.exposures.get(&vault).copied()
    }

    pub fn lender_count(&self) -> usize {
        self.exposures
            .values()
            .filter(|row| !row.lent_principal.is_zero())
            .count()
    }

    pub fn borrower_count(&self) -> usize {
        self.exposures
            .values()
            .filter(|row| !row.borrowed_principal.is_zero())
            .count()
    }

    pub fn counterparty_pair_count(&self) -> usize {
        self.matrix.pair_count()
    }

    pub fn overdue_pair_count(&self) -> usize {
        self.matrix.overdue_pair_count()
    }

    pub fn largest_pair_concentration_bps(&self) -> KeystoneResult<Bps> {
        self.matrix.concentration_bps()
    }
}

fn bucket_for_loan(loan: &crate::loan::LoanSnapshot, now: Epoch) -> MaturityBucketKind {
    if loan.status.is_terminal() {
        return MaturityBucketKind::Closed;
    }
    if matches!(loan.status, LoanStatus::Defaulted | LoanStatus::Liquidating)
        || now > loan.due_epoch
    {
        return MaturityBucketKind::Overdue;
    }
    if now >= loan.maturity_epoch {
        return MaturityBucketKind::Current;
    }
    let remaining = loan.maturity_epoch.raw().saturating_sub(now.raw());
    if remaining <= 30 {
        MaturityBucketKind::Short
    } else if remaining <= 90 {
        MaturityBucketKind::Medium
    } else {
        MaturityBucketKind::Long
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccountId, Amount, AssetId, EngineConfig, KeystoneEngine, VaultRole};

    #[test]
    fn portfolio_tracks_open_loan_exposure() {
        let asset = AssetId::native();
        let mut engine = KeystoneEngine::new(EngineConfig::local(asset).unwrap()).unwrap();
        let lender = engine
            .register_vault("lender", VaultRole::Liquidity)
            .unwrap();
        let borrower = engine
            .register_vault("borrower", VaultRole::Borrower)
            .unwrap();
        engine
            .deposit(lender, AccountId::named("lp"), Amount(100_000))
            .unwrap();
        engine
            .deposit(borrower, AccountId::named("ops"), Amount(50_000))
            .unwrap();
        engine
            .open_loan(lender, borrower, Amount(10_000), Amount(12_000), 60)
            .unwrap();
        let analytics = PortfolioAnalytics::from_engine(&engine).unwrap();
        assert_eq!(analytics.lender_count(), 1);
        assert_eq!(analytics.borrower_count(), 1);
        assert_eq!(analytics.counterparty_pair_count(), 1);
        assert_eq!(analytics.open_loan_count(), 1);
        assert!(!analytics.has_overdue());
        assert_eq!(analytics.total_open_principal().unwrap(), Amount(10_000));
    }
}
