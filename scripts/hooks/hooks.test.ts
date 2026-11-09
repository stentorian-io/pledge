import { afterEach, expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const project = fileURLToPath(new URL("../../", import.meta.url));
const temporaryRepos: string[] = [];
const gitEnvironment = Bun.spawnSync(["git", "rev-parse", "--local-env-vars"], {
  cwd: project, stdout: "pipe", stderr: "pipe",
});
if (gitEnvironment.exitCode !== 0) throw new Error(gitEnvironment.stderr.toString());
const localGitVariables = gitEnvironment.stdout.toString().trim().split("\n");

afterEach(() => {
  for (const root of temporaryRepos.splice(0)) rmSync(root, { recursive: true, force: true });
});

function repository(inheritedEnvironment: NodeJS.ProcessEnv = process.env) {
  const root = mkdtempSync(join(tmpdir(), "pledge-hooks-"));
  temporaryRepos.push(root);
  cpSync(join(project, "scripts"), join(root, "scripts"), {
    recursive: true, filter: source => !source.split("/").includes("node_modules"),
  });
  mkdirSync(join(root, "src"));
  mkdirSync(join(root, "src/languages/typescript/compiler"), { recursive: true });
  mkdirSync(join(root, "tests/fixtures"), { recursive: true });
  mkdirSync(join(root, "bin"));
  writeFileSync(join(root, "README.md"), "# Example\n");
  writeFileSync(join(root, "src/lib.rs"), "pub fn answer() -> u8 { 42 }\n");
  writeFileSync(join(root, "src/languages/typescript/compiler/compiler.cjs"), "console.log('{}');\n");
  writeFileSync(join(root, "tests/fixtures/example.js"), "export const answer = 42;\n");
  writeFileSync(join(root, ".coverage-baseline"), "90\n");
  const log = join(root, "commands.log");
  writeFileSync(log, "");
  for (const command of ["cargo", "cargo-llvm-cov", "node", "npm", "typescript-language-server"]) {
    writeFileSync(join(root, "bin", command), `#!/bin/sh
printf '%s\\n' '${command} '"$*" >> "$PLEDGE_HOOK_LOG"
if [ "$1" = "$PLEDGE_FAIL_COMMAND" ]; then exit 7; fi
if [ "$1" = "llvm-cov" ]; then
  mkdir -p target/coverage
  printf 'SF:%s/src/lib.rs\\nDA:1,1\\nLF:1\\nLH:1\\nend_of_record\\n' "$PWD" > target/coverage/lcov.info
fi
`, { mode: 0o755 });
  }
  const env: NodeJS.ProcessEnv = { ...inheritedEnvironment, PATH: `${root}/bin:${process.env.PATH}`, PLEDGE_HOOK_LOG: log };
  // Hooks can inherit an absolute index or worktree path from Git. Child Git
  // commands must never use the caller's repository while building fixtures.
  for (const name of localGitVariables) delete env[name];
  delete env.PLEDGE_COVERAGE_BASE;
  function invoke(command: string[], input?: string, extraEnv: Record<string, string> = {}) {
    return Bun.spawnSync(command, {
      cwd: root, env: { ...env, ...extraEnv },
      stdin: input === undefined ? "ignore" : Buffer.from(input),
      stdout: "pipe", stderr: "pipe",
    });
  }
  function git(...args: string[]) {
    const result = invoke(["git", ...args]);
    if (result.exitCode !== 0) throw new Error(result.stderr.toString());
    return result.stdout.toString().trim();
  }
  function stage(path: string, content: string) {
    const destination = join(root, path);
    mkdirSync(dirname(destination), { recursive: true });
    writeFileSync(destination, content);
    git("add", "--", path);
  }
  function hook(name: string, args: string[] = [], input?: string, extraEnv: Record<string, string> = {}) {
    return invoke([process.execPath, `scripts/hooks/${name}.ts`, ...args], input, extraEnv);
  }
  function commit(message: string) {
    // Fixture construction must not run the development checks under test.
    git("-c", "core.hooksPath=/dev/null", "commit", "-qm", message);
    return git("rev-parse", "HEAD");
  }
  function message(text: string) {
    writeFileSync(join(root, "message.txt"), text);
    return hook("commit-msg", ["message.txt"]);
  }
  git("init", "-q", "-b", "main");
  git("config", "user.name", "Hook fixture");
  git("config", "user.email", "fixture@example.invalid");
  git("config", "commit.gpgsign", "false");
  git("add", ".");
  const base = commit("chore: initialize fixture");
  return { root, base, invoke, git, stage, hook, commit, message, commands: () => readFileSync(log, "utf8") };
}

test("installed hooks reject a mistyped documentation commit without rewriting intent", () => {
  const repo = repository();
  expect(repo.hook("install").exitCode).toBe(0);
  expect(repo.git("config", "--local", "core.hooksPath")).toBe("scripts/hooks");

  repo.stage("README.md", "# Updated usage\n");
  const commit = repo.invoke(["git", "commit", "-m", "feat: explain installation"]);

  expect(commit.exitCode).not.toBe(0);
  expect(repo.invoke(["git", "commit", "-m", "docs: explain installation"]).exitCode).toBe(0);
  expect(repo.git("log", "-1", "--format=%s")).toBe("docs: explain installation");
  expect(repo.commands()).toBe("");
});

test("make setup installs the locked action dependencies and activates hooks in its own checkout", () => {
  const repo = repository();
  cpSync(join(project, "Makefile"), join(repo.root, "Makefile"));
  expect(repo.invoke(["make", "setup"]).exitCode).toBe(0);
  expect(repo.commands()).toContain("npm ci --prefix scripts/action --ignore-scripts --no-audit --no-fund");
  expect(repo.git("config", "--local", "core.hooksPath")).toBe("scripts/hooks");
});

test.each([
  "feat(parser): add export detection",
  "fix: Uppercase description",
  "chore: trailing period.",
  "Merge branch feature",
  "fix: missing implementation change",
])("rejects an invalid commit message: %s", (subject) => {
  const repo = repository();
  repo.stage("README.md", "# Updated\n");
  expect(repo.message(subject).exitCode).not.toBe(0);
});

test.each(["Co-Authored-By", "co-authored-by", "Signed-off-by", "Reviewed-by", "Tested-by", "Generated-by"])("rejects %s attribution", (trailer) => {
  const repo = repository();
  repo.stage("README.md", "# Updated\n");
  expect(repo.message(`docs: explain setup\n\n${trailer}: Example <example@example.invalid>\n`).exitCode).not.toBe(0);
});

test("allows a meaningful breaking change footer", () => {
  const repo = repository();
  repo.stage("src/lib.rs", "pub fn answer() -> u16 { 42 }\n");
  expect(repo.message("feat!: widen the return type\n\nBREAKING CHANGE: answer returns u16.\n").exitCode).toBe(0);
});

test("rejects generation credits in the commit body", () => {
  const repo = repository();
  repo.stage("README.md", "# Updated\n");
  expect(repo.message("docs: explain setup\n\n🤖 Generated with [Claude Code](https://example.invalid)\n").exitCode).not.toBe(0);
});

test.each(["scripts/hooks/pre-commit", "scripts/releases/policy.cjs"])("tooling implementation permits fix commits: %s", (path) => {
  const repo = repository();
  repo.stage(path, path.endsWith(".cjs") ? "module.exports = {};\n" : "#!/bin/sh\nexit 1\n");
  expect(repo.message("fix: correct hook startup").exitCode).toBe(0);
});

test("Cargo examples count as implementation source", () => {
  const repo = repository();
  repo.stage("examples/dump.rs", "fn main() {}\n");
  expect(repo.message("fix: correct fixture generation").exitCode).toBe(0);
});

test("a deleted Rust file still triggers checks and counts as implementation", () => {
  const repo = repository();
  repo.git("rm", "src/lib.rs");

  expect(repo.message("fix: remove obsolete implementation").exitCode).toBe(0);
  expect(repo.hook("pre-commit").exitCode).toBe(0);
  expect(repo.commands().trim().split("\n")).toEqual([
    "cargo fmt --all -- --check",
    "cargo clippy --locked --all-targets -- -D warnings",
    "cargo test --locked --release",
  ]);
});

test("package fixture changes trigger checks without changing Rust files", () => {
  const repo = repository();
  repo.stage("tests/fixtures/example.js", "export const answer = 43;\n");
  expect(repo.hook("pre-commit").exitCode).toBe(0);
  expect(repo.commands()).toContain("cargo test --locked --release");
});

test("public language coverage edits validate the inventory before tooling checks", () => {
  const repo = repository();
  writeFileSync(join(repo.root, "bin/bun"), '#!/bin/sh\nprintf "%s\\n" "bun $*" >> "$PLEDGE_HOOK_LOG"\n', { mode: 0o755 });
  repo.stage("docs/coverage/inventory.json", "{}\n");

  expect(repo.hook("pre-commit").exitCode).toBe(0);
  const commands = repo.commands().trim().split("\n");
  expect(commands[0]).toBe("bun scripts/conformance/check.ts");
  expect(commands[1]).toContain("./scripts/conformance/conformance.test.ts");
  expect(commands.some(command => command.startsWith("cargo "))).toBe(false);
});

test("complete checks execute the full suite once while collecting coverage", () => {
  const repo = repository({ ...process.env, PLEDGE_COVERAGE_BASE: "origin/a-different-repository" });
  writeFileSync(join(repo.root, "bin/bun"), '#!/bin/sh\nprintf "%s\\n" "bun $*" >> "$PLEDGE_HOOK_LOG"\n', { mode: 0o755 });

  const result = repo.invoke([process.execPath, "scripts/check.ts", "all"]);

  expect(result.exitCode, result.stderr.toString()).toBe(0);
  expect(repo.commands().trim().split("\n")).toEqual([
    "bun scripts/conformance/check.ts",
    "bun test ./scripts/hooks/hooks.test.ts ./scripts/releases ./scripts/action ./scripts/conformance/conformance.test.ts ./scripts/coverage",
    "cargo fmt --all -- --check",
    "cargo clippy --locked --all-targets -- -D warnings",
    "cargo llvm-cov --locked --features lsp-tests --lcov --output-path target/coverage/lcov.info -- --test-threads=4",
  ]);
});

test.each(["Dockerfile", ".dockerignore", "action.yml"])("image build inputs trigger tooling checks: %s", path => {
  const repo = repository();
  writeFileSync(join(repo.root, "bin/bun"), '#!/bin/sh\nprintf "%s\\n" "bun $*" >> "$PLEDGE_HOOK_LOG"\n', { mode: 0o755 });
  repo.stage(path, "# build input\n");
  expect(repo.hook("pre-commit").exitCode).toBe(0);
  expect(repo.commands()).toContain("./scripts/releases");
  expect(repo.commands()).not.toContain("cargo ");
});

test.each(["modified", "deleted"])("%s embedded compiler source runs product checks and allows fix commits", (change) => {
  const repo = repository();
  expect(repo.hook("install").exitCode).toBe(0);
  const compiler = "src/languages/typescript/compiler/compiler.cjs";
  if (change === "deleted") repo.git("rm", compiler);
  else repo.stage(compiler, "console.log('{\"results\":[]}');\n");

  const commit = repo.invoke(["git", "commit", "-m", "fix: correct compiler evaluation"]);

  expect(commit.exitCode).toBe(0);
  expect(repo.git("log", "-1", "--format=%s")).toBe("fix: correct compiler evaluation");
  const tip = repo.git("rev-parse", "HEAD");
  expect(repo.hook("pre-push", [], `refs/heads/main ${tip} refs/heads/main ${repo.base}\n`).exitCode).toBe(0);
  expect(repo.commands().trim().split("\n")).toEqual([
    "cargo fmt --all -- --check",
    "cargo clippy --locked --all-targets -- -D warnings",
    "cargo test --locked --release",
    "cargo llvm-cov --locked --features lsp-tests --lcov --output-path target/coverage/lcov.info -- --test-threads=4",
  ]);
});

test("checks stop immediately when formatting fails", () => {
  const repo = repository();
  repo.stage("src/lib.rs", "pub fn answer() -> u8 { 43 }\n");
  expect(repo.hook("pre-commit", [], undefined, { PLEDGE_FAIL_COMMAND: "fmt" }).exitCode).toBe(7);
  expect(repo.commands()).toBe("cargo fmt --all -- --check\n");
});

test("documentation pushes inspect committed refs rather than dirty Rust files", () => {
  const repo = repository();
  repo.stage("README.md", "# Updated\n");
  const tip = repo.commit("docs: update usage");
  writeFileSync(join(repo.root, "src/lib.rs"), "uncommitted Rust changes\n");

  expect(repo.hook("pre-push", [], `refs/heads/main ${tip} refs/heads/main ${repo.base}\n`).exitCode).toBe(0);
  expect(repo.commands()).toBe("");
});

test("push checks include earlier source changes when the last commit only updates docs", () => {
  const repo = repository();
  repo.stage("src/lib.rs", "pub fn answer() -> u8 { 43 }\n");
  repo.commit("fix: correct the answer");
  repo.stage("README.md", "# Updated\n");
  const tip = repo.commit("docs: update usage");

  expect(repo.hook("pre-push", [], `refs/heads/main ${tip} refs/heads/main ${repo.base}\n`).exitCode).toBe(0);
  expect(repo.commands().trim().split("\n")).toEqual([
    "cargo llvm-cov --locked --features lsp-tests --lcov --output-path target/coverage/lcov.info -- --test-threads=4",
  ]);
});

test("branch deletion pushes do not run checks", () => {
  const repo = repository();
  expect(repo.hook("pre-push", [], `(delete) ${"0".repeat(40)} refs/heads/old ${repo.base}\n`).exitCode).toBe(0);
  expect(repo.commands()).toBe("");
});

test("a coverage threshold change reruns the instrumented suite before pushing", () => {
  const repo = repository();
  writeFileSync(join(repo.root, "bin/bun"), '#!/bin/sh\nprintf "%s\\n" "bun $*" >> "$PLEDGE_HOOK_LOG"\n', { mode: 0o755 });
  repo.stage(".coverage-baseline", "95\n");
  const tip = repo.commit("build: raise coverage floor");

  expect(repo.hook("pre-push", [], `refs/heads/main ${tip} refs/heads/main ${repo.base}\n`).exitCode).toBe(0);
  expect(repo.commands()).toContain("cargo llvm-cov --locked --features lsp-tests");
  expect(repo.commands()).not.toContain("cargo test ");
});

test.each(["0".repeat(40), "1".repeat(40)])("new or unavailable remote tips run the complete push checks: %s", (remote) => {
  const repo = repository();
  writeFileSync(join(repo.root, "bin/bun"), '#!/bin/sh\nprintf "%s\\n" "bun $*" >> "$PLEDGE_HOOK_LOG"\n', { mode: 0o755 });

  expect(repo.hook("pre-push", [], `refs/heads/main ${repo.base} refs/heads/main ${remote}\n`).exitCode).toBe(0);
  expect(repo.commands()).toContain("bun test ./scripts/hooks/hooks.test.ts");
  expect(repo.commands()).not.toContain("cargo test ");
  expect(repo.commands()).toContain("cargo llvm-cov --locked --features lsp-tests --lcov --output-path target/coverage/lcov.info -- --test-threads=4");
});

test("invalid coverage baselines fail before running coverage", () => {
  const repo = repository();
  writeFileSync(join(repo.root, ".coverage-baseline"), "-1\n");
  const check = repo.invoke([process.execPath, "scripts/check.ts", "coverage"]);
  expect(check.exitCode).not.toBe(0);
  expect(check.stderr.toString()).toContain("percentage between 0 and 100");
  expect(repo.commands()).toBe("");
});

test.each(["unknown", "../../outside"])("reference regeneration rejects an invalid package key: %s", (key) => {
  const repo = repository();
  const regeneration = repo.invoke(["bash", "scripts/regen-reference-suite.sh", key]);
  expect(regeneration.exitCode).not.toBe(0);
  expect(regeneration.stderr.toString()).toContain("Unknown reference package");
  expect(repo.commands()).toBe("");
});
