import { readFileSync } from "node:fs";
import { fail, prepareRustEnvironment, requireCommand, run } from "./command";

export function checkRust() {
  prepareRustEnvironment();
  run("cargo", ["fmt", "--all", "--", "--check"]);
  run("cargo", ["clippy", "--locked", "--all-targets", "--", "-D", "warnings"]);
  run("cargo", ["test", "--locked", "--release"]);
}

export function checkLsp() {
  prepareRustEnvironment();
  requireCommand("node", "Install Node.js.");
  requireCommand("npm", "Install Node.js with npm.");
  requireCommand("typescript-language-server", "Run npm install -g typescript typescript-language-server.");
  run("cargo", ["test", "--locked", "--release", "--features", "lsp-tests", "--", "--test-threads=1"]);
}

export function checkCoverage() {
  prepareRustEnvironment();
  requireCommand("cargo-llvm-cov", "Run rustup component add llvm-tools-preview, then cargo install cargo-llvm-cov --locked.");
  let baseline: string;
  try {
    baseline = readFileSync(".coverage-baseline", "utf8").trim();
  } catch {
    fail("coverage", ".coverage-baseline is missing; restore the checked-in baseline.");
  }
  if (!/^\d+(\.\d+)?$/.test(baseline) || Number(baseline) > 100) {
    fail("coverage", ".coverage-baseline must contain a percentage between 0 and 100.");
  }
  run("cargo", ["llvm-cov", "--locked", "--fail-under-lines", baseline]);
}

export function checkTools() {
  run("bun", ["test", "./scripts/hooks/hooks.test.ts", "./scripts/releases"]);
}
