#!/usr/bin/env bun
// Compile the runtime-independent Rust harness once for the Node/TypeScript matrix.
import { spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const directory = resolve(root, "target/consumer-tests");
const targets = ["javascript", "typescript", "node"];
const build = spawnSync("cargo", ["test", "--locked", "--release", "--features", "lsp-tests",
  ...targets.flatMap(target => ["--test", target]), "--no-run", "--message-format=json"],
{ cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024, stdio: ["ignore", "pipe", "inherit"] });
if (build.error) throw build.error;
if (build.status !== 0) process.exit(build.status ?? 1);
const artifacts = build.stdout.split("\n").filter(Boolean).map(line => JSON.parse(line));
mkdirSync(directory, { recursive: true });
for (const target of targets) {
  const artifact = artifacts.find(artifact => artifact.reason === "compiler-artifact"
    && artifact.target.kind.includes("test") && artifact.target.name === target && artifact.executable);
  if (!artifact) throw new Error(`missing compiled test target: ${target}`);
  copyFileSync(artifact.executable, resolve(directory, target));
  chmodSync(resolve(directory, target), 0o755);
}
const revision = spawnSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" });
if (revision.status !== 0) throw new Error(revision.stderr || "cannot identify test build revision");
writeFileSync(resolve(directory, "manifest.json"), JSON.stringify({ schema: 1,
  commit: revision.stdout.trim(), root: resolve(root), targets }) + "\n");
