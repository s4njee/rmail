// Epic 2.4 guard: every `var(--token)` referenced by a component CSS file
// must resolve under BOTH treatments — that is, be defined on `:root` or in
// each `[data-theme]` block. That is exactly the property a hypothetical
// third treatment needs: adding one requires only a new `[data-theme]` block,
// never a component edit.
//
//   node scripts/check-token-usage.mjs
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));

// Component-local custom properties — intentionally defined at the use site
// (e.g. a row sets `--account-color` inline from account data) rather than in
// tokens.css. The guard ignores these; it exists to catch *theme* tokens that
// a third treatment would break.
const COMPONENT_LOCAL = new Set(["--account-color"]);

function walk(dir, out) {
  for (const entry of readdirSync(join(root, dir), { withFileTypes: true })) {
    const p = join(dir, entry.name);
    if (entry.isDirectory()) walk(p, out);
    else if (p.endsWith(".css")) out.push(p);
  }
}

const cssFiles = [];
walk("src", cssFiles);

// Parse token definitions per scope from tokens.css.
const tokens = readFileSync(join(root, "src/styles/tokens.css"), "utf8");
const scopes = { root: new Set(), hairline: new Set(), banded: new Set() };
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
    const m = line.match(/^\s*(--[\w-]+)\s*:/);
    if (m) scopes[current].add(m[1]);
  }
}

let failures = 0;
const seen = new Set();
for (const file of cssFiles) {
  if (file === "src/styles/tokens.css") continue;
  const css = readFileSync(join(root, file), "utf8");
  for (const m of css.matchAll(/var\(\s*(--[\w-]+)\s*(?:,[^)]*)?\)/g)) {
    const name = m[1];
    if (seen.has(name)) continue;
    seen.add(name);
    if (COMPONENT_LOCAL.has(name)) continue;

    const inRoot = scopes.root.has(name);
    const resolvesHairline = inRoot || scopes.hairline.has(name);
    const resolvesBanded = inRoot || scopes.banded.has(name);
    if (!resolvesHairline || !resolvesBanded) {
      failures += 1;
      const missing = [];
      if (!resolvesHairline) missing.push('[data-theme="hairline"]');
      if (!resolvesBanded) missing.push('[data-theme="banded"]');
      console.error(
        `FAIL: ${relative(root, file)} uses var(${name}), which is not defined on :root ` +
          `or in ${missing.join(" and ")} — a third treatment would break.`,
      );
    }
  }
}

if (failures > 0) {
  console.error(
    `\n${failures} token(s) referenced by components do not resolve under both treatments.`,
  );
  process.exit(1);
}
console.log(
  `OK: all ${seen.size} var(--token) references in component CSS resolve under both treatments.`,
);

// A token's VALUE must not reference an undefined token (e.g. a typo'd
// --space-90px): that silently invalidates the whole declaration.
const definedTokens = new Set([
  ...scopes.root,
  ...scopes.hairline,
  ...scopes.banded,
]);
const badRefs = [];
for (const m of tokens.matchAll(/var\(\s*(--[\w-]+)\s*\)/g)) {
  if (!definedTokens.has(m[1])) badRefs.push(m[1]);
}
if (badRefs.length > 0) {
  console.error(
    `\n${badRefs.length} undefined token(s) referenced inside tokens.css: ${[
      ...new Set(badRefs),
    ].join(", ")}.`,
  );
  process.exit(1);
}
console.log("OK: every var() inside tokens.css references a defined token.");
