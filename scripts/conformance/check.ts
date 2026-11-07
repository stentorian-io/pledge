#!/usr/bin/env bun
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { load } from "./inventory";
import { render, updateDocument } from "./report";

const root = fileURLToPath(new URL("../../", import.meta.url));
try {
  const mode = Bun.argv[2];
  if (mode && mode !== "--write") throw new Error("usage: bun scripts/conformance/check.ts [--write]");
  const loaded = load(root);
  const path = resolve(root, "docs/language-coverage.md");
  const before = readFileSync(path, "utf8");
  const after = updateDocument(before, render(loaded));
  if (mode === "--write") writeFileSync(path, after);
  else if (before !== after) throw new Error("public coverage report is stale; run make language-coverage-update");
  console.log(`language coverage: ${loaded.references.length} references classified; ${loaded.inventory.evidence.length} evidence links checked (not a conformance pass rate)`);
} catch (error) {
  console.error(`language coverage: ${(error as Error).message}`);
  process.exitCode = 1;
}
