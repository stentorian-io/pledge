import { afterEach, expect, test } from "bun:test";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { assessCoverage, changedLines, readCoverage, totals } from "./report";

function lcov(path = "/project/src/lib.rs", counts = [1, 1, 1, 1, 1, 1, 1, 1, 1, 0]) {
  return [`SF:${path}`, ...counts.map((hits, index) => `DA:${index + 1},${hits}`),
    `LF:${counts.length}`, `LH:${counts.filter(count => count > 0).length}`, "end_of_record", ""].join("\n");
}

test("changed-line coverage fails independently of a passing overall percentage", () => {
  const files = readCoverage(lcov(), "/project");
  const result = assessCoverage(files, new Map([["src/lib.rs", new Set([10])]]), 90);
  expect(result.overall).toEqual({ covered: 9, total: 10 });
  expect(result.patch).toEqual({ covered: 0, total: 1 });
  expect(result.failures).toEqual(["changed-line coverage 0.00% is below 90%"]);
  expect(result.uncovered).toEqual(["src/lib.rs:10"]);
});

test("overall coverage fails even when every changed line is covered", () => {
  const files = readCoverage(lcov("/project/src/lib.rs", [1, 0]), "/project");
  const result = assessCoverage(files, new Map([["src/lib.rs", new Set([1])]]), 90);
  expect(result.failures).toEqual(["overall coverage 50.00% is below 90%"]);
});

test("LLVM aggregate counts remain authoritative when some lines lack DA records", () => {
  const files = readCoverage(lcov().replace("LF:10", "LF:11").replace("LH:9", "LH:10"), "/project");
  expect(totals(files)).toEqual({ covered: 10, total: 11 });
  expect(assessCoverage(files, new Map(), 90).failures).toEqual([]);
});

test("unmeasured files and non-executable changed lines never become a 100 percent result", () => {
  const files = readCoverage(lcov(), "/project");
  const changed = new Map([["src/lib.rs", new Set([20])], ["src/uncompiled.rs", new Set([1])]]);
  const result = assessCoverage(files, changed, 90);
  expect(result.patch).toEqual({ covered: 0, total: 0 });
  expect(result.unmeasured).toEqual(["src/uncompiled.rs"]);
});

test.each([
  "", lcov().replace("end_of_record", ""), lcov().replace("LF:10", "LF:invalid"),
  lcov().replace("LH:9", "LH:8"),
  lcov().replace("DA:1,1", "DA:1,-1"), lcov().replace("DA:1,1", "DA:1,1\nDA:1,1"),
  lcov() + lcov(), lcov("/elsewhere/src/lib.rs"),
])("malformed or unrelated coverage cannot pass the gate: %#", text => {
  expect(() => readCoverage(text, "/project")).toThrow();
});

test("diff hunks count additions and replacements but exclude deletions and context", () => {
  expect([...changedLines("@@ -2,3 +2,0 @@\n@@ -8 +5 @@\n@@ -15,2 +12,3 @@\n")]).toEqual([5, 12, 13, 14]);
});

const project = fileURLToPath(new URL("../../", import.meta.url));
const repositories: string[] = [];
const localGitVariables = Bun.spawnSync(["git", "rev-parse", "--local-env-vars"], { cwd: project }).stdout.toString().trim().split("\n");
afterEach(() => { for (const root of repositories.splice(0)) rmSync(root, { recursive: true, force: true }); });

function repository() {
  const root = mkdtempSync(join(tmpdir(), "pledge-coverage-"));
  repositories.push(root);
  mkdirSync(join(root, "scripts"));
  for (const path of ["coverage", "lib", "check.ts", "update-baseline.sh"]) {
    cpSync(join(project, "scripts", path), join(root, "scripts", path), { recursive: true });
  }
  mkdirSync(join(root, "src"));
  mkdirSync(join(root, "bin"));
  writeFileSync(join(root, "src/lib.rs"), "pub fn answer() -> u8 { 42 }\n");
  writeFileSync(join(root, ".coverage-baseline"), "89\n");
  writeFileSync(join(root, ".gitignore"), "/target\n/bin\n/commands.log\n");
  for (const command of ["node", "npm", "typescript-language-server", "cargo-llvm-cov"]) {
    writeFileSync(join(root, "bin", command), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  }
  writeFileSync(join(root, "bin/rustc"), "#!/bin/sh\necho 'rustc fixture'\n", { mode: 0o755 });
  writeFileSync(join(root, "bin/cargo"), `#!/bin/sh
printf '%s\\n' "$*" >> commands.log
if [ "$PLEDGE_COVERAGE_FAIL" = 1 ]; then exit 7; fi
mkdir -p target/coverage
cat > target/coverage/lcov.info <<'LCOV'
${lcov(join(root, "src/lib.rs"))}LCOV
if [ "$PLEDGE_MUTATE_SOURCE" = 1 ]; then echo '// changed while running' >> src/lib.rs; fi
`, { mode: 0o755 });
  const env: NodeJS.ProcessEnv = { ...process.env, PATH: `${root}/bin:${process.env.PATH}` };
  for (const name of localGitVariables) delete env[name];
  delete env.PLEDGE_COVERAGE_BASE;
  function invoke(args: string[], extraEnv: Record<string, string> = {}) {
    return Bun.spawnSync(args, { cwd: root, env: { ...env, ...extraEnv }, stdout: "pipe", stderr: "pipe" });
  }
  function git(...args: string[]) {
    const result = invoke(["git", ...args]);
    if (result.exitCode !== 0) throw new Error(result.stderr.toString());
    return result.stdout.toString().trim();
  }
  git("init", "-q", "-b", "main");
  git("config", "user.name", "Coverage fixture");
  git("config", "user.email", "fixture@example.invalid");
  git("add", ".");
  git("-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", "commit", "-qm", "chore: initialize fixture");
  const base = git("rev-parse", "HEAD");
  const check = (extraEnv = {}) => invoke([process.execPath, "scripts/check.ts", "coverage"], extraEnv);
  const update = () => invoke([process.execPath, "scripts/coverage/update-baseline.ts"]);
  const changes = (bases: string[] = []) => invoke([process.execPath, "-e", `
    import { changedSourceLines } from './scripts/coverage/report';
    console.log(JSON.stringify([...changedSourceLines(${JSON.stringify(bases)})].map(([path,lines]) => [path,[...lines]])));
  `]);
  return { root, base, git, invoke, check, update, changes };
}

test("coverage collection runs once and baseline updates reuse its report without invoking Cargo", () => {
  const repo = repository();
  const result = repo.check();
  expect(result.exitCode, result.stderr.toString()).toBe(0);
  const commands = readFileSync(join(repo.root, "commands.log"), "utf8");
  expect(commands.trim().split("\n")).toEqual([
    "llvm-cov --locked --features lsp-tests --lcov --output-path target/coverage/lcov.info -- --test-threads=4",
  ]);
  expect(result.stdout.toString()).toContain("overall: 90.00%");
  expect(repo.update().exitCode).toBe(0);
  expect(readFileSync(join(repo.root, ".coverage-baseline"), "utf8")).toBe("90\n");
  expect(readFileSync(join(repo.root, "commands.log"), "utf8")).toBe(commands);
});

test.each(["src/lib.rs", "tests/consumer.rs", "scripts/lib/checks.ts", "target/coverage/lcov.info"])("baseline reuse rejects changed evidence: %s", path => {
  const repo = repository();
  expect(repo.check().exitCode).toBe(0);
  mkdirSync(join(repo.root, "tests"), { recursive: true });
  writeFileSync(join(repo.root, path), "changed\n");
  const result = repo.update();
  expect(result.exitCode).not.toBe(0);
  expect(result.stderr.toString()).toContain("stale or modified");
  expect(readFileSync(join(repo.root, ".coverage-baseline"), "utf8")).toBe("89\n");
});

test("failed collection invalidates the previous measurement and cannot update a baseline", () => {
  const repo = repository();
  expect(repo.check().exitCode).toBe(0);
  expect(repo.check({ PLEDGE_COVERAGE_FAIL: "1" }).exitCode).toBe(7);
  expect(existsSync(join(repo.root, "target/coverage/measurement.json"))).toBe(false);
  expect(repo.update().exitCode).not.toBe(0);
});

test("source mutations during collection cannot produce reusable coverage evidence", () => {
  const repo = repository();
  const result = repo.check({ PLEDGE_MUTATE_SOURCE: "1" });
  expect(result.exitCode).not.toBe(0);
  expect(result.stderr.toString()).toContain("sources changed during coverage collection");
  expect(existsSync(join(repo.root, "target/coverage/measurement.json"))).toBe(false);
});

test("baseline updates refuse to lower the checked-in requirement", () => {
  const repo = repository();
  expect(repo.check().exitCode).toBe(0);
  writeFileSync(join(repo.root, ".coverage-baseline"), "95\n");
  const result = repo.update();
  expect(result.exitCode).not.toBe(0);
  expect(result.stderr.toString()).toContain("refusing to lower");
  expect(readFileSync(join(repo.root, ".coverage-baseline"), "utf8")).toBe("95\n");
});

test("diff selection includes committed, staged, unstaged, renamed and untracked Rust sources", () => {
  const repo = repository();
  writeFileSync(join(repo.root, "src/lib.rs"), "pub fn answer() -> u8 { 43 }\n");
  repo.git("add", "src/lib.rs");
  repo.git("-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", "commit", "-qm", "fix: answer");
  repo.git("mv", "src/lib.rs", "src/renamed file.rs");
  writeFileSync(join(repo.root, "src/renamed file.rs"), "pub fn answer() -> u8 { 44 }\n");
  writeFileSync(join(repo.root, "src/new.rs"), "pub fn new() {}\n");
  const result = repo.changes([repo.base, "HEAD"]);
  expect(result.exitCode, result.stderr.toString()).toBe(0);
  expect(JSON.parse(result.stdout.toString())).toEqual([
    ["src/renamed file.rs", [1]], ["src/new.rs", [1, 2]],
  ]);
});

test("unknown explicit diff bases fail rather than silently checking an empty patch", () => {
  const repo = repository();
  const result = repo.changes(["missing-branch"]);
  expect(result.exitCode).not.toBe(0);
  const check = repo.check({ PLEDGE_COVERAGE_BASE: "missing-branch" });
  expect(check.exitCode).not.toBe(0);
  expect(existsSync(join(repo.root, "commands.log"))).toBe(false);
});
