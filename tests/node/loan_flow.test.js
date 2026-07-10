import assert from "node:assert/strict";
import test from "node:test";

import { assertCommon, loanOf, scenario, vault } from "../helpers/scenario_helpers.js";

test("loan scenario opens an internal credit line with collateral", () => {
  const payload = scenario("loan");
  assertCommon(payload, "loan");
  const loan = loanOf(payload);
  const senior = vault(payload, "atlas-income");
  const borrower = vault(payload, "delta-maker");

  assert.equal(loan.status, "active");
  assert.equal(loan.principal, 300_000);
  assert.equal(loan.remaining_principal, 300_000);
  assert.equal(loan.scheduled_interest, 9_000);
  assert.equal(loan.collateral_locked, 360_000);
  assert.equal(senior.cash, 1_200_000);
  assert.equal(senior.outstanding_principal, 300_000);
  assert.equal(borrower.debt_principal, 300_000);
  assert.equal(borrower.locked_collateral, 360_000);
});

test("portfolio scenario supports concurrent lender books", () => {
  const payload = scenario("portfolio");
  assertCommon(payload, "portfolio");
  assert.equal(Object.keys(payload.engine.loans).length, 2);
  const senior = vault(payload, "atlas-income");
  const bridge = vault(payload, "bridge-buffer");
  assert.equal(senior.outstanding_principal, 0);
  assert.equal(bridge.outstanding_principal, 50_000);
  assert.equal(payload.engine.totals.outstanding_principal, 50_000);
});
