#!/usr/bin/env bun
import { readFileSync } from "node:fs";
import { classifyChanges, stagedFiles, suggestedType } from "../lib/changes";
import { fail } from "../lib/command";

const message = readFileSync(Bun.argv[2], "utf8");
const subject = message.split(/\r?\n/, 1)[0] ?? "";
const match = subject.match(/^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)!?: (.+)$/);

if (!match) fail("commit-msg", "use <type>: <description> without a component or scope.");
const [, type, description] = match;
if (!/^[a-z]/.test(description)) fail("commit-msg", "start the description with a lowercase letter.");
if (description.endsWith(".")) fail("commit-msg", "omit the final period from the subject.");
if (/^\s*(co[- ]authored[- ]by|co[- ]developed[- ]by|signed[- ]off[- ]by|reviewed[- ]by|tested[- ]by|acked[- ]by|generated[- ]by|assisted[- ]by):/im.test(message)) {
  fail("commit-msg", "remove attribution trailers (co-author, sign-off, reviewed-by, or generated-by).");
}
if (/^\s*(?:🤖\s*)?(?:generated|written|assisted) (?:with|by) \[?(?:Claude|Codex|ChatGPT|OpenAI|Anthropic)\b/im.test(message)) {
  fail("commit-msg", "remove generated-by attribution from the commit body.");
}

const paths = stagedFiles();
if (paths.length === 0) process.exit(0);
const kinds = classifyChanges(paths);
if ((type === "feat" || type === "fix") && !kinds.has("source")) {
  fail("commit-msg", `${type} requires implementation changes; staged files suggest ${suggestedType(kinds)}.`);
}
if (type === "test" && !kinds.has("test") && !kinds.has("source")) {
  fail("commit-msg", "test commits must change tests or implementation source.");
}
