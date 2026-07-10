import assert from "node:assert/strict";
import test from "node:test";

import { assertCommon, operation, scenario, vault } from "../helpers/scenario_helpers.js";

test("redistribution pays realized interest to vault shareholders", () => {
  const payload = scenario("redistribution");
  assertCommon(payload, "redistribution");
  const senior = vault(payload, "atlas-income");
  const distribution = operation(payload, "distribute_interest");

  assert.equal(distribution.amount, 8_955);
  assert.equal(senior.realized_interest, 0);
  assert.equal(senior.distributed_interest, 8_955);
  assert.equal(senior.cash, 1_500_045);
});
