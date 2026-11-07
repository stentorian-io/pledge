#!/usr/bin/env bun
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { load } from "./inventory";
import { sha256 } from "./sources";

const root = fileURLToPath(new URL("../../", import.meta.url));
const selected: string[] = [];
let output: string | undefined;
type Result = { target: string; name: string; evidence: string[]; status: "pending" | "passed" | "failed"; log?: string };
const report = { schema: 1, status: "running", commit: "", dirty: false, inventorySha256: "", profiles: selected, node: "", typescript: "", results: [] as Result[], error: "" };
function save() {
  if (output) { mkdirSync(dirname(output), { recursive: true }); writeFileSync(output, JSON.stringify(report, null, 2) + "\n"); }
}
function command(executable: string, args: string[], timeout = 600_000) {
  const result = spawnSync(executable, args, { cwd: root, encoding: "utf8", timeout, maxBuffer: 16 * 1024 * 1024 });
  const log = (result.stdout ?? "") + (result.stderr ?? "");
  if (result.error || result.status !== 0) throw new Error(`${executable} ${args.join(" ")}: ${result.error?.message ?? `exit ${result.status}`}\n${log}`);
  return log;
}

try {
  const args = Bun.argv.slice(2);
  while (args.length) {
    const flag = args.shift(), value = args.shift();
    if (flag === "--profile" && value) selected.push(value);
    else if (flag === "--output" && value) output = resolve(value);
    else throw new Error("usage: bun scripts/conformance/run.ts --profile <id> [--profile <id>] --output <path>");
  }
  if (!selected.length || !output || new Set(selected).size !== selected.length) throw new Error("select unique profiles and a result output path");
  const { inventory } = load(root);
  report.commit = command("git", ["rev-parse", "HEAD"]).trim();
  report.dirty = command("git", ["status", "--porcelain"]).trim().length !== 0;
  report.inventorySha256 = sha256(readFileSync(resolve(root, "docs/coverage/inventory.json")));
  report.node = command("node", ["--version"], 30_000).trim().replace(/^v/, "");
  const profiles = selected.map(id => {
    const profile = inventory.profiles.find(profile => profile.id === id);
    if (!profile) throw new Error(`unknown execution profile: ${id}`);
    return profile;
  });
  if (profiles.some(profile => profile.typescript.length)) report.typescript = command("tsc", ["--version"], 30_000).trim().replace(/^Version /, "");
  for (const profile of profiles) {
    if (!profile.node.includes(report.node)) throw new Error(`${profile.id}: Node ${report.node} is outside the declared exact versions`);
    if (profile.typescript.length && !profile.typescript.includes(report.typescript)) throw new Error(`${profile.id}: TypeScript ${report.typescript} is outside the declared exact versions`);
  }
  const evidence = inventory.evidence.filter(evidence => evidence.profiles.some(id => selected.includes(id)));
  if (!evidence.length) throw new Error("selected profiles have no evidence tests");
  const tests = new Map<string, Result>();
  for (const entry of evidence) {
    const key = `${entry.target}/${entry.name}`;
    if (!tests.has(key)) tests.set(key, { target: entry.target, name: entry.name, evidence: [], status: "pending" });
    tests.get(key)!.evidence.push(entry.id);
  }
  report.results = [...tests.values()]; save();
  for (const target of new Set(report.results.map(result => result.target))) {
    const listing = command("cargo", ["test", "--locked", "--release", "--features", "lsp-tests", "--test", target, "--", "--list"]);
    const registered = new Set([...listing.matchAll(/^(\S+): test$/gm)].map(match => match[1]));
    for (const result of report.results.filter(result => result.target === target)) if (!registered.has(result.name)) throw new Error(`coverage test is not registered: ${target}/${result.name}`);
  }
  for (const result of report.results) {
    console.log(`coverage consumer: ${result.target}/${result.name}`);
    try {
      result.log = command("cargo", ["test", "--locked", "--release", "--features", "lsp-tests", "--test", result.target, "--", result.name, "--exact", "--nocapture", "--test-threads=1"]);
      if (!/test result: ok\. 1 passed; 0 failed; 0 ignored;/.test(result.log)) throw new Error(`test was skipped or did not run exactly once:\n${result.log}`);
      result.status = "passed";
    } catch (error) { result.status = "failed"; result.log = (error as Error).message; }
    save();
  }
  if (report.results.some(result => result.status !== "passed")) throw new Error("one or more consumer evidence tests failed");
  report.status = "passed"; save();
  console.log(`passed ${report.results.length} exact evidence tests; results: ${output}`);
} catch (error) {
  report.status = "failed"; report.error = (error as Error).message; save();
  console.error(`coverage consumers: ${report.error}`);
  process.exitCode = 1;
}
