use serde::{Deserialize, Serialize};

use crate::{
    Amount, Bps, EngineSnapshot, KeystoneError, KeystoneResult, VaultId, amount::BPS_DENOMINATOR,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StressScenario {
    pub redemption_bps: Bps,
    pub repayment_delay_bps: Bps,
    pub collateral_haircut_bps: Bps,
    pub reserve_release_bps: Bps,
    pub minimum_coverage_bps: Bps,
}

impl StressScenario {
    pub fn operating() -> KeystoneResult<Self> {
        Ok(Self {
            redemption_bps: Bps::strict(800)?,
            repayment_delay_bps: Bps::strict(1_500)?,
            collateral_haircut_bps: Bps::strict(600)?,
            reserve_release_bps: Bps::strict(3_000)?,
            minimum_coverage_bps: Bps::new(10_500)?,
        })
    }

    pub fn severe() -> KeystoneResult<Self> {
        Ok(Self {
            redemption_bps: Bps::strict(2_500)?,
            repayment_delay_bps: Bps::strict(4_000)?,
            collateral_haircut_bps: Bps::strict(2_000)?,
            reserve_release_bps: Bps::strict(7_500)?,
            minimum_coverage_bps: Bps::new(11_000)?,
        })
    }

    pub fn validate(self) -> KeystoneResult<Self> {
        for value in [
            self.redemption_bps,
            self.repayment_delay_bps,
            self.collateral_haircut_bps,
            self.reserve_release_bps,
        ] {
            if value.raw() > 10_000 {
                return Err(KeystoneError::configuration(
                    "stress inputs must be bounded to 100 percent",
                ));
            }
        }
        if self.minimum_coverage_bps.raw() < 10_000 {
            return Err(KeystoneError::configuration(
                "minimum coverage must be at least 100 percent",
            ));
        }
        Ok(self)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidityResources {
    pub cash: Amount,
    pub collectible_receivables: Amount,
    pub releasable_reserve: Amount,
    pub total: Amount,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidityUses {
    pub stressed_redemptions: Amount,
    pub contractual_debt: Amount,
    pub collateral_impairment: Amount,
    pub total: Amount,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultStressRow {
    pub vault: VaultId,
    pub resources: LiquidityResources,
    pub uses: LiquidityUses,
    pub surplus: Amount,
    pub shortfall: Amount,
    pub coverage_bps: Bps,
    pub passes_floor: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidityStressReport {
    pub scenario: StressScenario,
    pub rows: Vec<VaultStressRow>,
    pub resources: Amount,
    pub uses: Amount,
    pub surplus: Amount,
    pub shortfall: Amount,
    pub coverage_bps: Bps,
    pub breached_vaults: usize,
}

impl LiquidityResources {
    fn new(cash: Amount, receivables: Amount, reserve: Amount) -> KeystoneResult<Self> {
        let total = Amount::checked_sum([cash, receivables, reserve])?;
        Ok(Self {
            cash,
            collectible_receivables: receivables,
            releasable_reserve: reserve,
            total,
        })
    }
}

impl LiquidityUses {
    fn new(redemptions: Amount, debt: Amount, impairment: Amount) -> KeystoneResult<Self> {
        let total = Amount::checked_sum([redemptions, debt, impairment])?;
        Ok(Self {
            stressed_redemptions: redemptions,
            contractual_debt: debt,
            collateral_impairment: impairment,
            total,
        })
    }
}

impl VaultStressRow {
    pub fn from_vault(
        vault: &crate::vault::VaultSnapshot,
        scenario: StressScenario,
    ) -> KeystoneResult<Self> {
        let scenario = scenario.validate()?;
        let receivables = vault
            .outstanding_principal
            .checked_add(vault.expected_interest)?;
        let collectible = scenario
            .repayment_delay_bps
            .complement()?
            .apply_floor(receivables)?;
        let releasable_reserve = scenario
            .reserve_release_bps
            .apply_floor(vault.loss_reserve)?;
        let resources = LiquidityResources::new(vault.cash, collectible, releasable_reserve)?;

        let redemptions = scenario.redemption_bps.apply_ceil(vault.nav)?;
        let debt = vault.debt_principal.checked_add(vault.debt_interest_due)?;
        let impairment = scenario
            .collateral_haircut_bps
            .apply_ceil(vault.locked_collateral)?;
        let uses = LiquidityUses::new(redemptions, debt, impairment)?;

        let (surplus, shortfall) = signed_gap(resources.total, uses.total)?;
        let coverage_bps = coverage(resources.total, uses.total)?;
        Ok(Self {
            vault: vault.id,
            resources,
            uses,
            surplus,
            shortfall,
            coverage_bps,
            passes_floor: shortfall.is_zero()
                && coverage_bps.raw() >= scenario.minimum_coverage_bps.raw(),
        })
    }
}

impl LiquidityStressReport {
    pub fn from_snapshot(
        snapshot: &EngineSnapshot,
        scenario: StressScenario,
    ) -> KeystoneResult<Self> {
        let scenario = scenario.validate()?;
        let rows = snapshot
            .vaults
            .iter()
            .map(|vault| VaultStressRow::from_vault(vault, scenario))
            .collect::<KeystoneResult<Vec<_>>>()?;
        let resources = Amount::checked_sum(rows.iter().map(|row| row.resources.total))?;
        let uses = Amount::checked_sum(rows.iter().map(|row| row.uses.total))?;
        let (surplus, shortfall) = signed_gap(resources, uses)?;
        let coverage_bps = coverage(resources, uses)?;
        let breached_vaults = rows.iter().filter(|row| !row.passes_floor).count();
        Ok(Self {
            scenario,
            rows,
            resources,
            uses,
            surplus,
            shortfall,
            coverage_bps,
            breached_vaults,
        })
    }

    pub fn from_engine(
        engine: &crate::KeystoneEngine,
        scenario: StressScenario,
    ) -> KeystoneResult<Self> {
        Self::from_snapshot(&engine.snapshot()?, scenario)
    }

    pub fn vault(&self, vault: VaultId) -> Option<VaultStressRow> {
        self.rows.iter().copied().find(|row| row.vault == vault)
    }

    pub fn passes(&self) -> bool {
        self.shortfall.is_zero()
            && self.breached_vaults == 0
            && self.coverage_bps.raw() >= self.scenario.minimum_coverage_bps.raw()
    }

    pub fn required_buffer(&self) -> KeystoneResult<Amount> {
        let required = Amount::from_u128(
            self.uses.as_u128() * self.scenario.minimum_coverage_bps.raw() as u128
                / BPS_DENOMINATOR,
        )?;
        Ok(required.saturating_sub(self.resources))
    }
}

fn signed_gap(assets: Amount, uses: Amount) -> KeystoneResult<(Amount, Amount)> {
    if assets >= uses {
        Ok((assets.checked_sub(uses)?, Amount::ZERO))
    } else {
        Ok((Amount::ZERO, uses.checked_sub(assets)?))
    }
}

fn coverage(resources: Amount, uses: Amount) -> KeystoneResult<Bps> {
    if uses.is_zero() {
        return Bps::new(100_000);
    }
    let value = resources
        .as_u128()
        .checked_mul(BPS_DENOMINATOR)
        .ok_or(KeystoneError::AmountOverflow)?
        / uses.as_u128();
    Bps::new(value.min(100_000) as u32)
}

#[cfg(test)]
mod tests {
    use crate::{Amount, Bps, Shares, VaultId, VaultRole, vault::VaultSnapshot};

    use super::{LiquidityStressReport, StressScenario, VaultStressRow};

    fn vault() -> VaultSnapshot {
        VaultSnapshot {
            id: VaultId::named("senior-treasury", crate::AssetId::native()),
            name: "Senior Treasury".into(),
            role: VaultRole::Hybrid,
            cash: Amount(600_000),
            share_supply: Shares(1_000_000),
            gross_assets: Amount(1_050_000),
            liabilities: Amount::ZERO,
            nav: Amount(1_050_000),
            outstanding_principal: Amount(400_000),
            expected_interest: Amount(50_000),
            realized_interest: Amount::ZERO,
            debt_principal: Amount(100_000),
            debt_interest_due: Amount(10_000),
            locked_collateral: Amount(160_000),
            loss_reserve: Amount(40_000),
            protocol_fees: Amount::ZERO,
            distributed_interest: Amount::ZERO,
            utilization_bps: Bps(3_810),
            holder_count: 4,
        }
    }

    #[test]
    fn operating_stress_builds_a_reconciled_waterfall() {
        let row =
            VaultStressRow::from_vault(&vault(), StressScenario::operating().unwrap()).unwrap();
        assert_eq!(row.resources.total, Amount(994_500));
        assert_eq!(row.uses.total, Amount(203_600));
        assert_eq!(row.surplus, Amount(790_900));
        assert_eq!(row.shortfall, Amount::ZERO);
        assert!(row.passes_floor);
    }

    #[test]
    fn severe_redemption_pressure_produces_a_bounded_report() {
        let mut stressed = vault();
        stressed.cash = Amount(40_000);
        stressed.outstanding_principal = Amount(20_000);
        stressed.expected_interest = Amount(2_000);
        stressed.nav = Amount(900_000);
        let snapshot = crate::EngineSnapshot {
            network_id: 1,
            epoch: crate::Epoch(10),
            policy_digest: crate::Digest::ZERO,
            oracle_digest: crate::Digest::ZERO,
            journal_digest: crate::Digest::ZERO,
            vaults: vec![stressed],
            loans: vec![],
        };
        let report =
            LiquidityStressReport::from_snapshot(&snapshot, StressScenario::severe().unwrap())
                .unwrap();
        assert!(!report.passes());
        assert_eq!(report.breached_vaults, 1);
        assert!(report.required_buffer().unwrap().raw() > 0);
    }

    #[test]
    fn invalid_percentage_is_rejected() {
        let scenario = StressScenario {
            redemption_bps: Bps(10_001),
            ..StressScenario::operating().unwrap()
        };
        assert!(scenario.validate().is_err());
    }
}
