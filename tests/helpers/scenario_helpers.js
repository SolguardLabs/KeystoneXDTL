import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..");
const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
const cache = new Map();

export function scenario(name) {
  if (!cache.has(name)) {
    const stdout = execFileSync(cargo, ["run", "--quiet", "--", name], {
      cwd: root,
      encoding: "utf8",
      env: { ...process.env, CARGO_TERM_COLOR: "never" },
      timeout: 120_000,
    });
    cache.set(name, JSON.parse(stdout));
  }
  return cache.get(name);
}

export function loanOf(payload) {
  const loans = Object.values(payload.engine.loans);
  assert.equal(loans.length, 1);
  return loans[0];
}

export function assertHex32(value) {
  assert.equal(typeof value, "string");
  assert.match(value, /^[0-9a-f]{64}$/u);
}

export function assertCommon(payload, name) {
  assert.equal(payload.scenario, name);
  assert.equal(payload.engine.network_id, 8_812);
  assert.equal(payload.engine.conservation_ok, true);
  assertHex32(payload.engine.asset);
  assertHex32(payload.engine.state_digest);
  assertHex32(payload.engine.policy_digest);
  assertHex32(payload.engine.journal_digest);
  assert.ok(payload.engine.event_count >= 1);
  assert.ok(payload.vault_aliases.senior);
  assert.ok(payload.account_aliases.senior_lp);
}

export function vault(payload, name) {
  const value = payload.engine.vaults[name];
  assert.ok(value, `missing vault ${name}`);
  return value;
}

export function operation(payload, name) {
  const value = payload.operations.find((item) => item.name === name);
  assert.ok(value, `missing operation ${name}`);
  return value;
}
