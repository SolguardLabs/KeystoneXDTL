use serde::{Deserialize, Serialize};

use crate::{Amount, Bps, Digest, KeystoneError, KeystoneResult};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    Prime,
    Standard,
    Watch,
    Restricted,
}

impl RiskTier {
    pub fn base_haircut(self) -> KeystoneResult<Bps> {
        match self {
            RiskTier::Prime => Bps::strict(500),
            RiskTier::Standard => Bps::strict(1_000),
            RiskTier::Watch => Bps::strict(1_750),
            RiskTier::Restricted => Bps::strict(3_500),
        }
    }

    pub fn max_ltv(self) -> KeystoneResult<Bps> {
        match self {
            RiskTier::Prime => Bps::strict(8_500),
            RiskTier::Standard => Bps::strict(7_000),
            RiskTier::Watch => Bps::strict(5_500),
            RiskTier::Restricted => Bps::strict(3_000),
        }
    }

    pub fn liquidation_bonus(self) -> KeystoneResult<Bps> {
        match self {
            RiskTier::Prime => Bps::strict(300),
            RiskTier::Standard => Bps::strict(500),
            RiskTier::Watch => Bps::strict(800),
            RiskTier::Restricted => Bps::strict(1_250),
        }
    }

    pub fn admits_new_credit(self) -> bool {
        !matches!(self, RiskTier::Restricted)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterestModel {
    pub base_rate_bps: Bps,
    pub utilization_slope_bps: Bps,
    pub reserve_factor_bps: Bps,
    pub overdue_penalty_bps: Bps,
    pub epochs_per_year: u64,
}

impl InterestModel {
    pub fn conservative() -> KeystoneResult<Self> {
        Ok(Self {
            base_rate_bps: Bps::strict(900)?,
            utilization_slope_bps: Bps::strict(1_800)?,
            reserve_factor_bps: Bps::strict(1_000)?,
            overdue_penalty_bps: Bps::strict(700)?,
            epochs_per_year: 360,
        })
    }

    pub fn stable() -> KeystoneResult<Self> {
        Ok(Self {
            base_rate_bps: Bps::strict(1_200)?,
            utilization_slope_bps: Bps::strict(2_400)?,
            reserve_factor_bps: Bps::strict(800)?,
            overdue_penalty_bps: Bps::strict(900)?,
            epochs_per_year: 360,
        })
    }

    pub fn quoted_rate(self, utilization_bps: Bps, risk_tier: RiskTier) -> KeystoneResult<Bps> {
        let utilization_component = Amount(utilization_bps.raw() as u64);
        let slope = self
            .utilization_slope_bps
            .apply_floor(utilization_component)?;
        let risk_addon = match risk_tier {
            RiskTier::Prime => 0,
            RiskTier::Standard => 150,
            RiskTier::Watch => 450,
            RiskTier::Restricted => 1_000,
        };
        self.base_rate_bps
            .checked_add(Bps::new(slope.raw() as u32)?)?
            .checked_add(Bps::new(risk_addon)?)
    }

    pub fn projected_interest(
        self,
        principal: Amount,
        annual_rate: Bps,
        tenor_epochs: u64,
    ) -> KeystoneResult<Amount> {
        Bps::annualized_for_epochs_ceil(annual_rate, principal, tenor_epochs, self.epochs_per_year)
    }

    pub fn accrued_interest(
        self,
        principal: Amount,
        annual_rate: Bps,
        elapsed_epochs: u64,
    ) -> KeystoneResult<Amount> {
        Bps::annualized_for_epochs_ceil(
            annual_rate,
            principal,
            elapsed_epochs,
            self.epochs_per_year,
        )
    }

    pub fn reserve_cut(self, interest: Amount) -> KeystoneResult<Amount> {
        self.reserve_factor_bps.apply_floor(interest)
    }

    pub fn lender_cut(self, interest: Amount) -> KeystoneResult<Amount> {
        interest.checked_sub(self.reserve_cut(interest)?)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollateralPolicy {
    pub tier: RiskTier,
    pub min_collateral_bps: Bps,
    pub maintenance_bps: Bps,
    pub release_buffer_bps: Bps,
    pub max_single_loan_bps: Bps,
}

impl CollateralPolicy {
    pub fn for_tier(tier: RiskTier) -> KeystoneResult<Self> {
        let min_collateral_bps = match tier {
            RiskTier::Prime => Bps::new(11_800)?,
            RiskTier::Standard => Bps::new(14_500)?,
            RiskTier::Watch => Bps::new(18_000)?,
            RiskTier::Restricted => Bps::new(26_000)?,
        };
        let maintenance_bps = match tier {
            RiskTier::Prime => Bps::new(10_700)?,
            RiskTier::Standard => Bps::new(12_800)?,
            RiskTier::Watch => Bps::new(16_200)?,
            RiskTier::Restricted => Bps::new(22_000)?,
        };
        Ok(Self {
            tier,
            min_collateral_bps,
            maintenance_bps,
            release_buffer_bps: Bps::strict(500)?,
            max_single_loan_bps: tier.max_ltv()?,
        })
    }

    pub fn required_for_principal(self, principal: Amount) -> KeystoneResult<Amount> {
        self.min_collateral_bps.apply_ceil(principal)
    }

    pub fn maintenance_for_debt(self, debt: Amount) -> KeystoneResult<Amount> {
        self.maintenance_bps.apply_ceil(debt)
    }

    pub fn releasable_after_repay(
        self,
        original_collateral: Amount,
        original_principal: Amount,
        remaining_principal: Amount,
    ) -> KeystoneResult<Amount> {
        if original_principal.is_zero() || remaining_principal >= original_principal {
            return Ok(Amount::ZERO);
        }
        let retained = original_collateral
            .proportion_ceil(remaining_principal.as_u128(), original_principal.as_u128())?;
        original_collateral.checked_sub(retained)
    }

    pub fn check_opening(
        self,
        principal: Amount,
        collateral: Amount,
        lender_nav: Amount,
    ) -> KeystoneResult<()> {
        if !self.tier.admits_new_credit() {
            return Err(KeystoneError::RiskTierBlocked);
        }
        let required = self.required_for_principal(principal)?;
        if collateral < required {
            return Err(KeystoneError::InsufficientCollateral);
        }
        let max_single = self.max_single_loan_bps.apply_floor(lender_nav)?;
        if principal > max_single {
            return Err(KeystoneError::LimitExceeded);
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitPolicy {
    pub max_utilization_bps: Bps,
    pub max_borrower_debt_bps: Bps,
    pub max_vaults: usize,
    pub max_active_loans: usize,
    pub redemption_liquidity_floor_bps: Bps,
}

impl LimitPolicy {
    pub fn institutional() -> KeystoneResult<Self> {
        Ok(Self {
            max_utilization_bps: Bps::strict(8_500)?,
            max_borrower_debt_bps: Bps::strict(7_500)?,
            max_vaults: 64,
            max_active_loans: 256,
            redemption_liquidity_floor_bps: Bps::strict(800)?,
        })
    }

    pub fn check_utilization(self, borrowed: Amount, nav: Amount) -> KeystoneResult<()> {
        if nav.is_zero() {
            return Err(KeystoneError::InsufficientLiquidity);
        }
        let max = self.max_utilization_bps.apply_floor(nav)?;
        if borrowed > max {
            return Err(KeystoneError::LimitExceeded);
        }
        Ok(())
    }

    pub fn check_borrower_debt(self, debt: Amount, nav: Amount) -> KeystoneResult<()> {
        if nav.is_zero() {
            return Err(KeystoneError::InsufficientCollateral);
        }
        let max = self.max_borrower_debt_bps.apply_floor(nav)?;
        if debt > max {
            return Err(KeystoneError::LimitExceeded);
        }
        Ok(())
    }

    pub fn cash_floor(self, nav: Amount) -> KeystoneResult<Amount> {
        self.redemption_liquidity_floor_bps.apply_floor(nav)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolPolicy {
    pub collateral: CollateralPolicy,
    pub interest: InterestModel,
    pub limits: LimitPolicy,
    pub liquidation_close_factor_bps: Bps,
    pub protocol_fee_bps: Bps,
}

impl ProtocolPolicy {
    pub fn default_prime() -> KeystoneResult<Self> {
        Ok(Self {
            collateral: CollateralPolicy::for_tier(RiskTier::Prime)?,
            interest: InterestModel::stable()?,
            limits: LimitPolicy::institutional()?,
            liquidation_close_factor_bps: Bps::strict(5_000)?,
            protocol_fee_bps: Bps::strict(50)?,
        })
    }

    pub fn for_tier(tier: RiskTier) -> KeystoneResult<Self> {
        Ok(Self {
            collateral: CollateralPolicy::for_tier(tier)?,
            interest: InterestModel::stable()?,
            limits: LimitPolicy::institutional()?,
            liquidation_close_factor_bps: Bps::strict(5_000)?,
            protocol_fee_bps: Bps::strict(50)?,
        })
    }

    pub fn digest(self) -> KeystoneResult<Digest> {
        let bytes = serde_json::to_vec(&self)
            .map_err(|error| KeystoneError::serialization(error.to_string()))?;
        Ok(Digest::from_parts("keystonexdtl-policy-v1", &[&bytes]))
    }

    pub fn protocol_fee(self, interest: Amount) -> KeystoneResult<Amount> {
        self.protocol_fee_bps.apply_floor(interest)
    }

    pub fn distributable_interest(self, interest: Amount) -> KeystoneResult<Amount> {
        interest.checked_sub(self.protocol_fee(interest)?)
    }

    pub fn check_open_loan(
        self,
        principal: Amount,
        collateral: Amount,
        lender_nav: Amount,
        borrower_debt_after: Amount,
        borrower_nav: Amount,
    ) -> KeystoneResult<()> {
        self.collateral
            .check_opening(principal, collateral, lender_nav)?;
        self.limits
            .check_borrower_debt(borrower_debt_after, borrower_nav.max(Amount::ONE))?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PolicySnapshot {
    pub digest: Digest,
    pub risk_tier: RiskTier,
    pub min_collateral_bps: Bps,
    pub maintenance_bps: Bps,
    pub max_utilization_bps: Bps,
    pub reserve_factor_bps: Bps,
    pub protocol_fee_bps: Bps,
}

impl PolicySnapshot {
    pub fn from_policy(policy: ProtocolPolicy) -> KeystoneResult<Self> {
        Ok(Self {
            digest: policy.digest()?,
            risk_tier: policy.collateral.tier,
            min_collateral_bps: policy.collateral.min_collateral_bps,
            maintenance_bps: policy.collateral.maintenance_bps,
            max_utilization_bps: policy.limits.max_utilization_bps,
            reserve_factor_bps: policy.interest.reserve_factor_bps,
            protocol_fee_bps: policy.protocol_fee_bps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collateral_policy_scales_with_principal() {
        let policy = CollateralPolicy::for_tier(RiskTier::Standard).unwrap();
        assert_eq!(
            policy.required_for_principal(Amount(10_000)).unwrap(),
            Amount(14_500)
        );
    }

    #[test]
    fn policy_digest_is_stable_for_equal_values() {
        let first = ProtocolPolicy::default_prime().unwrap().digest().unwrap();
        let second = ProtocolPolicy::default_prime().unwrap().digest().unwrap();
        assert_eq!(first, second);
    }
}
