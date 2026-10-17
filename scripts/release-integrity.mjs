import { existsSync, readFileSync, statSync } from "node:fs";

const root = new URL("../", import.meta.url);
const expected = "1.0.0";
const required = [
    "README.md",
    "SECURITY.md",
    "assets/banner.png",
    "docs/arquitectura.md",
    "docs/modelo-economico.md",
    "docs/gobernanza.md",
    "docs/operacion.md",
    "docs/integracion.md",
];

const packageJson = JSON.parse(readFileSync(new URL("package.json", root), "utf8"));
const cargo = readFileSync(new URL("Cargo.toml", root), "utf8");
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];

if (packageJson.version !== expected || cargoVersion !== expected) {
    throw new Error(`version mismatch: npm=${packageJson.version} cargo=${cargoVersion}`);
}

for (const path of required) {
    const absolute = new URL(path, root);
    if (!existsSync(absolute) || statSync(absolute).size === 0) {
        throw new Error(`missing release artifact: ${path}`);
    }
}

if (statSync(new URL("assets/banner.png", root)).size < 100_000) {
    throw new Error("banner does not meet the production asset size gate");
}

const ref = process.env.GITHUB_REF ?? "";
if (ref.startsWith("refs/tags/") && ref !== `refs/tags/v${expected}`) {
    throw new Error(`tag ${ref} does not match package version ${expected}`);
}

console.log(`release integrity verified for v${expected}`);
