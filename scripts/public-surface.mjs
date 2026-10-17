import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("../", import.meta.url);
const roots = ["README.md", "SECURITY.md", "docs"];
const forbidden = [
    ["c", "tf"].join(""),
    ["labor", "atorio"].join(""),
    ["vulnerab", "ilidad"].join(""),
    ["vulnerab", "ility"].join(""),
    ["expl", "oit"].join(""),
];

function files(path) {
    const absolute = new URL(path, root);
    if (statSync(absolute).isFile()) return [absolute];
    return readdirSync(absolute, { withFileTypes: true }).flatMap((entry) => {
        const child = join(path, entry.name);
        return entry.isDirectory() ? files(child) : [new URL(child, root)];
    });
}

const findings = [];
for (const path of roots.flatMap(files)) {
    const content = readFileSync(path, "utf8").toLowerCase();
    for (const token of forbidden) {
        if (content.includes(token)) {
            findings.push(`${relative(root.pathname, path.pathname)}: ${token}`);
        }
    }
}

if (findings.length > 0) {
    console.error(findings.join("\n"));
    process.exit(1);
}

console.log(`public surface verified across ${roots.length} roots`);
