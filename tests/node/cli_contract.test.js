import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { assertCommon, scenario } from "../helpers/scenario_helpers.js";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..");
const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";

test("snapshot scenario exposes a stable JSON contract", () => {
  const payload = scenario("snapshot");
  assertCommon(payload, "snapshot");
  assert.equal(Object.keys(payload.engine.vaults).length, 4);
  assert.equal(Object.keys(payload.engine.loans).length, 0);
  assert.equal(payload.engine.totals.cash, 2_450_000);
  assert.equal(payload.engine.totals.nav, 2_450_000);
});

test("list command returns available scenarios", () => {
  const stdout = execFileSync(cargo, ["run", "--quiet", "--", "list"], {
    cwd: root,
    encoding: "utf8",
    env: { ...process.env, CARGO_TERM_COLOR: "never" },
    timeout: 120_000,
  });
  const names = stdout.trim().split(/\r?\n/u);
  assert.deepEqual(names, [
    "loan",
    "repayment",
    "prepayment",
    "default",
    "liquidation",
    "redistribution",
    "portfolio",
    "snapshot",
  ]);
});
