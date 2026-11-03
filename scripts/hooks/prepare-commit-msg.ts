#!/usr/bin/env bun
import { appendFileSync, readFileSync } from "node:fs";
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

if (source !== "message" || !type) {
  appendFileSync(file, `\n# Staged files suggest: ${suggestion}
# Use an unscoped Conventional Commit: <type>: <description>
# Start the description lowercase and omit the final period.
# Write the user-visible outcome as a release note; internal work stays internal.
# Choose the type by compatibility impact, not the files changed.
# Explain breaking changes and migration in a BREAKING CHANGE: footer.
# Do not include co-author, sign-off, or generated-by attribution.
`);
}
