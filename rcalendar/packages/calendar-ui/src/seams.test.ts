import { describe, expect, it } from "vitest";
import * as fs from "fs";
import * as path from "path";

function findFiles(dir: string, ext: string): string[] {
  let results: string[] = [];
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== "node_modules" && entry.name !== "dist") {
        results = results.concat(findFiles(full, ext));
      }
    } else if (entry.name.endsWith(ext) && !entry.name.includes(".test.")) {
      results.push(full);
    }
  }
  return results;
}

describe("Seam Isolation (plan.md §8 & S7.3)", () => {
  it("packages/calendar-ui/src MUST NEVER import @tauri-apps/api", () => {
    const srcDir = path.resolve(__dirname);
    const tsFiles = findFiles(srcDir, ".ts").concat(findFiles(srcDir, ".tsx"));

    for (const file of tsFiles) {
      const content = fs.readFileSync(file, "utf-8");
      const hasForbiddenImport =
        content.includes("from '@tauri-apps/api") ||
        content.includes('from "@tauri-apps/api') ||
        content.includes("import('@tauri-apps/api");
      expect(
        hasForbiddenImport,
        `Forbidden import in ${file}: @tauri-apps/api must not be imported in calendar-ui!`,
      ).toBe(false);
    }
  });

  it("packages/calendar-ui/src MUST NEVER import rusqlite, sqlite, or node:sqlite", () => {
    const srcDir = path.resolve(__dirname);
    const tsFiles = findFiles(srcDir, ".ts").concat(findFiles(srcDir, ".tsx"));

    for (const file of tsFiles) {
      const content = fs.readFileSync(file, "utf-8");
      const hasForbiddenSqlite =
        content.includes("from 'rusqlite") ||
        content.includes("from 'sqlite3") ||
        content.includes("from 'better-sqlite3") ||
        content.includes("from 'node:sqlite");
      expect(
        hasForbiddenSqlite,
        `Forbidden dependency in ${file}: calendar-ui must not import SQLite!`,
      ).toBe(false);
    }
  });
});
