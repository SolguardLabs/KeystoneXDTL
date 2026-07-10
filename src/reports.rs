use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    AccountId, Amount, Digest, EngineSnapshot, Epoch, Event, KeystoneEngine, KeystoneError,
    KeystoneResult, LoanId, LoanStatus, Shares, VaultId, VaultRole,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultReport {
    pub id: VaultId,
    pub name: String,
    pub role: VaultRole,
    pub cash: Amount,
    pub shares: Shares,
    pub nav: Amount,
    pub gross_assets: Amount,
    pub liabilities: Amount,
    pub outstanding_principal: Amount,
    pub expected_interest: Amount,
    pub realized_interest: Amount,
    pub debt_principal: Amount,
    pub debt_interest_due: Amount,
    pub locked_collateral: Amount,
    pub distributed_interest: Amount,
    pub utilization_bps: crate::Bps,
    pub holder_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoanReport {
    pub id: LoanId,
    pub lender: VaultId,
    pub borrower: VaultId,
    pub status: LoanStatus,
    pub principal: Amount,
    pub remaining_principal: Amount,
    pub scheduled_interest: Amount,
    pub interest_paid: Amount,
    pub collateral_locked: Amount,
    pub collateral_released: Amount,
    pub start_epoch: Epoch,
    pub maturity_epoch: Epoch,
    pub due_epoch: Epoch,
    pub annual_rate_bps: crate::Bps,
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct TotalsReport {
    pub cash: Amount,
    pub locked_collateral: Amount,
    pub outstanding_principal: Amount,
    pub expected_interest: Amount,
    pub realized_interest: Amount,
    pub debt_principal: Amount,
    pub debt_interest_due: Amount,
    pub distributed_interest: Amount,
    pub nav: Amount,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngineReport {
    pub network_id: u32,
    pub epoch: Epoch,
    pub asset: crate::AssetId,
    pub state_digest: Digest,
    pub policy_digest: Digest,
    pub journal_digest: Digest,
    pub vaults: BTreeMap<String, VaultReport>,
    pub loans: BTreeMap<String, LoanReport>,
    pub totals: TotalsReport,
    pub event_count: usize,
    pub conservation_ok: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationReport {
    pub name: String,
    pub tx: Option<crate::TxId>,
    pub loan: Option<LoanId>,
    pub vault: Option<VaultId>,
    pub amount: Option<Amount>,
    pub shares: Option<Shares>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioReport {
    pub scenario: String,
    pub engine: EngineReport,
    pub vault_aliases: BTreeMap<String, VaultId>,
    pub account_aliases: BTreeMap<String, AccountId>,
    pub operations: Vec<OperationReport>,
    pub events: Vec<Event>,
}

impl VaultReport {
    pub fn from_snapshot(snapshot: crate::vault::VaultSnapshot) -> Self {
        Self {
            id: snapshot.id,
            name: snapshot.name,
            role: snapshot.role,
            cash: snapshot.cash,
            shares: snapshot.share_supply,
            nav: snapshot.nav,
            gross_assets: snapshot.gross_assets,
            liabilities: snapshot.liabilities,
            outstanding_principal: snapshot.outstanding_principal,
            expected_interest: snapshot.expected_interest,
            realized_interest: snapshot.realized_interest,
            debt_principal: snapshot.debt_principal,
            debt_interest_due: snapshot.debt_interest_due,
            locked_collateral: snapshot.locked_collateral,
            distributed_interest: snapshot.distributed_interest,
            utilization_bps: snapshot.utilization_bps,
            holder_count: snapshot.holder_count,
        }
    }
}

impl LoanReport {
    pub fn from_snapshot(snapshot: crate::loan::LoanSnapshot) -> Self {
        Self {
            id: snapshot.loan,
            lender: snapshot.lender,
            borrower: snapshot.borrower,
            status: snapshot.status,
            principal: snapshot.principal,
            remaining_principal: snapshot.remaining_principal,
            scheduled_interest: snapshot.scheduled_interest,
            interest_paid: snapshot.interest_paid,
            collateral_locked: snapshot.collateral_locked,
            collateral_released: snapshot.collateral_released,
            start_epoch: snapshot.start_epoch,
            maturity_epoch: snapshot.maturity_epoch,
            due_epoch: snapshot.due_epoch,
            annual_rate_bps: snapshot.annual_rate_bps,
        }
    }
}

impl TotalsReport {
    pub fn add_vault(&mut self, vault: &VaultReport) -> KeystoneResult<()> {
        self.cash = self.cash.checked_add(vault.cash)?;
        self.locked_collateral = self
            .locked_collateral
            .checked_add(vault.locked_collateral)?;
        self.outstanding_principal = self
            .outstanding_principal
            .checked_add(vault.outstanding_principal)?;
        self.expected_interest = self
            .expected_interest
            .checked_add(vault.expected_interest)?;
        self.realized_interest = self
            .realized_interest
            .checked_add(vault.realized_interest)?;
        self.debt_principal = self.debt_principal.checked_add(vault.debt_principal)?;
        self.debt_interest_due = self
            .debt_interest_due
            .checked_add(vault.debt_interest_due)?;
        self.distributed_interest = self
            .distributed_interest
            .checked_add(vault.distributed_interest)?;
        self.nav = self.nav.checked_add(vault.nav)?;
        Ok(())
    }
}

impl EngineReport {
    pub fn from_engine(engine: &KeystoneEngine) -> KeystoneResult<Self> {
        let snapshot = engine.snapshot()?;
        Self::from_snapshot(engine, snapshot)
    }

    pub fn from_snapshot(
        engine: &KeystoneEngine,
        snapshot: EngineSnapshot,
    ) -> KeystoneResult<Self> {
        let mut vaults = BTreeMap::new();
        let mut totals = TotalsReport::default();
        for snapshot in snapshot.vaults {
            let report = VaultReport::from_snapshot(snapshot);
            totals.add_vault(&report)?;
            vaults.insert(report.name.clone(), report);
        }
        let mut loans = BTreeMap::new();
        for snapshot in snapshot.loans {
            let report = LoanReport::from_snapshot(snapshot);
            loans.insert(report.id.to_hex(), report);
        }
        Ok(Self {
            network_id: snapshot.network_id,
            epoch: snapshot.epoch,
            asset: engine.asset(),
            state_digest: engine.state_digest()?,
            policy_digest: snapshot.policy_digest,
            journal_digest: snapshot.journal_digest,
            vaults,
            loans,
            totals,
            event_count: engine.journal().len(),
            conservation_ok: engine.verify_invariants().is_ok(),
        })
    }
}

impl OperationReport {
    pub fn tx(name: impl Into<String>, tx: crate::TxId) -> Self {
        Self {
            name: name.into(),
            tx: Some(tx),
            loan: None,
            vault: None,
            amount: None,
            shares: None,
        }
    }

    pub fn loan(name: impl Into<String>, loan: LoanId, amount: Amount) -> Self {
        Self {
            name: name.into(),
            tx: None,
            loan: Some(loan),
            vault: None,
            amount: Some(amount),
            shares: None,
        }
    }

    pub fn vault_amount(name: impl Into<String>, vault: VaultId, amount: Amount) -> Self {
        Self {
            name: name.into(),
            tx: None,
            loan: None,
            vault: Some(vault),
            amount: Some(amount),
            shares: None,
        }
    }

    pub fn vault_shares(
        name: impl Into<String>,
        vault: VaultId,
        amount: Amount,
        shares: Shares,
    ) -> Self {
        Self {
            name: name.into(),
            tx: None,
            loan: None,
            vault: Some(vault),
            amount: Some(amount),
            shares: Some(shares),
        }
    }
}

impl ScenarioReport {
    pub fn build(
        scenario: impl Into<String>,
        engine: &KeystoneEngine,
        vault_aliases: BTreeMap<String, VaultId>,
        account_aliases: BTreeMap<String, AccountId>,
        operations: Vec<OperationReport>,
    ) -> KeystoneResult<Self> {
        let events = engine.journal().events().to_vec();
        Ok(Self {
            scenario: scenario.into(),
            engine: EngineReport::from_engine(engine)?,
            vault_aliases,
            account_aliases,
            operations,
            events,
        })
    }

    pub fn to_json_pretty(&self) -> KeystoneResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|error| KeystoneError::serialization(error.to_string()))
    }
}
