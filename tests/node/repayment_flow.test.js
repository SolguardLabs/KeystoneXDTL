import assert from "node:assert/strict";
import test from "node:test";

import { assertCommon, loanOf, operation, scenario, vault } from "../helpers/scenario_helpers.js";

test("scheduled repayment closes principal and releases collateral", () => {
  const payload = scenario("repayment");
  assertCommon(payload, "repayment");
  const loan = loanOf(payload);
  const senior = vault(payload, "atlas-income");
  const borrower = vault(payload, "delta-maker");
  const repay = operation(payload, "scheduled_repay");

  assert.equal(loan.status, "paid");
  assert.equal(loan.remaining_principal, 0);
  assert.equal(loan.interest_paid, 9_000);
  assert.equal(loan.collateral_released, 360_000);
  assert.equal(repay.amount, 309_000);
  assert.equal(senior.cash, 1_509_000);
  assert.equal(senior.outstanding_principal, 0);
  assert.equal(borrower.debt_principal, 0);
  assert.equal(borrower.locked_collateral, 0);
});

test("early repayment closes the credit line and processes a normal redemption", () => {
  const payload = scenario("prepayment");
  assertCommon(payload, "prepayment");
  const loan = loanOf(payload);
  const redeem = operation(payload, "routine_redeem");

  assert.equal(loan.status, "paid");
  assert.equal(loan.remaining_principal, 0);
  assert.equal(loan.interest_paid, 3_000);
  assert.equal(loan.collateral_released, 360_000);
  assert.equal(redeem.shares, 125_000);
  assert.equal(redeem.amount, 126_000);
});
