use serde::{Deserialize, Serialize};

use crate::{
    Amount, Bps, EngineSnapshot, Epoch, KeystoneError, KeystoneResult, LoanId, LoanStatus,
    ProtocolPolicy, VaultId, amount::checked_ratio_bps,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskGrade {
    Green,
    Yellow,
    Orange,
    Red,
    Closed,
}

impl RiskGrade {
    pub fn from_bps(value: Bps, green: u32, yellow: u32, orange: u32) -> Self {
        if value.raw() <= green {
            RiskGrade::Green
        } else if value.raw() <= yellow {
            RiskGrade::Yellow
        } else if value.raw() <= orange {
            RiskGrade::Orange
        } else {
            RiskGrade::Red
        }
    }

    pub fn ordinal(self) -> u8 {
        match self {
            RiskGrade::Green => 0,
            RiskGrade::Yellow => 1,
            RiskGrade::Orange => 2,
            RiskGrade::Red => 3,
            RiskGrade::Closed => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultRiskReport {
    pub vault: VaultId,
    pub name: String,
    pub liquidity_bps: Bps,
    pub utilization_bps: Bps,
    pub leverage_bps: Bps,
    pub collateral_buffer: Amount,
    pub grade: RiskGrade,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoanRiskReport {
    pub loan: LoanId,
    pub status: LoanStatus,
    pub progress_bps: Bps,
    pub collateralization_bps: Bps,
    pub remaining_principal: Amount,
    pub remaining_interest: Amount,
    pub overdue_epochs: u64,
    pub grade: RiskGrade,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskDashboard {
    pub epoch: Epoch,
    pub vaults: Vec<VaultRiskReport>,
    pub loans: Vec<LoanRiskReport>,
    pub max_vault_grade: RiskGrade,
    pub max_loan_grade: RiskGrade,
}

impl VaultRiskReport {
    pub fn from_snapshot(
        snapshot: &crate::vault::VaultSnapshot,
        policy: ProtocolPolicy,
    ) -> KeystoneResult<Self> {
        let liquidity_bps = if snapshot.nav.is_zero() {
            Bps::strict(0)?
        } else {
            checked_ratio_bps(snapshot.cash, snapshot.nav)?
        };
        let utilization_bps = snapshot.utilization_bps;
        let leverage_bps = if snapshot.nav.is_zero() {
            Bps::strict(0)?
        } else {
            checked_ratio_bps(snapshot.liabilities, snapshot.nav)?
        };
        let maintenance = policy.collateral.maintenance_for_debt(
            snapshot
                .debt_principal
                .checked_add(snapshot.debt_interest_due)?,
        )?;
        let collateral_buffer = snapshot.locked_collateral.saturating_sub(maintenance);
        let liquidity_grade = if liquidity_bps.raw() < 500 {
            RiskGrade::Red
        } else if liquidity_bps.raw() < 1_000 {
            RiskGrade::Orange
        } else if liquidity_bps.raw() < 2_000 {
            RiskGrade::Yellow
        } else {
            RiskGrade::Green
        };
        let utilization_grade = RiskGrade::from_bps(utilization_bps, 4_000, 7_000, 8_500);
        let leverage_grade = RiskGrade::from_bps(leverage_bps, 3_000, 6_000, 8_000);
        let grade = max_grade([liquidity_grade, utilization_grade, leverage_grade]);
        Ok(Self {
            vault: snapshot.id,
            name: snapshot.name.clone(),
            liquidity_bps,
            utilization_bps,
            leverage_bps,
            collateral_buffer,
            grade,
        })
    }
}

impl LoanRiskReport {
    pub fn from_snapshot(
        snapshot: &crate::loan::LoanSnapshot,
        now: Epoch,
        policy: ProtocolPolicy,
    ) -> KeystoneResult<Self> {
        if snapshot.status.is_terminal() {
            return Ok(Self {
                loan: snapshot.loan,
                status: snapshot.status,
                progress_bps: Bps::strict(10_000)?,
                collateralization_bps: Bps::strict(0)?,
                remaining_principal: snapshot.remaining_principal,
                remaining_interest: Amount::ZERO,
                overdue_epochs: 0,
                grade: RiskGrade::Closed,
            });
        }
        let tenor = snapshot.maturity_epoch.checked_sub(snapshot.start_epoch)?;
        let elapsed = if now <= snapshot.start_epoch {
            0
        } else {
            now.checked_sub(snapshot.start_epoch)?.min(tenor)
        };
        let progress_bps = if tenor == 0 {
            Bps::strict(10_000)?
        } else {
            Bps::new(((elapsed as u128) * 10_000 / tenor as u128) as u32)?
        };
        let remaining_interest = snapshot
            .scheduled_interest
            .checked_sub(snapshot.interest_paid)?;
        let debt = snapshot
            .remaining_principal
            .checked_add(remaining_interest)?;
        let collateralization_bps = if debt.is_zero() {
            Bps::strict(10_000)?
        } else {
            checked_ratio_bps(snapshot.collateral_locked, debt)?
        };
        let overdue_epochs = if now > snapshot.due_epoch {
            now.checked_sub(snapshot.due_epoch)?
        } else {
            0
        };
        let maintenance = policy.collateral.maintenance_bps;
        let grade = if overdue_epochs > 0 || matches!(snapshot.status, LoanStatus::Defaulted) {
            RiskGrade::Red
        } else if collateralization_bps.raw() < maintenance.raw() {
            RiskGrade::Orange
        } else if progress_bps.raw() > 8_500 {
            RiskGrade::Yellow
        } else {
            RiskGrade::Green
        };
        Ok(Self {
            loan: snapshot.loan,
            status: snapshot.status,
            progress_bps,
            collateralization_bps,
            remaining_principal: snapshot.remaining_principal,
            remaining_interest,
            overdue_epochs,
            grade,
        })
    }
}

impl RiskDashboard {
    pub fn from_snapshot(
        snapshot: &EngineSnapshot,
        policy: ProtocolPolicy,
    ) -> KeystoneResult<Self> {
        let mut vaults = Vec::with_capacity(snapshot.vaults.len());
        let mut loans = Vec::with_capacity(snapshot.loans.len());
        for vault in &snapshot.vaults {
            vaults.push(VaultRiskReport::from_snapshot(vault, policy)?);
        }
        for loan in &snapshot.loans {
            loans.push(LoanRiskReport::from_snapshot(loan, snapshot.epoch, policy)?);
        }
        let max_vault_grade =
            max_grade(vaults.iter().map(|report| report.grade).collect::<Vec<_>>());
        let max_loan_grade = max_grade(loans.iter().map(|report| report.grade).collect::<Vec<_>>());
        Ok(Self {
            epoch: snapshot.epoch,
            vaults,
            loans,
            max_vault_grade,
            max_loan_grade,
        })
    }

    pub fn from_engine(engine: &crate::KeystoneEngine) -> KeystoneResult<Self> {
        let snapshot = engine.snapshot()?;
        Self::from_snapshot(&snapshot, engine.policy())
    }

    pub fn red_items(&self) -> usize {
        self.vaults
            .iter()
            .filter(|item| item.grade == RiskGrade::Red)
            .count()
            + self
                .loans
                .iter()
                .filter(|item| item.grade == RiskGrade::Red)
                .count()
    }

    pub fn loan(&self, loan: LoanId) -> Option<&LoanRiskReport> {
        self.loans.iter().find(|report| report.loan == loan)
    }

    pub fn vault(&self, vault: VaultId) -> Option<&VaultRiskReport> {
        self.vaults.iter().find(|report| report.vault == vault)
    }

    pub fn assert_no_red(&self) -> KeystoneResult<()> {
        if self.red_items() > 0 {
            return Err(KeystoneError::invariant(
                "risk dashboard contains red items",
            ));
        }
        Ok(())
    }
}

fn max_grade(values: impl IntoIterator<Item = RiskGrade>) -> RiskGrade {
    let mut max = RiskGrade::Green;
    for value in values {
        if value.ordinal() > max.ordinal() {
            max = value;
        }
    }
    max
}

pub fn weighted_average_utilization(vaults: &[VaultRiskReport]) -> KeystoneResult<Bps> {
    if vaults.is_empty() {
        return Bps::strict(0);
    }
    let sum: u128 = vaults
        .iter()
        .map(|vault| vault.utilization_bps.raw() as u128)
        .sum();
    Bps::new((sum / vaults.len() as u128) as u32)
}

pub fn count_by_grade<T>(
    items: impl IntoIterator<Item = T>,
    grade_of: impl Fn(&T) -> RiskGrade,
) -> [usize; 5] {
    let mut counts = [0usize; 5];
    for item in items {
        counts[grade_of(&item).ordinal() as usize] += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccountId, Amount, AssetId, EngineConfig, KeystoneEngine, VaultRole};

    #[test]
    fn dashboard_scores_seeded_engine_green() {
        let asset = AssetId::native();
        let mut engine = KeystoneEngine::new(EngineConfig::local(asset).unwrap()).unwrap();
        let vault = engine.register_vault("risk", VaultRole::Hybrid).unwrap();
        engine
            .deposit(vault, AccountId::named("owner"), Amount(50_000))
            .unwrap();
        let dashboard = RiskDashboard::from_engine(&engine).unwrap();
        assert_eq!(dashboard.max_vault_grade, RiskGrade::Green);
        dashboard.assert_no_red().unwrap();
    }
}
