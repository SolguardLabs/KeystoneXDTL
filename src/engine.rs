use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    AccountId, Amount, DepositReceipt, Digest, Epoch, EpochClock, EventKind, Journal,
    KeystoneError, KeystoneResult, LiquidationPlan, LiquidationResult, LoanId, LoanState,
    LoanStatus, LoanTerms, LoanTermsInput, OracleBook, PaymentQuote, ProtocolPolicy,
    RedemptionQuote, RepaymentMode, Schedule, Shares, TxId, Vault, VaultId, VaultRole,
};

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct EngineConfig {
    pub network_id: u32,
    pub asset: crate::AssetId,
    pub start_epoch: Epoch,
    pub default_policy: ProtocolPolicy,
}

impl EngineConfig {
    pub fn local(asset: crate::AssetId) -> KeystoneResult<Self> {
        Ok(Self {
            network_id: 8_812,
            asset,
            start_epoch: Epoch(1),
            default_policy: ProtocolPolicy::default_prime()?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngineSnapshot {
    pub network_id: u32,
    pub epoch: Epoch,
    pub policy_digest: Digest,
    pub oracle_digest: Digest,
    pub journal_digest: Digest,
    pub vaults: Vec<crate::vault::VaultSnapshot>,
    pub loans: Vec<crate::loan::LoanSnapshot>,
}

#[derive(Clone, Debug)]
pub struct KeystoneEngine {
    config: EngineConfig,
    clock: EpochClock,
    policy: ProtocolPolicy,
    oracle: OracleBook,
    vaults: BTreeMap<VaultId, Vault>,
    loans: BTreeMap<LoanId, LoanState>,
    journal: Journal,
    next_loan_nonce: u64,
}

impl KeystoneEngine {
    pub fn new(config: EngineConfig) -> KeystoneResult<Self> {
        let mut oracle = OracleBook::default();
        oracle.set_unit_price(config.asset, config.start_epoch)?;
        Ok(Self {
            config,
            clock: EpochClock::new(config.start_epoch),
            policy: config.default_policy,
            oracle,
            vaults: BTreeMap::new(),
            loans: BTreeMap::new(),
            journal: Journal::new(config.network_id, config.start_epoch)?,
            next_loan_nonce: 1,
        })
    }

    pub fn network_id(&self) -> u32 {
        self.config.network_id
    }

    pub fn asset(&self) -> crate::AssetId {
        self.config.asset
    }

    pub fn epoch(&self) -> Epoch {
        self.clock.current()
    }

    pub fn policy(&self) -> ProtocolPolicy {
        self.policy
    }

    pub fn oracle(&self) -> &OracleBook {
        &self.oracle
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    pub fn vault_count(&self) -> usize {
        self.vaults.len()
    }

    pub fn loan_count(&self) -> usize {
        self.loans.len()
    }

    pub fn active_loan_count(&self) -> usize {
        self.loans
            .values()
            .filter(|loan| !loan.status().is_terminal())
            .count()
    }

    pub fn vault_ids(&self) -> Vec<VaultId> {
        self.vaults.keys().copied().collect()
    }

    pub fn loan_ids(&self) -> Vec<LoanId> {
        self.loans.keys().copied().collect()
    }

    pub fn vault(&self, id: VaultId) -> KeystoneResult<&Vault> {
        self.vaults.get(&id).ok_or(KeystoneError::VaultNotFound)
    }

    pub fn vault_mut(&mut self, id: VaultId) -> KeystoneResult<&mut Vault> {
        self.vaults.get_mut(&id).ok_or(KeystoneError::VaultNotFound)
    }

    pub fn loan(&self, id: LoanId) -> KeystoneResult<&LoanState> {
        self.loans.get(&id).ok_or(KeystoneError::LoanNotFound)
    }

    pub fn loan_mut(&mut self, id: LoanId) -> KeystoneResult<&mut LoanState> {
        self.loans.get_mut(&id).ok_or(KeystoneError::LoanNotFound)
    }

    pub fn register_vault(
        &mut self,
        name: impl Into<String>,
        role: VaultRole,
    ) -> KeystoneResult<VaultId> {
        if self.vaults.len() >= self.policy.limits.max_vaults {
            return Err(KeystoneError::LimitExceeded);
        }
        let name = name.into();
        let id = VaultId::named(&name, self.config.asset);
        if self.vaults.contains_key(&id) {
            return Err(KeystoneError::VaultAlreadyExists);
        }
        let vault = Vault::new(id, name, self.config.asset, role);
        self.vaults.insert(id, vault);
        self.journal.push(
            self.epoch(),
            EventKind::VaultRegistered {
                vault: id,
                role: role.as_str().to_owned(),
            },
        )?;
        Ok(id)
    }

    pub fn deposit(
        &mut self,
        vault_id: VaultId,
        owner: AccountId,
        amount: Amount,
    ) -> KeystoneResult<DepositReceipt> {
        let receipt = self.vault_mut(vault_id)?.deposit(owner, amount)?;
        self.journal.push(
            self.epoch(),
            EventKind::AccountDeposited {
                vault: vault_id,
                owner,
                amount,
                shares: receipt.shares,
            },
        )?;
        Ok(receipt)
    }

    pub fn redeem(
        &mut self,
        vault_id: VaultId,
        owner: AccountId,
        shares: Shares,
    ) -> KeystoneResult<RedemptionQuote> {
        let quote = self.vault_mut(vault_id)?.redeem(owner, shares)?;
        self.journal.push(
            self.epoch(),
            EventKind::AccountRedeemed {
                vault: vault_id,
                owner,
                amount: quote.amount,
                shares,
            },
        )?;
        Ok(quote)
    }

    pub fn quote_redeem(
        &self,
        vault_id: VaultId,
        owner: AccountId,
        shares: Shares,
    ) -> KeystoneResult<RedemptionQuote> {
        self.vault(vault_id)?.quote_redeem(owner, shares)
    }

    pub fn set_policy(&mut self, policy: ProtocolPolicy) -> KeystoneResult<TxId> {
        self.policy = policy;
        let digest = policy.digest()?;
        self.journal
            .push(self.epoch(), EventKind::PolicyUpdated { digest })
    }

    pub fn advance_epoch(&mut self, target: Epoch) -> KeystoneResult<TxId> {
        let from = self.epoch();
        self.clock.advance_to(target)?;
        self.journal
            .push(self.epoch(), EventKind::EpochAdvanced { from, to: target })
    }

    pub fn advance_by(&mut self, epochs: u64) -> KeystoneResult<TxId> {
        let target = self.epoch().checked_add(epochs)?;
        self.advance_epoch(target)
    }

    pub fn open_loan(
        &mut self,
        lender_id: VaultId,
        borrower_id: VaultId,
        principal: Amount,
        collateral: Amount,
        tenor_epochs: u64,
    ) -> KeystoneResult<LoanId> {
        if self.active_loan_count() >= self.policy.limits.max_active_loans {
            return Err(KeystoneError::LimitExceeded);
        }
        if lender_id == borrower_id {
            return Err(KeystoneError::SelfLoan);
        }
        let (mut lender, mut borrower) = self.take_two_vaults(lender_id, borrower_id)?;
        let result = self.open_loan_inner(
            &mut lender,
            &mut borrower,
            principal,
            collateral,
            tenor_epochs,
        );
        self.put_two_vaults(lender, borrower);
        let loan_id = result?;
        let loan = self.loan(loan_id)?;
        self.journal.push(
            self.epoch(),
            EventKind::LoanOpened {
                loan: loan_id,
                lender: lender_id,
                borrower: borrower_id,
                principal,
                projected_interest: loan.scheduled_interest(),
            },
        )?;
        Ok(loan_id)
    }

    fn open_loan_inner(
        &mut self,
        lender: &mut Vault,
        borrower: &mut Vault,
        principal: Amount,
        collateral: Amount,
        tenor_epochs: u64,
    ) -> KeystoneResult<LoanId> {
        if lender.asset() != borrower.asset() || lender.asset() != self.config.asset {
            return Err(KeystoneError::AssetMismatch);
        }
        if !lender.role().can_lend() || !borrower.role().can_borrow() {
            return Err(KeystoneError::VaultRoleMismatch);
        }
        let utilization = lender.utilization_bps()?;
        let annual_rate = self
            .policy
            .interest
            .quoted_rate(utilization, self.policy.collateral.tier)?;
        let schedule = Schedule::new(
            self.epoch(),
            tenor_epochs,
            3,
            self.policy.interest.epochs_per_year,
        )?;
        let scheduled_interest =
            self.policy
                .interest
                .projected_interest(principal, annual_rate, tenor_epochs)?;
        let borrower_debt_after = borrower
            .debt_principal()
            .checked_add(principal)?
            .checked_add(borrower.debt_interest_due())?
            .checked_add(scheduled_interest)?;
        self.policy.check_open_loan(
            principal,
            collateral,
            lender.net_asset_value()?,
            borrower_debt_after,
            borrower.net_asset_value()?.max(Amount::ONE),
        )?;
        let available = lender.available_to_lend(self.policy)?;
        if principal > available {
            return Err(KeystoneError::InsufficientLiquidity);
        }
        borrower.lock_collateral(collateral)?;
        lender.fund_loan(principal, scheduled_interest)?;
        borrower.receive_loan_draw(principal, scheduled_interest)?;
        let nonce = self.next_loan_nonce;
        self.next_loan_nonce = self
            .next_loan_nonce
            .checked_add(1)
            .ok_or(KeystoneError::AmountOverflow)?;
        let terms = LoanTerms::new(LoanTermsInput {
            network_id: self.config.network_id,
            lender: lender.id(),
            borrower: borrower.id(),
            principal,
            collateral,
            annual_rate_bps: annual_rate,
            schedule,
            policy_digest: self.policy.digest()?,
            nonce,
        })?;
        let loan = LoanState::activate(terms, scheduled_interest)?;
        let loan_id = loan.id();
        if self.loans.insert(loan_id, loan).is_some() {
            return Err(KeystoneError::LoanAlreadyExists);
        }
        Ok(loan_id)
    }

    pub fn quote_repayment(
        &self,
        loan_id: LoanId,
        mode: RepaymentMode,
        principal: Option<Amount>,
    ) -> KeystoneResult<PaymentQuote> {
        let loan = self.loan(loan_id)?;
        match mode {
            RepaymentMode::Scheduled => loan.quote_scheduled_repayment(),
            RepaymentMode::Early => {
                loan.quote_early_repayment(self.epoch(), self.policy.interest.epochs_per_year)
            }
            RepaymentMode::Partial => loan.quote_partial_repayment(
                self.epoch(),
                principal.unwrap_or(loan.remaining_principal()),
                self.policy.interest.epochs_per_year,
                self.policy.collateral,
            ),
            RepaymentMode::Recovery => loan.quote_scheduled_repayment(),
        }
    }

    pub fn repay_loan(
        &mut self,
        loan_id: LoanId,
        mode: RepaymentMode,
        principal: Option<Amount>,
    ) -> KeystoneResult<PaymentQuote> {
        let quote = self.quote_repayment(loan_id, mode, principal)?;
        if quote.is_zero() {
            return Err(KeystoneError::ZeroAmount);
        }
        let loan_view = self.loan(loan_id)?.clone();
        let lender_id = loan_view.lender();
        let borrower_id = loan_view.borrower();
        let (mut lender, mut borrower) = self.take_two_vaults(lender_id, borrower_id)?;
        let mut loan = self
            .loans
            .remove(&loan_id)
            .ok_or(KeystoneError::LoanNotFound)?;
        let result = self.repay_loan_inner(&mut lender, &mut borrower, &mut loan, quote);
        self.loans.insert(loan_id, loan);
        self.put_two_vaults(lender, borrower);
        let applied = result?;
        self.journal.push(
            self.epoch(),
            EventKind::LoanRepaid {
                loan: loan_id,
                payer: borrower_id,
                principal: applied.principal,
                interest: applied.interest,
                mode: applied.mode.as_str().to_owned(),
            },
        )?;
        if !applied.collateral_release.is_zero() {
            self.journal.push(
                self.epoch(),
                EventKind::CollateralReleased {
                    loan: loan_id,
                    borrower: borrower_id,
                    amount: applied.collateral_release,
                },
            )?;
        }
        Ok(applied)
    }

    fn repay_loan_inner(
        &self,
        lender: &mut Vault,
        borrower: &mut Vault,
        loan: &mut LoanState,
        quote: PaymentQuote,
    ) -> KeystoneResult<PaymentQuote> {
        let total = quote.total()?;
        if total > borrower.cash() {
            return Err(KeystoneError::InsufficientCash);
        }
        borrower.pay_repayment_cash(
            quote.principal,
            quote.interest,
            quote.borrower_interest_reduction,
        )?;
        lender.receive_repayment_cash(quote.principal, quote.interest)?;
        if !quote.lender_projection_reduction.is_zero() {
            lender.reduce_expected_interest(quote.lender_projection_reduction)?;
        }
        let released = borrower.release_collateral(quote.collateral_release)?;
        let applied = PaymentQuote {
            collateral_release: released,
            ..quote
        };
        loan.apply_repayment(applied)?;
        Ok(applied)
    }

    pub fn mark_default(&mut self, loan_id: LoanId) -> KeystoneResult<u64> {
        let now = self.epoch();
        let overdue = self.loan_mut(loan_id)?.mark_defaulted(now)?;
        self.journal.push(
            now,
            EventKind::LoanDefaulted {
                loan: loan_id,
                overdue_epochs: overdue,
            },
        )?;
        Ok(overdue)
    }

    pub fn liquidation_plan(&self, loan_id: LoanId) -> KeystoneResult<LiquidationPlan> {
        LiquidationPlan::build(self.loan(loan_id)?, self.policy)
    }

    pub fn liquidate(&mut self, loan_id: LoanId) -> KeystoneResult<LiquidationResult> {
        {
            let loan = self.loan_mut(loan_id)?;
            if loan.status() == LoanStatus::Defaulted {
                loan.begin_liquidation()?;
            }
        }
        let plan = self.liquidation_plan(loan_id)?;
        let lender_id = plan.lender;
        let borrower_id = plan.borrower;
        let (mut lender, mut borrower) = self.take_two_vaults(lender_id, borrower_id)?;
        let mut loan = self
            .loans
            .remove(&loan_id)
            .ok_or(KeystoneError::LoanNotFound)?;
        let result = self.liquidate_inner(&mut lender, &mut borrower, &mut loan, plan);
        self.loans.insert(loan_id, loan);
        self.put_two_vaults(lender, borrower);
        let result = result?;
        self.journal.push(
            self.epoch(),
            EventKind::LoanLiquidated {
                loan: loan_id,
                seized: result.seized,
                shortfall: result.shortfall,
            },
        )?;
        Ok(result)
    }

    fn liquidate_inner(
        &self,
        lender: &mut Vault,
        borrower: &mut Vault,
        loan: &mut LoanState,
        plan: LiquidationPlan,
    ) -> KeystoneResult<LiquidationResult> {
        let seized = borrower.seize_collateral(plan.collateral_to_seize)?;
        let principal_covered = plan.principal_covered()?;
        let interest_covered = plan.interest_covered();
        let reserve_recovery = plan.reserve_recovery()?;
        lender.receive_liquidation_proceeds(
            principal_covered,
            interest_covered,
            reserve_recovery,
        )?;
        borrower.extinguish_borrower_debt(principal_covered, interest_covered)?;
        loan.complete_liquidation(principal_covered, interest_covered, seized)?;
        plan.result(loan.remaining_principal())
    }

    pub fn distribute_interest(
        &mut self,
        vault_id: VaultId,
    ) -> KeystoneResult<Vec<crate::vault::InterestClaim>> {
        let policy = self.policy;
        let claims = self.vault_mut(vault_id)?.distribute_interest(policy)?;
        let amount = Amount::checked_sum(claims.iter().map(|claim| claim.amount))?;
        self.journal.push(
            self.epoch(),
            EventKind::InterestDistributed {
                vault: vault_id,
                amount,
                recipients: claims.len(),
            },
        )?;
        Ok(claims)
    }

    pub fn snapshot(&self) -> KeystoneResult<EngineSnapshot> {
        let mut vaults = Vec::with_capacity(self.vaults.len());
        for vault in self.vaults.values() {
            vaults.push(vault.snapshot()?);
        }
        let mut loans = Vec::with_capacity(self.loans.len());
        for loan in self.loans.values() {
            loans.push(loan.snapshot()?);
        }
        Ok(EngineSnapshot {
            network_id: self.config.network_id,
            epoch: self.epoch(),
            policy_digest: self.policy.digest()?,
            oracle_digest: self.oracle.digest()?,
            journal_digest: self.journal.digest(),
            vaults,
            loans,
        })
    }

    pub fn state_digest(&self) -> KeystoneResult<Digest> {
        let snapshot = self.snapshot()?;
        let bytes = serde_json::to_vec(&snapshot)
            .map_err(|error| KeystoneError::serialization(error.to_string()))?;
        Ok(Digest::from_parts(
            "keystonexdtl-engine-state-v1",
            &[&bytes],
        ))
    }

    pub fn verify_invariants(&self) -> KeystoneResult<()> {
        for vault in self.vaults.values() {
            vault.assert_sane()?;
        }
        let mut loan_principal = Amount::ZERO;
        let mut borrower_debt = Amount::ZERO;
        let mut borrower_interest = Amount::ZERO;
        for loan in self.loans.values() {
            if loan.status().is_terminal() {
                continue;
            }
            loan_principal = loan_principal.checked_add(loan.remaining_principal())?;
            borrower_debt = borrower_debt.checked_add(loan.remaining_principal())?;
            borrower_interest =
                borrower_interest.checked_add(loan.remaining_scheduled_interest()?)?;
        }
        let mut vault_outstanding = Amount::ZERO;
        let mut vault_borrower_debt = Amount::ZERO;
        let mut vault_borrower_interest = Amount::ZERO;
        for vault in self.vaults.values() {
            vault_outstanding = vault_outstanding.checked_add(vault.outstanding_principal())?;
            vault_borrower_debt = vault_borrower_debt.checked_add(vault.debt_principal())?;
            vault_borrower_interest =
                vault_borrower_interest.checked_add(vault.debt_interest_due())?;
        }
        if loan_principal != vault_outstanding {
            return Err(KeystoneError::invariant("principal ledger mismatch"));
        }
        if borrower_debt != vault_borrower_debt {
            return Err(KeystoneError::invariant("borrower principal mismatch"));
        }
        if borrower_interest != vault_borrower_interest {
            return Err(KeystoneError::invariant("borrower interest mismatch"));
        }
        Ok(())
    }

    pub fn push_invariant_event(&mut self) -> KeystoneResult<TxId> {
        self.verify_invariants()?;
        let digest = self.state_digest()?;
        self.journal
            .push(self.epoch(), EventKind::InvariantChecked { digest })
    }

    fn take_two_vaults(
        &mut self,
        first: VaultId,
        second: VaultId,
    ) -> KeystoneResult<(Vault, Vault)> {
        if first == second {
            return Err(KeystoneError::SelfLoan);
        }
        let first_vault = self
            .vaults
            .remove(&first)
            .ok_or(KeystoneError::VaultNotFound)?;
        let second_vault = match self.vaults.remove(&second) {
            Some(vault) => vault,
            None => {
                self.vaults.insert(first, first_vault);
                return Err(KeystoneError::VaultNotFound);
            }
        };
        Ok((first_vault, second_vault))
    }

    fn put_two_vaults(&mut self, first: Vault, second: Vault) {
        self.vaults.insert(first.id(), first);
        self.vaults.insert(second.id(), second);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetId, RiskTier};

    fn fixture() -> KeystoneEngine {
        let asset = AssetId::native();
        let mut engine = KeystoneEngine::new(EngineConfig::local(asset).unwrap()).unwrap();
        engine
            .set_policy(ProtocolPolicy::for_tier(RiskTier::Prime).unwrap())
            .unwrap();
        let lender = engine
            .register_vault("senior-liquidity", VaultRole::Liquidity)
            .unwrap();
        let borrower = engine
            .register_vault("market-maker", VaultRole::Borrower)
            .unwrap();
        engine
            .deposit(lender, AccountId::named("lp"), Amount(100_000))
            .unwrap();
        engine
            .deposit(borrower, AccountId::named("mm"), Amount(40_000))
            .unwrap();
        engine
    }

    #[test]
    fn open_and_repay_scheduled_loan() {
        let mut engine = fixture();
        let lender = VaultId::named("senior-liquidity", engine.asset());
        let borrower = VaultId::named("market-maker", engine.asset());
        let loan = engine
            .open_loan(lender, borrower, Amount(20_000), Amount(24_000), 90)
            .unwrap();
        engine.advance_by(90).unwrap();
        let quote = engine
            .repay_loan(loan, RepaymentMode::Scheduled, None)
            .unwrap();
        assert_eq!(quote.principal, Amount(20_000));
        assert_eq!(engine.loan(loan).unwrap().status(), LoanStatus::Paid);
        engine.verify_invariants().unwrap();
    }

    #[test]
    fn default_and_liquidation_close_part_of_debt() {
        let mut engine = fixture();
        let lender = VaultId::named("senior-liquidity", engine.asset());
        let borrower = VaultId::named("market-maker", engine.asset());
        let loan = engine
            .open_loan(lender, borrower, Amount(20_000), Amount(24_000), 30)
            .unwrap();
        engine.advance_by(35).unwrap();
        engine.mark_default(loan).unwrap();
        let result = engine.liquidate(loan).unwrap();
        assert!(!result.seized.is_zero());
        engine.verify_invariants().unwrap();
    }
}
