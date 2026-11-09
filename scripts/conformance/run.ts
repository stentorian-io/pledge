#!/usr/bin/env bun
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { load } from "./inventory";
import { sha256 } from "./sources";

const root = fileURLToPath(new URL("../../", import.meta.url));
const selected: string[] = [];
const filters: { target: string; pattern: string }[] = [];
let output: string | undefined;
let binaries: string | undefined;
type Result = { target: string; name: string; evidence: string[]; status: "pending" | "passed" | "failed"; log?: string };
const report = { schema: 1, status: "running", commit: "", dirty: false, inventorySha256: "", profiles: selected, node: "", typescript: "", npm: "", results: [] as Result[], error: "" };
function save() {
  if (output) { mkdirSync(dirname(output), { recursive: true }); writeFileSync(output, JSON.stringify(report, null, 2) + "\n"); }
}
function command(executable: string, args: string[], timeout = 600_000) {
  const result = spawnSync(executable, args, { cwd: root, encoding: "utf8", timeout, maxBuffer: 16 * 1024 * 1024 });
  const log = (result.stdout ?? "") + (result.stderr ?? "");
  if (result.error || result.status !== 0) throw new Error(`${executable} ${args.join(" ")}: ${result.error?.message ?? `exit ${result.status}`}\n${log}`);
  return log;
}
function testCommand(target: string, args: string[]) {
  if (binaries) {
    if (!/^[a-zA-Z0-9_-]+$/.test(target)) throw new Error(`invalid test target: ${target}`);
    const manifest = JSON.parse(readFileSync(resolve(binaries, "manifest.json"), "utf8"));
    if (manifest.schema !== 1 || manifest.commit !== report.commit || manifest.root !== resolve(root)
      || !Array.isArray(manifest.targets) || !manifest.targets.includes(target)) {
      throw new Error(`compiled tests do not match this checkout or target: ${target}`);
    }
    return command(resolve(binaries, target), args);
  }
  return command("cargo", ["test", "--locked", "--release", "--features", "lsp-tests", "--test", target, "--", ...args]);
}

try {
  const args = Bun.argv.slice(2);
  while (args.length) {
    const flag = args.shift(), value = args.shift();
    if (flag === "--profile" && value) selected.push(value);
    else if (flag === "--output" && value) output = resolve(value);
    else if (flag === "--binaries" && value) binaries = resolve(value);
    else if (flag === "--filter" && value?.includes("/")) {
      const separator = value.indexOf("/");
      const target = value.slice(0, separator), pattern = value.slice(separator + 1);
      if (!target || !pattern) throw new Error("filters require a target and a nonempty test pattern");
      filters.push({ target, pattern });
    } else throw new Error("usage: bun scripts/conformance/run.ts --profile <id> [--filter <target/pattern>] [--binaries <directory>] --output <path>");
  }
  if (!selected.length || !output || new Set(selected).size !== selected.length) throw new Error("select unique profiles and a result output path");
  const { inventory } = load(root);
  report.commit = command("git", ["rev-parse", "HEAD"]).trim();
  report.dirty = command("git", ["status", "--porcelain"]).trim().length !== 0;
  if (binaries && report.dirty) throw new Error("prebuilt consumer tests require a clean checkout");
  report.inventorySha256 = sha256(readFileSync(resolve(root, "docs/coverage/inventory.json")));
  report.node = command("node", ["--version"], 30_000).trim().replace(/^v/, "");
  const profiles = selected.map(id => {
    const profile = inventory.profiles.find(profile => profile.id === id);
    if (!profile) throw new Error(`unknown execution profile: ${id}`);
    return profile;
  });
  if (profiles.some(profile => profile.typescript.length)) report.typescript = command("tsc", ["--version"], 30_000).trim().replace(/^Version /, "");
  if (profiles.some(profile => profile.npm?.length)) report.npm = command("npm", ["--version"], 30_000).trim();
  for (const profile of profiles) {
    if (!profile.node.includes(report.node)) throw new Error(`${profile.id}: Node ${report.node} is outside the declared exact versions`);
    if (profile.typescript.length && !profile.typescript.includes(report.typescript)) throw new Error(`${profile.id}: TypeScript ${report.typescript} is outside the declared exact versions`);
    if (profile.npm?.length && !profile.npm.includes(report.npm)) throw new Error(`${profile.id}: npm ${report.npm} is outside the declared exact versions`);
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
  const targets = new Set([...report.results.map(result => result.target), ...filters.map(filter => filter.target)]);
  for (const target of targets) {
    const listing = testCommand(target, ["--list"]);
    const registered = new Set([...listing.matchAll(/^(\S+): test$/gm)].map(match => match[1]));
    for (const result of report.results.filter(result => result.target === target)) if (!registered.has(result.name)) throw new Error(`coverage test is not registered: ${target}/${result.name}`);
    for (const filter of filters.filter(filter => filter.target === target)) {
      const matches = [...registered].filter(name => name.includes(filter.pattern));
      if (!matches.length) throw new Error(`test filter matched nothing: ${target}/${filter.pattern}`);
      for (const name of matches) {
        const key = `${target}/${name}`;
        if (!tests.has(key)) tests.set(key, { target, name, evidence: [], status: "pending" });
      }
    }
  }
  report.results = [...tests.values()]; save();
  for (const target of targets) {
    const batch = report.results.filter(result => result.target === target);
    console.log(`coverage consumers: ${target}, ${batch.length} exact tests`);
    const started = performance.now();
    try {
      const log = testCommand(target, [...batch.map(result => result.name), "--exact", "--format=pretty", "--color=never", "--test-threads=4"]);
      const passed = [...log.matchAll(/^test (\S+)(?: - should panic)? \.\.\. ok$/gm)].map(match => match[1]);
      if (passed.length !== batch.length || new Set(passed).size !== batch.length
        || batch.some(result => !passed.includes(result.name))
        || !log.includes(`test result: ok. ${batch.length} passed; 0 failed; 0 ignored;`)) {
        throw new Error(`selected tests were skipped, duplicated, or did not run exactly once:\n${log}`);
      }
      for (const result of batch) { result.status = "passed"; result.log = `test ${result.name} ... ok`; }
    } catch (error) {
      console.error(`coverage consumers: ${target}: ${(error as Error).message}`);
      for (const result of batch) { result.status = "failed"; result.log = (error as Error).message; }
    }
    console.log(`${target} completed in ${((performance.now() - started) / 1000).toFixed(2)}s`);
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
