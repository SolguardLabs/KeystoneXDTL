use serde::{Deserialize, Serialize};

use crate::{Amount, EngineSnapshot, KeystoneError, KeystoneResult, LoanStatus, VaultId};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountingSide {
    Asset,
    Liability,
    Equity,
    Memo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingLine {
    pub side: AccountingSide,
    pub code: String,
    pub amount: Amount,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultAccounting {
    pub vault: VaultId,
    pub name: String,
    pub lines: Vec<AccountingLine>,
    pub assets: Amount,
    pub liabilities: Amount,
    pub equity: Amount,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemAccounting {
    pub cash: Amount,
    pub locked_collateral: Amount,
    pub receivable_principal: Amount,
    pub receivable_interest: Amount,
    pub realized_interest: Amount,
    pub borrower_principal: Amount,
    pub borrower_interest: Amount,
    pub protocol_fees: Amount,
    pub equity: Amount,
    pub active_principal: Amount,
    pub terminal_principal: Amount,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingBook {
    pub vaults: Vec<VaultAccounting>,
    pub system: SystemAccounting,
    pub balanced: bool,
}

impl AccountingLine {
    pub fn asset(code: impl Into<String>, amount: Amount) -> Self {
        Self {
            side: AccountingSide::Asset,
            code: code.into(),
            amount,
        }
    }

    pub fn liability(code: impl Into<String>, amount: Amount) -> Self {
        Self {
            side: AccountingSide::Liability,
            code: code.into(),
            amount,
        }
    }

    pub fn equity(code: impl Into<String>, amount: Amount) -> Self {
        Self {
            side: AccountingSide::Equity,
            code: code.into(),
            amount,
        }
    }

    pub fn memo(code: impl Into<String>, amount: Amount) -> Self {
        Self {
            side: AccountingSide::Memo,
            code: code.into(),
            amount,
        }
    }
}

impl VaultAccounting {
    pub fn from_snapshot(snapshot: &crate::vault::VaultSnapshot) -> KeystoneResult<Self> {
        let lines = vec![
            AccountingLine::asset("cash", snapshot.cash),
            AccountingLine::asset("locked_collateral", snapshot.locked_collateral),
            AccountingLine::asset("receivable_principal", snapshot.outstanding_principal),
            AccountingLine::asset("receivable_interest", snapshot.expected_interest),
            AccountingLine::asset("loss_reserve", snapshot.loss_reserve),
            AccountingLine::liability("borrowed_principal", snapshot.debt_principal),
            AccountingLine::liability("borrowed_interest", snapshot.debt_interest_due),
            AccountingLine::liability("protocol_fees", snapshot.protocol_fees),
            AccountingLine::equity("net_asset_value", snapshot.nav),
            AccountingLine::memo("realized_interest", snapshot.realized_interest),
            AccountingLine::memo("distributed_interest", snapshot.distributed_interest),
        ];
        let assets = sum_side(&lines, AccountingSide::Asset)?;
        let liabilities = sum_side(&lines, AccountingSide::Liability)?;
        let equity = sum_side(&lines, AccountingSide::Equity)?;
        Ok(Self {
            vault: snapshot.id,
            name: snapshot.name.clone(),
            lines,
            assets,
            liabilities,
            equity,
        })
    }

    pub fn computed_equity(&self) -> KeystoneResult<Amount> {
        self.assets.checked_sub(self.liabilities)
    }

    pub fn is_balanced(&self) -> KeystoneResult<bool> {
        Ok(self.computed_equity()? == self.equity)
    }

    pub fn line(&self, code: &str) -> Option<Amount> {
        self.lines
            .iter()
            .find(|line| line.code == code)
            .map(|line| line.amount)
    }

    pub fn non_zero_lines(&self) -> impl Iterator<Item = &AccountingLine> {
        self.lines.iter().filter(|line| !line.amount.is_zero())
    }
}

impl SystemAccounting {
    pub fn add_vault(&mut self, snapshot: &crate::vault::VaultSnapshot) -> KeystoneResult<()> {
        self.cash = self.cash.checked_add(snapshot.cash)?;
        self.locked_collateral = self
            .locked_collateral
            .checked_add(snapshot.locked_collateral)?;
        self.receivable_principal = self
            .receivable_principal
            .checked_add(snapshot.outstanding_principal)?;
        self.receivable_interest = self
            .receivable_interest
            .checked_add(snapshot.expected_interest)?;
        self.realized_interest = self
            .realized_interest
            .checked_add(snapshot.realized_interest)?;
        self.borrower_principal = self
            .borrower_principal
            .checked_add(snapshot.debt_principal)?;
        self.borrower_interest = self
            .borrower_interest
            .checked_add(snapshot.debt_interest_due)?;
        self.protocol_fees = self.protocol_fees.checked_add(snapshot.protocol_fees)?;
        self.equity = self.equity.checked_add(snapshot.nav)?;
        Ok(())
    }

    pub fn add_loan(&mut self, snapshot: &crate::loan::LoanSnapshot) -> KeystoneResult<()> {
        if snapshot.status.is_terminal() {
            self.terminal_principal = self
                .terminal_principal
                .checked_add(snapshot.remaining_principal)?;
        } else {
            self.active_principal = self
                .active_principal
                .checked_add(snapshot.remaining_principal)?;
        }
        Ok(())
    }

    pub fn net_cash_assets(&self) -> KeystoneResult<Amount> {
        self.cash.checked_add(self.locked_collateral)
    }

    pub fn receivables(&self) -> KeystoneResult<Amount> {
        self.receivable_principal
            .checked_add(self.receivable_interest)
    }

    pub fn borrower_debts(&self) -> KeystoneResult<Amount> {
        self.borrower_principal.checked_add(self.borrower_interest)
    }

    pub fn accounting_spread(&self) -> KeystoneResult<i128> {
        let receivables = self.receivables()?.raw() as i128;
        let debts = self.borrower_debts()?.raw() as i128;
        Ok(receivables - debts)
    }

    pub fn principal_matched(&self) -> bool {
        self.receivable_principal == self.borrower_principal
    }

    pub fn active_principal_matched(&self) -> bool {
        self.receivable_principal == self.active_principal
    }
}

impl AccountingBook {
    pub fn from_snapshot(snapshot: &EngineSnapshot) -> KeystoneResult<Self> {
        let mut vaults = Vec::with_capacity(snapshot.vaults.len());
        let mut system = SystemAccounting::default();
        let mut balanced = true;
        for vault in &snapshot.vaults {
            system.add_vault(vault)?;
            let accounting = VaultAccounting::from_snapshot(vault)?;
            balanced &= accounting.is_balanced()?;
            vaults.push(accounting);
        }
        for loan in &snapshot.loans {
            system.add_loan(loan)?;
        }
        Ok(Self {
            vaults,
            system,
            balanced,
        })
    }

    pub fn from_engine(engine: &crate::KeystoneEngine) -> KeystoneResult<Self> {
        let snapshot = engine.snapshot()?;
        Self::from_snapshot(&snapshot)
    }

    pub fn vault(&self, id: VaultId) -> Option<&VaultAccounting> {
        self.vaults.iter().find(|vault| vault.vault == id)
    }

    pub fn assert_balanced(&self) -> KeystoneResult<()> {
        if !self.balanced {
            return Err(KeystoneError::invariant("accounting book is not balanced"));
        }
        for vault in &self.vaults {
            if !vault.is_balanced()? {
                return Err(KeystoneError::invariant(format!(
                    "vault accounting is not balanced: {}",
                    vault.name
                )));
            }
        }
        Ok(())
    }

    pub fn active_loans_match_vaults(&self) -> bool {
        self.system.active_principal_matched()
    }

    pub fn total_equity(&self) -> Amount {
        self.system.equity
    }
}

fn sum_side(lines: &[AccountingLine], side: AccountingSide) -> KeystoneResult<Amount> {
    Amount::checked_sum(
        lines
            .iter()
            .filter(|line| line.side == side)
            .map(|line| line.amount),
    )
}

pub fn loan_status_weight(status: LoanStatus) -> u8 {
    match status {
        LoanStatus::Proposed => 0,
        LoanStatus::Active => 1,
        LoanStatus::Paid => 2,
        LoanStatus::Defaulted => 3,
        LoanStatus::Liquidating => 4,
        LoanStatus::Liquidated => 5,
        LoanStatus::WrittenOff => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccountId, AssetId, EngineConfig, KeystoneEngine, VaultRole};

    #[test]
    fn accounting_book_balances_seeded_engine() {
        let asset = AssetId::native();
        let mut engine = KeystoneEngine::new(EngineConfig::local(asset).unwrap()).unwrap();
        let vault = engine.register_vault("cash", VaultRole::Hybrid).unwrap();
        engine
            .deposit(vault, AccountId::named("owner"), Amount(10_000))
            .unwrap();
        let book = AccountingBook::from_engine(&engine).unwrap();
        book.assert_balanced().unwrap();
        assert_eq!(book.total_equity(), Amount(10_000));
    }
}
