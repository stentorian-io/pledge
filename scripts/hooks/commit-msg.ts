#!/usr/bin/env bun
import { readFileSync } from "node:fs";
import { classifyChanges, stagedFiles, suggestedType } from "../lib/changes";
import { fail } from "../lib/command";
import { validateMessage } from "../lib/commit-message.cjs";

const message = readFileSync(Bun.argv[2], "utf8");
const error = validateMessage(message);
if (error) fail("commit-msg", error);
const type = message.split(/[!:]/, 1)[0];

const paths = stagedFiles();
if (paths.length === 0) process.exit(0);
const kinds = classifyChanges(paths);
if ((type === "feat" || type === "fix") && !kinds.has("source")) {
  fail("commit-msg", `${type} requires implementation changes; staged files suggest ${suggestedType(kinds)}.`);
}
if (type === "test" && !kinds.has("test") && !kinds.has("source")) {
  fail("commit-msg", "test commits must change tests or implementation source.");
}
