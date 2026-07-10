import assert from "node:assert/strict";
import test from "node:test";

import { assertCommon, loanOf, operation, scenario, vault } from "../helpers/scenario_helpers.js";

test("default scenario marks overdue credit without liquidating immediately", () => {
  const payload = scenario("default");
  assertCommon(payload, "default");
  const loan = loanOf(payload);

  assert.equal(payload.engine.epoch, 96);
  assert.equal(loan.status, "defaulted");
  assert.equal(loan.remaining_principal, 300_000);
  assert.equal(loan.collateral_locked, 360_000);
  assert.ok(payload.operations.some((item) => item.name === "mark_default_2"));
});

test("liquidation applies close factor and updates lender exposure", () => {
  const payload = scenario("liquidation");
  assertCommon(payload, "liquidation");
  const loan = loanOf(payload);
  const senior = vault(payload, "atlas-income");
  const liquidation = operation(payload, "liquidate_primary");

  assert.equal(loan.status, "defaulted");
  assert.equal(loan.remaining_principal, 150_000);
  assert.equal(loan.collateral_locked, 200_865);
  assert.equal(liquidation.amount, 159_135);
  assert.equal(senior.outstanding_principal, 150_000);
  assert.equal(senior.cash, 1_359_135);
});
