import { mkdirSync, readFileSync, rmSync } from "node:fs";
import {
  changedSourceLines, coverageDirectory, coverageMeasurement, coverageReport, enforceCoverage,
  readBaseline, readCoverage, recordMeasurement, sourceFingerprint,
} from "../coverage/report";
import { fail, prepareRustEnvironment, requireCommand, run } from "./command";

export function checkRust() {
  checkRustStyle();
  run("cargo", ["test", "--locked", "--release"]);
}

export function checkRustStyle() {
  prepareRustEnvironment();
  run("cargo", ["fmt", "--all", "--", "--check"]);
  run("cargo", ["clippy", "--locked", "--all-targets", "--", "-D", "warnings"]);
}

export function checkLsp() {
  prepareRustEnvironment();
  requireCompilerTools();
  run("cargo", ["test", "--locked", "--release", "--features", "lsp-tests", "--", "--test-threads=4"]);
}

function requireCompilerTools() {
  requireCommand("node", "Install Node.js.");
  requireCommand("npm", "Install Node.js with npm.");
  requireCommand("typescript-language-server", "Run npm install -g typescript typescript-language-server.");
}

export function checkCoverage(bases: string[] = []) {
  prepareRustEnvironment();
  requireCommand("cargo-llvm-cov", "Run rustup component add llvm-tools-preview, then cargo install cargo-llvm-cov --locked.");
  try {
    const baseline = readBaseline();
    const changed = changedSourceLines(bases);
    requireCompilerTools();
    mkdirSync(coverageDirectory, { recursive: true });
    for (const path of [coverageMeasurement, coverageReport, `${coverageDirectory}/summary.txt`]) rmSync(path, { force: true });
    const fingerprint = sourceFingerprint();
    run("cargo", ["llvm-cov", "--locked", "--features", "lsp-tests", "--lcov", "--output-path", coverageReport, "--", "--test-threads=4"]);
    const files = readCoverage(readFileSync(coverageReport, "utf8"));
    recordMeasurement(fingerprint);
    enforceCoverage(files, baseline, changed);
  } catch (error) {
    fail("coverage", String(error));
  }
}

export function checkTools() {
  run("bun", ["test", "./scripts/hooks/hooks.test.ts", "./scripts/releases", "./scripts/action", "./scripts/conformance/conformance.test.ts", "./scripts/coverage"]);
}

export function checkLanguageCoverage() {
  run("bun", ["scripts/conformance/check.ts"]);
}
