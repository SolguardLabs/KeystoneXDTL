use std::collections::BTreeMap;

use crate::{
    AccountId, Amount, EngineConfig, KeystoneEngine, KeystoneError, KeystoneResult,
    OperationReport, ProtocolPolicy, RepaymentMode, RiskTier, ScenarioReport, Shares, VaultId,
    VaultRole,
};

#[derive(Clone)]
struct ScenarioAccounts {
    senior_lp: AccountId,
    junior_lp: AccountId,
    borrower_ops: AccountId,
    treasury: AccountId,
}

#[derive(Clone)]
struct ScenarioVaults {
    senior: VaultId,
    junior: VaultId,
    borrower: VaultId,
    treasury: VaultId,
}

struct Fixture {
    engine: KeystoneEngine,
    accounts: ScenarioAccounts,
    vaults: ScenarioVaults,
    operations: Vec<OperationReport>,
}

impl ScenarioAccounts {
    fn new() -> Self {
        Self {
            senior_lp: AccountId::named("senior-lp"),
            junior_lp: AccountId::named("junior-lp"),
            borrower_ops: AccountId::named("borrower-ops"),
            treasury: AccountId::named("treasury"),
        }
    }

    fn aliases(&self) -> BTreeMap<String, AccountId> {
        BTreeMap::from([
            ("senior_lp".to_owned(), self.senior_lp),
            ("junior_lp".to_owned(), self.junior_lp),
            ("borrower_ops".to_owned(), self.borrower_ops),
            ("treasury".to_owned(), self.treasury),
        ])
    }
}

impl ScenarioVaults {
    fn aliases(&self) -> BTreeMap<String, VaultId> {
        BTreeMap::from([
            ("senior".to_owned(), self.senior),
            ("junior".to_owned(), self.junior),
            ("borrower".to_owned(), self.borrower),
            ("treasury".to_owned(), self.treasury),
        ])
    }
}

impl Fixture {
    fn new() -> KeystoneResult<Self> {
        let asset = crate::AssetId::native();
        let mut engine = KeystoneEngine::new(EngineConfig::local(asset)?)?;
        engine.set_policy(ProtocolPolicy::for_tier(RiskTier::Prime)?)?;
        let senior = engine.register_vault("atlas-income", VaultRole::Liquidity)?;
        let junior = engine.register_vault("bridge-buffer", VaultRole::Hybrid)?;
        let borrower = engine.register_vault("delta-maker", VaultRole::Borrower)?;
        let treasury = engine.register_vault("keystone-treasury", VaultRole::Treasury)?;
        let accounts = ScenarioAccounts::new();
        let vaults = ScenarioVaults {
            senior,
            junior,
            borrower,
            treasury,
        };
        let mut fixture = Self {
            engine,
            accounts,
            vaults,
            operations: Vec::new(),
        };
        fixture.seed()?;
        Ok(fixture)
    }

    fn seed(&mut self) -> KeystoneResult<()> {
        let receipt = self.engine.deposit(
            self.vaults.senior,
            self.accounts.senior_lp,
            Amount(1_000_000),
        )?;
        self.operations.push(OperationReport::vault_shares(
            "seed_senior",
            self.vaults.senior,
            receipt.amount,
            receipt.shares,
        ));
        let receipt =
            self.engine
                .deposit(self.vaults.senior, self.accounts.junior_lp, Amount(500_000))?;
        self.operations.push(OperationReport::vault_shares(
            "seed_secondary_senior",
            self.vaults.senior,
            receipt.amount,
            receipt.shares,
        ));
        let receipt =
            self.engine
                .deposit(self.vaults.junior, self.accounts.junior_lp, Amount(250_000))?;
        self.operations.push(OperationReport::vault_shares(
            "seed_junior",
            self.vaults.junior,
            receipt.amount,
            receipt.shares,
        ));
        let receipt = self.engine.deposit(
            self.vaults.borrower,
            self.accounts.borrower_ops,
            Amount(650_000),
        )?;
        self.operations.push(OperationReport::vault_shares(
            "seed_borrower",
            self.vaults.borrower,
            receipt.amount,
            receipt.shares,
        ));
        let receipt =
            self.engine
                .deposit(self.vaults.treasury, self.accounts.treasury, Amount(50_000))?;
        self.operations.push(OperationReport::vault_shares(
            "seed_treasury",
            self.vaults.treasury,
            receipt.amount,
            receipt.shares,
        ));
        Ok(())
    }

    fn build(self, name: &str) -> KeystoneResult<ScenarioReport> {
        ScenarioReport::build(
            name,
            &self.engine,
            self.vaults.aliases(),
            self.accounts.aliases(),
            self.operations,
        )
    }

    fn open_primary_loan(&mut self) -> KeystoneResult<crate::LoanId> {
        let loan = self.engine.open_loan(
            self.vaults.senior,
            self.vaults.borrower,
            Amount(300_000),
            Amount(360_000),
            90,
        )?;
        self.operations
            .push(OperationReport::loan("open_primary", loan, Amount(300_000)));
        Ok(loan)
    }

    fn open_bridge_loan(&mut self) -> KeystoneResult<crate::LoanId> {
        let loan = self.engine.open_loan(
            self.vaults.junior,
            self.vaults.borrower,
            Amount(80_000),
            Amount(96_000),
            45,
        )?;
        self.operations
            .push(OperationReport::loan("open_bridge", loan, Amount(80_000)));
        Ok(loan)
    }
}

pub fn run_named(name: &str) -> KeystoneResult<ScenarioReport> {
    match name {
        "loan" => loan(),
        "repayment" => repayment(),
        "prepayment" => prepayment(),
        "default" => default_case(),
        "liquidation" => liquidation(),
        "redistribution" => redistribution(),
        "portfolio" => portfolio(),
        "snapshot" => snapshot(),
        other => Err(KeystoneError::scenario(other)),
    }
}

pub fn snapshot() -> KeystoneResult<ScenarioReport> {
    Fixture::new()?.build("snapshot")
}

pub fn loan() -> KeystoneResult<ScenarioReport> {
    let mut fixture = Fixture::new()?;
    fixture.open_primary_loan()?;
    fixture.engine.push_invariant_event()?;
    fixture.build("loan")
}

pub fn repayment() -> KeystoneResult<ScenarioReport> {
    let mut fixture = Fixture::new()?;
    let loan = fixture.open_primary_loan()?;
    fixture.engine.advance_by(90)?;
    let quote = fixture
        .engine
        .repay_loan(loan, RepaymentMode::Scheduled, None)?;
    fixture.operations.push(OperationReport::loan(
        "scheduled_repay",
        loan,
        quote.total()?,
    ));
    fixture.engine.push_invariant_event()?;
    fixture.build("repayment")
}

pub fn prepayment() -> KeystoneResult<ScenarioReport> {
    let mut fixture = Fixture::new()?;
    let loan = fixture.open_primary_loan()?;
    fixture.engine.advance_by(30)?;
    let quote = fixture
        .engine
        .repay_loan(loan, RepaymentMode::Early, None)?;
    fixture
        .operations
        .push(OperationReport::loan("early_repay", loan, quote.total()?));
    let redeem = fixture.engine.redeem(
        fixture.vaults.senior,
        fixture.accounts.junior_lp,
        Shares(125_000),
    )?;
    fixture.operations.push(OperationReport::vault_shares(
        "routine_redeem",
        fixture.vaults.senior,
        redeem.amount,
        redeem.shares,
    ));
    fixture.engine.push_invariant_event()?;
    fixture.build("prepayment")
}

pub fn default_case() -> KeystoneResult<ScenarioReport> {
    let mut fixture = Fixture::new()?;
    let loan = fixture.open_primary_loan()?;
    fixture.engine.advance_by(95)?;
    let overdue = fixture.engine.mark_default(loan)?;
    fixture.operations.push(OperationReport::loan(
        format!("mark_default_{overdue}"),
        loan,
        Amount(overdue),
    ));
    fixture.engine.push_invariant_event()?;
    fixture.build("default")
}

pub fn liquidation() -> KeystoneResult<ScenarioReport> {
    let mut fixture = Fixture::new()?;
    let loan = fixture.open_primary_loan()?;
    fixture.engine.advance_by(95)?;
    fixture.engine.mark_default(loan)?;
    let result = fixture.engine.liquidate(loan)?;
    fixture.operations.push(OperationReport::loan(
        "liquidate_primary",
        loan,
        result.seized,
    ));
    fixture.engine.push_invariant_event()?;
    fixture.build("liquidation")
}

pub fn redistribution() -> KeystoneResult<ScenarioReport> {
    let mut fixture = Fixture::new()?;
    let loan = fixture.open_primary_loan()?;
    fixture.engine.advance_by(90)?;
    let quote = fixture
        .engine
        .repay_loan(loan, RepaymentMode::Scheduled, None)?;
    fixture.operations.push(OperationReport::loan(
        "scheduled_repay",
        loan,
        quote.total()?,
    ));
    let claims = fixture.engine.distribute_interest(fixture.vaults.senior)?;
    let total = Amount::checked_sum(claims.iter().map(|claim| claim.amount))?;
    fixture.operations.push(OperationReport::vault_amount(
        "distribute_interest",
        fixture.vaults.senior,
        total,
    ));
    fixture.engine.push_invariant_event()?;
    fixture.build("redistribution")
}

pub fn portfolio() -> KeystoneResult<ScenarioReport> {
    let mut fixture = Fixture::new()?;
    let primary = fixture.open_primary_loan()?;
    let bridge = fixture.open_bridge_loan()?;
    fixture.engine.advance_by(20)?;
    let quote = fixture
        .engine
        .repay_loan(bridge, RepaymentMode::Partial, Some(Amount(30_000)))?;
    fixture.operations.push(OperationReport::loan(
        "partial_bridge_repay",
        bridge,
        quote.total()?,
    ));
    fixture.engine.advance_by(70)?;
    let quote = fixture
        .engine
        .repay_loan(primary, RepaymentMode::Scheduled, None)?;
    fixture.operations.push(OperationReport::loan(
        "scheduled_primary_repay",
        primary,
        quote.total()?,
    ));
    fixture.engine.push_invariant_event()?;
    fixture.build("portfolio")
}
