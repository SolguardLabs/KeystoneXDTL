use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::amount::{amount_for_shares, checked_ratio_bps, shares_for_deposit, split_by_shares};
use crate::{
    AccountId, Amount, Bps, Digest, KeystoneError, KeystoneResult, PositionId, ProtocolPolicy,
    Shares, VaultId,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultRole {
    Liquidity,
    Borrower,
    Hybrid,
    Treasury,
}

impl VaultRole {
    pub fn can_lend(self) -> bool {
        matches!(self, VaultRole::Liquidity | VaultRole::Hybrid)
    }

    pub fn can_borrow(self) -> bool {
        matches!(self, VaultRole::Borrower | VaultRole::Hybrid)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            VaultRole::Liquidity => "liquidity",
            VaultRole::Borrower => "borrower",
            VaultRole::Hybrid => "hybrid",
            VaultRole::Treasury => "treasury",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepositReceipt {
    pub vault: VaultId,
    pub owner: AccountId,
    pub position: PositionId,
    pub amount: Amount,
    pub shares: Shares,
    pub nav_before: Amount,
    pub nav_after: Amount,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedemptionQuote {
    pub vault: VaultId,
    pub owner: AccountId,
    pub shares: Shares,
    pub amount: Amount,
    pub nav: Amount,
    pub cash: Amount,
    pub price_e12: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterestClaim {
    pub owner: AccountId,
    pub amount: Amount,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultSnapshot {
    pub id: VaultId,
    pub name: String,
    pub role: VaultRole,
    pub cash: Amount,
    pub share_supply: Shares,
    pub gross_assets: Amount,
    pub liabilities: Amount,
    pub nav: Amount,
    pub outstanding_principal: Amount,
    pub expected_interest: Amount,
    pub realized_interest: Amount,
    pub debt_principal: Amount,
    pub debt_interest_due: Amount,
    pub locked_collateral: Amount,
    pub loss_reserve: Amount,
    pub protocol_fees: Amount,
    pub distributed_interest: Amount,
    pub utilization_bps: Bps,
    pub holder_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vault {
    id: VaultId,
    name: String,
    asset: crate::AssetId,
    role: VaultRole,
    paused: bool,
    frozen: bool,
    cash: Amount,
    share_supply: Shares,
    accounts: BTreeMap<AccountId, Shares>,
    locked_collateral: Amount,
    outstanding_principal: Amount,
    expected_interest: Amount,
    realized_interest: Amount,
    debt_principal: Amount,
    debt_interest_due: Amount,
    loss_reserve: Amount,
    protocol_fees: Amount,
    distributed_interest: Amount,
    pending_redemptions: Amount,
}

impl Vault {
    pub fn new(
        id: VaultId,
        name: impl Into<String>,
        asset: crate::AssetId,
        role: VaultRole,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            asset,
            role,
            paused: false,
            frozen: false,
            cash: Amount::ZERO,
            share_supply: Shares::ZERO,
            accounts: BTreeMap::new(),
            locked_collateral: Amount::ZERO,
            outstanding_principal: Amount::ZERO,
            expected_interest: Amount::ZERO,
            realized_interest: Amount::ZERO,
            debt_principal: Amount::ZERO,
            debt_interest_due: Amount::ZERO,
            loss_reserve: Amount::ZERO,
            protocol_fees: Amount::ZERO,
            distributed_interest: Amount::ZERO,
            pending_redemptions: Amount::ZERO,
        }
    }

    pub fn id(&self) -> VaultId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn asset(&self) -> crate::AssetId {
        self.asset
    }

    pub fn role(&self) -> VaultRole {
        self.role
    }

    pub fn cash(&self) -> Amount {
        self.cash
    }

    pub fn share_supply(&self) -> Shares {
        self.share_supply
    }

    pub fn outstanding_principal(&self) -> Amount {
        self.outstanding_principal
    }

    pub fn expected_interest(&self) -> Amount {
        self.expected_interest
    }

    pub fn realized_interest(&self) -> Amount {
        self.realized_interest
    }

    pub fn debt_principal(&self) -> Amount {
        self.debt_principal
    }

    pub fn debt_interest_due(&self) -> Amount {
        self.debt_interest_due
    }

    pub fn locked_collateral(&self) -> Amount {
        self.locked_collateral
    }

    pub fn protocol_fees(&self) -> Amount {
        self.protocol_fees
    }

    pub fn distributed_interest(&self) -> Amount {
        self.distributed_interest
    }

    pub fn holder_count(&self) -> usize {
        self.accounts.len()
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn unpause(&mut self) {
        self.paused = false;
    }

    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn unfreeze(&mut self) {
        self.frozen = false;
    }

    pub fn gross_assets(&self) -> KeystoneResult<Amount> {
        Amount::checked_sum([
            self.cash,
            self.locked_collateral,
            self.outstanding_principal,
            self.expected_interest,
            self.loss_reserve,
        ])
    }

    pub fn liabilities(&self) -> KeystoneResult<Amount> {
        Amount::checked_sum([
            self.debt_principal,
            self.debt_interest_due,
            self.protocol_fees,
            self.pending_redemptions,
        ])
    }

    pub fn net_asset_value(&self) -> KeystoneResult<Amount> {
        self.gross_assets()?.checked_sub(self.liabilities()?)
    }

    pub fn liquid_assets(&self) -> Amount {
        self.cash
    }

    pub fn price_e12(&self) -> KeystoneResult<u128> {
        if self.share_supply.is_zero() {
            return Ok(crate::amount::DECIMAL_SCALE);
        }
        Ok(
            self.net_asset_value()?.as_u128() * crate::amount::DECIMAL_SCALE
                / self.share_supply.as_u128(),
        )
    }

    pub fn utilization_bps(&self) -> KeystoneResult<Bps> {
        let nav = self.net_asset_value()?;
        if nav.is_zero() {
            return Bps::strict(0);
        }
        checked_ratio_bps(self.outstanding_principal, nav)
    }

    pub fn available_to_lend(&self, policy: ProtocolPolicy) -> KeystoneResult<Amount> {
        if !self.role.can_lend() {
            return Err(KeystoneError::VaultRoleMismatch);
        }
        let nav = self.net_asset_value()?;
        let max_utilized = policy.limits.max_utilization_bps.apply_floor(nav)?;
        let remaining = max_utilized.saturating_sub(self.outstanding_principal);
        Ok(self.cash.min(remaining))
    }

    pub fn available_cash_after_floor(&self, policy: ProtocolPolicy) -> KeystoneResult<Amount> {
        let nav = self.net_asset_value()?;
        let floor = policy.limits.cash_floor(nav)?;
        Ok(self.cash.saturating_sub(floor))
    }

    pub fn shares_of(&self, owner: AccountId) -> Shares {
        self.accounts.get(&owner).copied().unwrap_or(Shares::ZERO)
    }

    pub fn position_id(&self, owner: AccountId) -> PositionId {
        PositionId::derive(self.id, owner)
    }

    pub fn deposit(&mut self, owner: AccountId, amount: Amount) -> KeystoneResult<DepositReceipt> {
        if self.paused {
            return Err(KeystoneError::VaultPaused);
        }
        if self.frozen {
            return Err(KeystoneError::VaultFrozen);
        }
        if amount.is_zero() {
            return Err(KeystoneError::ZeroAmount);
        }
        let nav_before = self.net_asset_value()?;
        let shares = shares_for_deposit(amount, nav_before, self.share_supply)?;
        if shares.is_zero() {
            return Err(KeystoneError::ZeroShares);
        }
        self.cash = self.cash.checked_add(amount)?;
        self.share_supply = self.share_supply.checked_add(shares)?;
        let current = self.accounts.entry(owner).or_insert(Shares::ZERO);
        *current = current.checked_add(shares)?;
        let nav_after = self.net_asset_value()?;
        Ok(DepositReceipt {
            vault: self.id,
            owner,
            position: self.position_id(owner),
            amount,
            shares,
            nav_before,
            nav_after,
        })
    }

    pub fn quote_redeem(
        &self,
        owner: AccountId,
        shares: Shares,
    ) -> KeystoneResult<RedemptionQuote> {
        if shares.is_zero() {
            return Err(KeystoneError::ZeroShares);
        }
        if self.shares_of(owner) < shares {
            return Err(KeystoneError::InsufficientShares);
        }
        let nav = self.net_asset_value()?;
        let amount = amount_for_shares(shares, nav, self.share_supply)?;
        Ok(RedemptionQuote {
            vault: self.id,
            owner,
            shares,
            amount,
            nav,
            cash: self.cash,
            price_e12: self.price_e12()?,
        })
    }

    pub fn redeem(&mut self, owner: AccountId, shares: Shares) -> KeystoneResult<RedemptionQuote> {
        if self.paused {
            return Err(KeystoneError::VaultPaused);
        }
        if self.frozen {
            return Err(KeystoneError::VaultFrozen);
        }
        let quote = self.quote_redeem(owner, shares)?;
        if quote.amount > self.cash {
            return Err(KeystoneError::RedemptionTooLarge);
        }
        self.cash = self.cash.checked_sub(quote.amount)?;
        self.share_supply = self.share_supply.checked_sub(shares)?;
        let current = self
            .accounts
            .get_mut(&owner)
            .ok_or(KeystoneError::AccountNotFound)?;
        *current = current.checked_sub(shares)?;
        if current.is_zero() {
            self.accounts.remove(&owner);
        }
        Ok(quote)
    }

    pub fn force_cash_credit(&mut self, amount: Amount) -> KeystoneResult<()> {
        self.cash = self.cash.checked_add(amount)?;
        Ok(())
    }

    pub fn force_cash_debit(&mut self, amount: Amount) -> KeystoneResult<()> {
        if amount > self.cash {
            return Err(KeystoneError::InsufficientCash);
        }
        self.cash = self.cash.checked_sub(amount)?;
        Ok(())
    }

    pub fn lock_collateral(&mut self, amount: Amount) -> KeystoneResult<()> {
        if !self.role.can_borrow() {
            return Err(KeystoneError::VaultRoleMismatch);
        }
        if amount > self.cash {
            return Err(KeystoneError::InsufficientCollateral);
        }
        self.cash = self.cash.checked_sub(amount)?;
        self.locked_collateral = self.locked_collateral.checked_add(amount)?;
        Ok(())
    }

    pub fn release_collateral(&mut self, amount: Amount) -> KeystoneResult<Amount> {
        let released = amount.min(self.locked_collateral);
        self.locked_collateral = self.locked_collateral.checked_sub(released)?;
        self.cash = self.cash.checked_add(released)?;
        Ok(released)
    }

    pub fn seize_collateral(&mut self, amount: Amount) -> KeystoneResult<Amount> {
        let seized = amount.min(self.locked_collateral);
        self.locked_collateral = self.locked_collateral.checked_sub(seized)?;
        Ok(seized)
    }

    pub fn fund_loan(
        &mut self,
        principal: Amount,
        projected_interest: Amount,
    ) -> KeystoneResult<()> {
        if !self.role.can_lend() {
            return Err(KeystoneError::VaultRoleMismatch);
        }
        if principal > self.cash {
            return Err(KeystoneError::InsufficientLiquidity);
        }
        self.cash = self.cash.checked_sub(principal)?;
        self.outstanding_principal = self.outstanding_principal.checked_add(principal)?;
        self.expected_interest = self.expected_interest.checked_add(projected_interest)?;
        Ok(())
    }

    pub fn receive_loan_draw(
        &mut self,
        principal: Amount,
        projected_interest: Amount,
    ) -> KeystoneResult<()> {
        if !self.role.can_borrow() {
            return Err(KeystoneError::VaultRoleMismatch);
        }
        self.cash = self.cash.checked_add(principal)?;
        self.debt_principal = self.debt_principal.checked_add(principal)?;
        self.debt_interest_due = self.debt_interest_due.checked_add(projected_interest)?;
        Ok(())
    }

    pub fn receive_repayment_cash(
        &mut self,
        principal: Amount,
        interest: Amount,
    ) -> KeystoneResult<()> {
        let total = principal.checked_add(interest)?;
        self.cash = self.cash.checked_add(total)?;
        self.outstanding_principal = self.outstanding_principal.checked_sub(principal)?;
        self.realized_interest = self.realized_interest.checked_add(interest)?;
        Ok(())
    }

    pub fn pay_repayment_cash(
        &mut self,
        principal: Amount,
        interest: Amount,
        debt_interest_reduction: Amount,
    ) -> KeystoneResult<()> {
        let total = principal.checked_add(interest)?;
        if total > self.cash {
            return Err(KeystoneError::InsufficientCash);
        }
        self.cash = self.cash.checked_sub(total)?;
        self.debt_principal = self.debt_principal.checked_sub(principal)?;
        self.debt_interest_due = self
            .debt_interest_due
            .checked_sub(debt_interest_reduction)?;
        Ok(())
    }

    pub fn reduce_expected_interest(&mut self, amount: Amount) -> KeystoneResult<()> {
        self.expected_interest = self.expected_interest.checked_sub(amount)?;
        Ok(())
    }

    pub fn reduce_debt_interest(&mut self, amount: Amount) -> KeystoneResult<()> {
        self.debt_interest_due = self.debt_interest_due.checked_sub(amount)?;
        Ok(())
    }

    pub fn receive_liquidation_proceeds(
        &mut self,
        principal_covered: Amount,
        interest_covered: Amount,
        reserve_recovery: Amount,
    ) -> KeystoneResult<()> {
        let total = Amount::checked_sum([principal_covered, interest_covered, reserve_recovery])?;
        self.cash = self.cash.checked_add(total)?;
        self.outstanding_principal = self.outstanding_principal.checked_sub(principal_covered)?;
        self.expected_interest = self.expected_interest.checked_sub(interest_covered)?;
        self.loss_reserve = self.loss_reserve.checked_add(reserve_recovery)?;
        Ok(())
    }

    pub fn absorb_shortfall(
        &mut self,
        principal_loss: Amount,
        interest_loss: Amount,
    ) -> KeystoneResult<()> {
        self.outstanding_principal = self.outstanding_principal.checked_sub(principal_loss)?;
        self.expected_interest = self.expected_interest.checked_sub(interest_loss)?;
        let total_loss = principal_loss.checked_add(interest_loss)?;
        self.loss_reserve = self.loss_reserve.saturating_sub(total_loss);
        Ok(())
    }

    pub fn extinguish_borrower_debt(
        &mut self,
        principal: Amount,
        interest: Amount,
    ) -> KeystoneResult<()> {
        self.debt_principal = self.debt_principal.checked_sub(principal)?;
        self.debt_interest_due = self.debt_interest_due.checked_sub(interest)?;
        Ok(())
    }

    pub fn accrue_protocol_fee(&mut self, amount: Amount) -> KeystoneResult<()> {
        self.protocol_fees = self.protocol_fees.checked_add(amount)?;
        Ok(())
    }

    pub fn withdraw_protocol_fee(&mut self, amount: Amount) -> KeystoneResult<()> {
        if amount > self.protocol_fees || amount > self.cash {
            return Err(KeystoneError::InsufficientCash);
        }
        self.protocol_fees = self.protocol_fees.checked_sub(amount)?;
        self.cash = self.cash.checked_sub(amount)?;
        Ok(())
    }

    pub fn distribute_interest(
        &mut self,
        policy: ProtocolPolicy,
    ) -> KeystoneResult<Vec<InterestClaim>> {
        if self.share_supply.is_zero() || self.realized_interest.is_zero() {
            return Ok(Vec::new());
        }
        let protocol_fee = policy.protocol_fee(self.realized_interest)?;
        let distributable = self.realized_interest.checked_sub(protocol_fee)?;
        if distributable > self.cash {
            return Err(KeystoneError::InsufficientCash);
        }
        let mut claims = Vec::with_capacity(self.accounts.len());
        let mut allocated = Amount::ZERO;
        for (owner, shares) in &self.accounts {
            let amount = split_by_shares(distributable, *shares, self.share_supply)?;
            if !amount.is_zero() {
                allocated = allocated.checked_add(amount)?;
                claims.push(InterestClaim {
                    owner: *owner,
                    amount,
                });
            }
        }
        let remainder = distributable.checked_sub(allocated)?;
        if let Some(last) = claims.last_mut() {
            last.amount = last.amount.checked_add(remainder)?;
            allocated = allocated.checked_add(remainder)?;
        }
        self.cash = self.cash.checked_sub(allocated)?;
        self.realized_interest = Amount::ZERO;
        self.protocol_fees = self.protocol_fees.checked_add(protocol_fee)?;
        self.distributed_interest = self.distributed_interest.checked_add(allocated)?;
        Ok(claims)
    }

    pub fn snapshot(&self) -> KeystoneResult<VaultSnapshot> {
        Ok(VaultSnapshot {
            id: self.id,
            name: self.name.clone(),
            role: self.role,
            cash: self.cash,
            share_supply: self.share_supply,
            gross_assets: self.gross_assets()?,
            liabilities: self.liabilities()?,
            nav: self.net_asset_value()?,
            outstanding_principal: self.outstanding_principal,
            expected_interest: self.expected_interest,
            realized_interest: self.realized_interest,
            debt_principal: self.debt_principal,
            debt_interest_due: self.debt_interest_due,
            locked_collateral: self.locked_collateral,
            loss_reserve: self.loss_reserve,
            protocol_fees: self.protocol_fees,
            distributed_interest: self.distributed_interest,
            utilization_bps: self.utilization_bps()?,
            holder_count: self.accounts.len(),
        })
    }

    pub fn digest(&self) -> KeystoneResult<Digest> {
        let snapshot = self.snapshot()?;
        let bytes = serde_json::to_vec(&snapshot)
            .map_err(|error| KeystoneError::serialization(error.to_string()))?;
        Ok(Digest::from_parts("keystonexdtl-vault-state-v1", &[&bytes]))
    }

    pub fn assert_sane(&self) -> KeystoneResult<()> {
        if self.share_supply.is_zero() && !self.accounts.is_empty() {
            return Err(KeystoneError::invariant("zero supply with active accounts"));
        }
        let account_supply = self
            .accounts
            .values()
            .try_fold(Shares::ZERO, |total, shares| total.checked_add(*shares))?;
        if account_supply != self.share_supply {
            return Err(KeystoneError::invariant("share supply mismatch"));
        }
        let _ = self.net_asset_value()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetId, ProtocolPolicy};

    fn vault() -> Vault {
        let asset = AssetId::native();
        Vault::new(
            VaultId::named("alpha", asset),
            "Alpha",
            asset,
            VaultRole::Hybrid,
        )
    }

    #[test]
    fn deposit_mints_initial_one_to_one_shares() {
        let owner = AccountId::named("owner");
        let mut vault = vault();
        let receipt = vault.deposit(owner, Amount(1_000)).unwrap();
        assert_eq!(receipt.shares, Shares(1_000));
        assert_eq!(vault.net_asset_value().unwrap(), Amount(1_000));
        assert_eq!(vault.shares_of(owner), Shares(1_000));
    }

    #[test]
    fn redemption_uses_current_nav() {
        let owner = AccountId::named("owner");
        let mut vault = vault();
        vault.deposit(owner, Amount(1_000)).unwrap();
        vault.force_cash_credit(Amount(250)).unwrap();
        let quote = vault.quote_redeem(owner, Shares(500)).unwrap();
        assert_eq!(quote.amount, Amount(625));
    }

    #[test]
    fn interest_distribution_allocates_to_shareholders() {
        let alice = AccountId::named("alice");
        let bob = AccountId::named("bob");
        let mut vault = vault();
        vault.deposit(alice, Amount(1_000)).unwrap();
        vault.deposit(bob, Amount(1_000)).unwrap();
        vault.realized_interest = Amount(100);
        vault.cash = vault.cash.checked_add(Amount(100)).unwrap();
        let claims = vault
            .distribute_interest(ProtocolPolicy::default_prime().unwrap())
            .unwrap();
        let total = Amount::checked_sum(claims.iter().map(|claim| claim.amount)).unwrap();
        assert_eq!(total, Amount(100));
    }
}
