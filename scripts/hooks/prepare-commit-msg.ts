#!/usr/bin/env bun
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import { classifyChanges, stagedFiles, suggestedType } from "../lib/changes";

const file = Bun.argv[2];
const source = Bun.argv[3] ?? "";
if (source === "merge" || source === "squash") process.exit(0);

const paths = stagedFiles();
if (paths.length === 0) process.exit(0);

const kinds = classifyChanges(paths);
const message = readFileSync(file, "utf8");
const type = message.match(/^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)!?: /)?.[1];
const suggestion = suggestedType(kinds);

if ((type === "feat" || type === "fix") && !kinds.has("source") && kinds.size === 1) {
  const replacement = suggestion === "chore or build" ? "chore" : suggestion;
  writeFileSync(file, message.replace(new RegExp(`^${type}`), replacement));
  console.log(`prepare-commit-msg: using ${replacement} for the staged file category.`);
}

if (source !== "message" || !type) {
  appendFileSync(file, `\n# Staged files suggest: ${suggestion}
# Use an unscoped Conventional Commit: <type>: <description>
# Start the description lowercase and omit the final period.
# Do not include co-author, sign-off, or generated-by attribution.
`);
}
