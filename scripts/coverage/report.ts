import { createHash } from "node:crypto";
import { existsSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { relative, resolve } from "node:path";
import { output, run } from "../lib/command";

export const coverageDirectory = "target/coverage";
export const coverageReport = `${coverageDirectory}/lcov.info`;
export const coverageMeasurement = `${coverageDirectory}/measurement.json`;
const suite = "lsp-tests/test-profile/v1";

type Counts = { covered: number; total: number };
type Coverage = Map<string, { lines: Map<number, number>; summary: Counts }>;

export function readBaseline(): number {
  const text = readFileSync(".coverage-baseline", "utf8").trim();
  if (!/^\d+(\.\d+)?$/.test(text) || Number(text) > 100) {
    throw new Error(".coverage-baseline must contain a percentage between 0 and 100");
  }
  return Number(text);
}

export function readCoverage(text: string, root = process.cwd()): Coverage {
  root = existsSync(root) ? realpathSync(root) : resolve(root);
  const files: Coverage = new Map();
  let filename: string | undefined;
  let lines = new Map<number, number>();
  let found: number | undefined;
  let hit: number | undefined;
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith("SF:")) {
      if (filename !== undefined) throw new Error("unterminated LCOV record");
      const source = resolve(root, line.slice(3));
      filename = relative(root, existsSync(source) ? realpathSync(source) : source);
      lines = new Map();
      found = hit = undefined;
    } else if (line.startsWith("DA:")) {
      const match = /^DA:(\d+),(\d+)(?:,[^,]+)?$/.exec(line);
      if (!filename || !match || Number(match[1]) < 1 || !Number.isSafeInteger(Number(match[1]))
        || !Number.isSafeInteger(Number(match[2]))) {
        throw new Error(`invalid LCOV line: ${line}`);
      }
      const number = Number(match[1]);
      if (lines.has(number)) throw new Error(`duplicate LCOV line: ${filename}:${number}`);
      lines.set(number, Number(match[2]));
    } else if (line.startsWith("LF:")) found = Number(line.slice(3));
    else if (line.startsWith("LH:")) hit = Number(line.slice(3));
    else if (line === "end_of_record") {
      const coveredRecords = [...lines.values()].filter(count => count > 0).length;
      if (!filename || !Number.isSafeInteger(found) || !Number.isSafeInteger(hit)
        || found! < lines.size || hit! < coveredRecords || hit! > found!
        || found! - hit! < lines.size - coveredRecords) {
        throw new Error("invalid LCOV line totals");
      }
      if (filename.startsWith("src/") && filename.endsWith(".rs")) {
        if (files.has(filename)) throw new Error(`duplicate LCOV source: ${filename}`);
        // LLVM's aggregate can count more lines than its individual DA records.
        // Retain its official totals; use the per-line records only for the diff.
        files.set(filename, { lines, summary: { total: found!, covered: hit! } });
      }
      filename = undefined;
    }
  }
  if (filename !== undefined) throw new Error("unterminated LCOV record");
  if (totals(files).total === 0) throw new Error("coverage report contains no measured Rust source lines");
  return files;
}

export function totals(files: Coverage): Counts {
  return [...files.values()].reduce((total, file) => ({
    covered: total.covered + file.summary.covered, total: total.total + file.summary.total,
  }), { covered: 0, total: 0 });
}

function percent({ covered, total }: Counts): number {
  return total === 0 ? 0 : covered * 100 / total;
}

export function changedLines(diff: string): Set<number> {
  const lines = new Set<number>();
  for (const match of diff.matchAll(/^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@/gm)) {
    const start = Number(match[1]);
    const count = match[2] === undefined ? 1 : Number(match[2]);
    for (let number = start; number < start + count; number++) lines.add(number);
  }
  return lines;
}

function coverageBases(requested: string[]): string[] {
  let refs = requested;
  if (refs.length === 0 && process.env.PLEDGE_COVERAGE_BASE) refs = [process.env.PLEDGE_COVERAGE_BASE];
  if (refs.length === 0) {
    const upstream = run("git", ["merge-base", "HEAD", "origin/main"], { capture: true, allowFailure: true });
    refs = [upstream.status === 0 ? upstream.stdout.trim() : "HEAD"];
  }
  return [...new Set(refs.map(ref => output("git", ["rev-parse", "--verify", "--end-of-options", `${ref}^{commit}`]).trim()))];
}

export function changedSourceLines(bases: string[]): Map<string, Set<number>> {
  const changed = new Map<string, Set<number>>();
  for (const base of coverageBases(bases)) {
    const paths = output("git", ["diff", "--name-only", "--no-renames", "-z", base, "--", "src/"]).split("\0").filter(Boolean);
    for (const path of paths.filter(path => path.endsWith(".rs"))) {
      const diff = output("git", ["diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--unified=0", base, "--", path]);
      const lines = changed.get(path) ?? new Set<number>();
      for (const number of changedLines(diff)) lines.add(number);
      if (lines.size > 0) changed.set(path, lines);
    }
  }
  const untracked = output("git", ["ls-files", "--others", "--exclude-standard", "-z", "--", "src/"]).split("\0").filter(Boolean);
  for (const path of untracked.filter(path => path.endsWith(".rs"))) {
    changed.set(path, new Set(readFileSync(path, "utf8").split("\n").map((_, index) => index + 1)));
  }
  return changed;
}

export function assessCoverage(files: Coverage, changed: Map<string, Set<number>>, baseline: number) {
  const overall = totals(files);
  const patch: Counts = { covered: 0, total: 0 };
  const uncovered: string[] = [];
  const unmeasured: string[] = [];
  for (const [filename, numbers] of changed) {
    const lines = files.get(filename)?.lines;
    if (!lines) { unmeasured.push(filename); continue; }
    for (const number of numbers) {
      const hits = lines.get(number);
      if (hits === undefined) continue; // LLVM does not count comments, declarations, or excluded code.
      patch.total++;
      if (hits > 0) patch.covered++;
      else uncovered.push(`${filename}:${number}`);
    }
  }
  const failures: string[] = [];
  if (percent(overall) < baseline) failures.push(`overall coverage ${percent(overall).toFixed(2)}% is below ${baseline}%`);
  if (patch.total > 0 && percent(patch) < baseline) failures.push(`changed-line coverage ${percent(patch).toFixed(2)}% is below ${baseline}%`);
  return { overall, patch, uncovered, unmeasured, failures };
}

export function enforceCoverage(files: Coverage, baseline: number, changed: Map<string, Set<number>>) {
  const assessment = assessCoverage(files, changed, baseline);
  const messages = [
    `overall: ${percent(assessment.overall).toFixed(2)}% (${assessment.overall.covered}/${assessment.overall.total}); minimum ${baseline}%`,
    assessment.patch.total === 0 ? "changed lines: no instrumented Rust lines in this diff"
      : `changed lines: ${percent(assessment.patch).toFixed(2)}% (${assessment.patch.covered}/${assessment.patch.total}); minimum ${baseline}%`,
    ...assessment.uncovered.map(line => `uncovered: ${line}`),
    ...assessment.unmeasured.map(file => `not measured by LLVM: ${file}`),
  ];
  writeFileSync(`${coverageDirectory}/summary.txt`, messages.join("\n") + "\n");
  console.log(messages.join("\n"));
  if (assessment.failures.length) throw new Error(assessment.failures.join("; "));
}

function hash(content: string | Buffer): string {
  return createHash("sha256").update(content).digest("hex");
}

export function sourceFingerprint(): string {
  const paths = output("git", ["ls-files", "--cached", "--others", "--exclude-standard", "-z"]).split("\0")
    .filter(path => /^(src\/|tests\/|Cargo\.(toml|lock)$|\.cargo\/|rust-toolchain|scripts\/lib\/checks\.ts$)/.test(path));
  return hash([...new Set(paths)].sort().map(path => `${path}\0${existsSync(path) ? hash(readFileSync(path)) : "deleted"}`).join("\0"));
}

export function recordMeasurement(fingerprint: string) {
  if (sourceFingerprint() !== fingerprint) throw new Error("sources changed during coverage collection; run make coverage again");
  writeFileSync(coverageMeasurement, JSON.stringify({ schema: 1, suite, fingerprint,
    reportHash: hash(readFileSync(coverageReport)), platform: `${process.platform}-${process.arch}`,
    rustc: output("rustc", ["--version"]).trim(), measuredAt: new Date().toISOString(),
  }, null, 2) + "\n");
}

export function updateBaseline() {
  const measurement = JSON.parse(readFileSync(coverageMeasurement, "utf8"));
  if (measurement.schema !== 1 || measurement.suite !== suite || measurement.fingerprint !== sourceFingerprint()
    || measurement.reportHash !== hash(readFileSync(coverageReport))) {
    throw new Error("coverage evidence is stale or modified; run make coverage again");
  }
  const baseline = readBaseline();
  const next = Math.floor(percent(totals(readCoverage(readFileSync(coverageReport, "utf8")))));
  if (next < baseline) throw new Error(`refusing to lower coverage baseline from ${baseline}% to ${next}%`);
  writeFileSync(".coverage-baseline", `${next}\n`);
  console.log(`coverage baseline: ${baseline}% -> ${next}% using ${measurement.platform}, ${measurement.rustc}, ${measurement.measuredAt}`);
}
