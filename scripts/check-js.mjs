import { readdirSync, statSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";

const roots = ["tests", "scripts"];
const files = [];

function walk(dir) {
    for (const entry of readdirSync(dir)) {
        const path = join(dir, entry);
        const stat = statSync(path);
        if (stat.isDirectory()) {
            walk(path);
            continue;
        }
        if (path.endsWith(".js") || path.endsWith(".mjs")) {
            files.push(path);
        }
    }
}

for (const root of roots) {
    walk(root);
}

for (const file of files) {
    execFileSync(process.execPath, ["--check", file], {
        stdio: "pipe",
    });
}

console.log(`checked ${files.length} JavaScript files`);
