#!/usr/bin/env bun
import { readFileSync } from "node:fs";
import { isRustInput, isToolingInput } from "../lib/changes";
import { checkCoverage, checkLsp, checkTools } from "../lib/checks";
import { repoRoot, run } from "../lib/command";

process.chdir(repoRoot());
let rustChanged = false;
let toolsChanged = false;
const updates = readFileSync(0, "utf8").trim().split(/\r?\n/).filter(Boolean);

for (const update of updates) {
  const [, localOid, , remoteOid] = update.split(/\s+/);
  if (/^0+$/.test(localOid)) continue;
  if (/^0+$/.test(remoteOid)) {
    rustChanged = toolsChanged = true;
    continue;
  }
  const diff = run("git", ["diff", "--name-only", "--no-renames", "-z", remoteOid, localOid], {
    capture: true,
    allowFailure: true,
  });
  if (diff.status !== 0) {
    rustChanged = toolsChanged = true;
    continue;
  }
  const paths = diff.stdout.split("\0").filter(Boolean);
  rustChanged ||= paths.some(isRustInput);
  toolsChanged ||= paths.some(isToolingInput);
}

if (toolsChanged) checkTools();
if (rustChanged) {
  checkLsp();
  checkCoverage();
} else console.log("pre-push: no Rust or fixture changes in the pushed refs; skipping LSP and coverage checks.");
