use serde::{Deserialize, Serialize};

use crate::{Amount, Bps, Epoch, KeystoneError, KeystoneResult, alm::LiquidityStressReport};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    UtilizationLimit,
    BorrowerLimit,
    CashFloor,
    CollateralRequirement,
    ReserveFactor,
    LiquidationCloseFactor,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskChange {
    pub kind: ChangeKind,
    pub current_bps: Bps,
    pub proposed_bps: Bps,
}

impl RiskChange {
    pub fn absolute_delta_bps(self) -> u32 {
        self.current_bps.raw().abs_diff(self.proposed_bps.raw())
    }

    pub fn loosens_risk(self) -> bool {
        match self.kind {
            ChangeKind::UtilizationLimit
            | ChangeKind::BorrowerLimit
            | ChangeKind::LiquidationCloseFactor => {
                self.proposed_bps.raw() > self.current_bps.raw()
            }
            ChangeKind::CashFloor
            | ChangeKind::CollateralRequirement
            | ChangeKind::ReserveFactor => self.proposed_bps.raw() < self.current_bps.raw(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceGuardrails {
    pub quorum_bps: Bps,
    pub approval_bps: Bps,
    pub timelock_epochs: u64,
    pub max_change_bps: u32,
    pub minimum_post_change_coverage_bps: Bps,
}

impl GovernanceGuardrails {
    pub fn institutional() -> KeystoneResult<Self> {
        Ok(Self {
            quorum_bps: Bps::strict(6_000)?,
            approval_bps: Bps::strict(6_667)?,
            timelock_epochs: 2,
            max_change_bps: 1_500,
            minimum_post_change_coverage_bps: Bps::new(11_000)?,
        })
    }

    pub fn validate(self) -> KeystoneResult<Self> {
        if self.timelock_epochs == 0 || self.max_change_bps == 0 {
            return Err(KeystoneError::configuration(
                "governance guardrails require a timelock and a bounded delta",
            ));
        }
        if self.quorum_bps.raw() > 10_000
            || self.approval_bps.raw() > 10_000
            || self.minimum_post_change_coverage_bps.raw() < 10_000
        {
            return Err(KeystoneError::configuration(
                "governance basis points are outside the admitted range",
            ));
        }
        Ok(self)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteTally {
    pub eligible_power: Amount,
    pub for_power: Amount,
    pub against_power: Amount,
    pub abstain_power: Amount,
}

impl VoteTally {
    pub fn participating_power(self) -> KeystoneResult<Amount> {
        Amount::checked_sum([self.for_power, self.against_power, self.abstain_power])
    }

    pub fn decisive_power(self) -> KeystoneResult<Amount> {
        self.for_power.checked_add(self.against_power)
    }

    pub fn quorum_bps(self) -> KeystoneResult<Bps> {
        ratio(self.participating_power()?, self.eligible_power)
    }

    pub fn approval_bps(self) -> KeystoneResult<Bps> {
        ratio(self.for_power, self.decisive_power()?)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Review,
    Rejected,
    Queued,
    Executable,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskProposal {
    pub sequence: u64,
    pub submitted_at: Epoch,
    pub expires_at: Epoch,
    pub changes: Vec<RiskChange>,
    pub rationale_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalAssessment {
    pub status: ProposalStatus,
    pub executable_at: Option<Epoch>,
    pub quorum_bps: Bps,
    pub approval_bps: Bps,
    pub largest_delta_bps: u32,
    pub loosening_changes: usize,
    pub stress_passes: bool,
    pub reasons: Vec<String>,
}

impl RiskProposal {
    pub fn new(
        sequence: u64,
        submitted_at: Epoch,
        lifetime_epochs: u64,
        changes: Vec<RiskChange>,
        rationale: &str,
    ) -> KeystoneResult<Self> {
        if sequence == 0
            || lifetime_epochs == 0
            || changes.is_empty()
            || rationale.trim().is_empty()
        {
            return Err(KeystoneError::configuration(
                "proposal metadata and change set must be non-empty",
            ));
        }
        let expires_at = submitted_at.checked_add(lifetime_epochs)?;
        let rationale_digest = blake3::hash(rationale.as_bytes()).to_hex().to_string();
        Ok(Self {
            sequence,
            submitted_at,
            expires_at,
            changes,
            rationale_digest,
        })
    }

    pub fn assess(
        &self,
        now: Epoch,
        votes: VoteTally,
        stress: &LiquidityStressReport,
        guardrails: GovernanceGuardrails,
    ) -> KeystoneResult<ProposalAssessment> {
        let guardrails = guardrails.validate()?;
        let quorum_bps = votes.quorum_bps()?;
        let approval_bps = votes.approval_bps()?;
        let largest_delta_bps = self
            .changes
            .iter()
            .map(|change| change.absolute_delta_bps())
            .max()
            .unwrap_or_default();
        let loosening_changes = self
            .changes
            .iter()
            .filter(|change| change.loosens_risk())
            .count();
        let stress_passes = stress.passes()
            && stress.coverage_bps.raw() >= guardrails.minimum_post_change_coverage_bps.raw();
        let executable_at = self.submitted_at.checked_add(guardrails.timelock_epochs)?;
        let mut reasons = Vec::new();
        if now > self.expires_at {
            reasons.push("proposal_expired".into());
        }
        if quorum_bps.raw() < guardrails.quorum_bps.raw() {
            reasons.push("quorum_not_met".into());
        }
        if approval_bps.raw() < guardrails.approval_bps.raw() {
            reasons.push("approval_not_met".into());
        }
        if largest_delta_bps > guardrails.max_change_bps {
            reasons.push("parameter_delta_exceeded".into());
        }
        if loosening_changes > 0 && !stress_passes {
            reasons.push("stress_gate_failed".into());
        }

        let status = if now > self.expires_at {
            ProposalStatus::Expired
        } else if !reasons.is_empty() {
            ProposalStatus::Rejected
        } else if now < executable_at {
            ProposalStatus::Queued
        } else {
            ProposalStatus::Executable
        };
        Ok(ProposalAssessment {
            status,
            executable_at: Some(executable_at),
            quorum_bps,
            approval_bps,
            largest_delta_bps,
            loosening_changes,
            stress_passes,
            reasons,
        })
    }
}

fn ratio(numerator: Amount, denominator: Amount) -> KeystoneResult<Bps> {
    if denominator.is_zero() {
        return Err(KeystoneError::DivisionByZero);
    }
    let raw = numerator
        .as_u128()
        .checked_mul(10_000)
        .ok_or(KeystoneError::AmountOverflow)?
        / denominator.as_u128();
    Bps::new(raw.min(100_000) as u32)
}

#[cfg(test)]
mod tests {
    use crate::{Amount, Bps, Epoch, alm::StressScenario};

    use super::{
        ChangeKind, GovernanceGuardrails, ProposalStatus, RiskChange, RiskProposal, VoteTally,
    };

    fn empty_report() -> crate::alm::LiquidityStressReport {
        crate::alm::LiquidityStressReport {
            scenario: StressScenario::severe().unwrap(),
            rows: vec![],
            resources: Amount(2_000),
            uses: Amount(1_000),
            surplus: Amount(1_000),
            shortfall: Amount::ZERO,
            coverage_bps: Bps(20_000),
            breached_vaults: 0,
        }
    }

    fn votes() -> VoteTally {
        VoteTally {
            eligible_power: Amount(1_000),
            for_power: Amount(700),
            against_power: Amount(100),
            abstain_power: Amount(50),
        }
    }

    #[test]
    fn accepted_change_observes_quorum_approval_and_timelock() {
        let proposal = RiskProposal::new(
            12,
            Epoch(100),
            20,
            vec![RiskChange {
                kind: ChangeKind::CashFloor,
                current_bps: Bps(1_500),
                proposed_bps: Bps(1_800),
            }],
            "Increase the operating liquidity floor",
        )
        .unwrap();
        let queued = proposal
            .assess(
                Epoch(101),
                votes(),
                &empty_report(),
                GovernanceGuardrails::institutional().unwrap(),
            )
            .unwrap();
        assert_eq!(queued.status, ProposalStatus::Queued);
        let executable = proposal
            .assess(
                Epoch(102),
                votes(),
                &empty_report(),
                GovernanceGuardrails::institutional().unwrap(),
            )
            .unwrap();
        assert_eq!(executable.status, ProposalStatus::Executable);
        assert!(executable.reasons.is_empty());
    }

    #[test]
    fn loosening_change_is_rejected_when_stress_gate_fails() {
        let proposal = RiskProposal::new(
            13,
            Epoch(100),
            20,
            vec![RiskChange {
                kind: ChangeKind::UtilizationLimit,
                current_bps: Bps(7_000),
                proposed_bps: Bps(7_500),
            }],
            "Expand utilization after treasury review",
        )
        .unwrap();
        let mut failed = empty_report();
        failed.shortfall = Amount(1);
        let assessment = proposal
            .assess(
                Epoch(110),
                votes(),
                &failed,
                GovernanceGuardrails::institutional().unwrap(),
            )
            .unwrap();
        assert_eq!(assessment.status, ProposalStatus::Rejected);
        assert!(assessment.reasons.contains(&"stress_gate_failed".into()));
    }

    #[test]
    fn large_parameter_jump_is_rejected() {
        let proposal = RiskProposal::new(
            14,
            Epoch(100),
            20,
            vec![RiskChange {
                kind: ChangeKind::ReserveFactor,
                current_bps: Bps(2_500),
                proposed_bps: Bps(500),
            }],
            "Reserve calibration",
        )
        .unwrap();
        let assessment = proposal
            .assess(
                Epoch(110),
                votes(),
                &empty_report(),
                GovernanceGuardrails::institutional().unwrap(),
            )
            .unwrap();
        assert_eq!(assessment.status, ProposalStatus::Rejected);
        assert!(
            assessment
                .reasons
                .contains(&"parameter_delta_exceeded".into())
        );
    }
}
