// Epic 2.1 — assert the token file still matches the README §Design Tokens
// tables (transcribed into scripts/fixtures/readme-tokens.mjs). Catches drift
// when someone edits a hex value in one place and forgets the other.
//
//   node scripts/check-readme-drift.mjs
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import expected from "./fixtures/readme-tokens.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const tokens = readFileSync(join(root, "src/styles/tokens.css"), "utf8");

// Extract per-scope token map.
const scopes = { root: {}, hairline: {}, banded: {} };
let current = null;
for (const line of tokens.split("\n")) {
  const rootOpen = line.match(/^\s*:root\s*\{/);
  const themeOpen = line.match(/^\[data-theme="([^"]+)"\]\s*\{/);
  if (rootOpen) {
    current = "root";
    continue;
  }
  if (themeOpen) {
    current = themeOpen[1];
    continue;
  }
  if (current && /^\s*}\s*$/.test(line)) {
    current = null;
    continue;
  }
  if (current) {
    // Tolerate inline `/* … */` comments after the value.
    const m = line.match(/^\s*(--[\w-]+)\s*:\s*(.+?)\s*;/);
    if (m) scopes[current][m[1]] = m[2];
  }
}

// Resolve one level of bare var() (same scope first, then :root).
function resolve(scope, name) {
  const raw = scopes[scope][name] ?? scopes.root[name];
  if (raw === undefined) return undefined;
  const ref = raw.match(/^var\(\s*(--[\w-]+)\s*\)$/);
  if (ref) return scopes[scope][ref[1]] ?? scopes.root[ref[1]];
  return raw;
}

let failures = 0;
const report = (scope, name, message) => {
  failures += 1;
  console.error(`FAIL: ${name} [${scope}] — ${message}`);
};

for (const [scope, entries] of Object.entries(expected)) {
  for (const [name, readmeValue] of Object.entries(entries)) {
    const actual = resolve(scope, name);
    if (actual === undefined) {
      report(scope, name, `missing from tokens.css (README: ${readmeValue})`);
    } else if (actual !== readmeValue) {
      report(
        scope,
        name,
        `tokens.css has ${actual}, README says ${readmeValue}`,
      );
    }
  }
}

if (failures > 0) {
  console.error(
    `\n${failures} drift(s) between README §Design Tokens and tokens.css.`,
  );
  process.exit(1);
}
console.log("OK: tokens.css matches the README §Design Tokens tables.");
