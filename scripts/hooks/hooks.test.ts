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

function repository() {
  const root = mkdtempSync(join(tmpdir(), "pledge-hooks-"));
  temporaryRepos.push(root);
  cpSync(join(project, "scripts"), join(root, "scripts"), { recursive: true });
  mkdirSync(join(root, "src"));
  mkdirSync(join(root, "tests/fixtures"), { recursive: true });
  mkdirSync(join(root, "bin"));
  writeFileSync(join(root, "README.md"), "# Example\n");
  writeFileSync(join(root, "src/lib.rs"), "pub fn answer() -> u8 { 42 }\n");
  writeFileSync(join(root, "tests/fixtures/example.js"), "export const answer = 42;\n");
  writeFileSync(join(root, ".coverage-baseline"), "69\n");
  const log = join(root, "commands.log");
  writeFileSync(log, "");
  for (const command of ["cargo", "cargo-llvm-cov", "node", "npm", "typescript-language-server"]) {
    writeFileSync(join(root, "bin", command), `#!/bin/sh
printf '%s\\n' '${command} '"$*" >> "$PLEDGE_HOOK_LOG"
if [ "$1" = "$PLEDGE_FAIL_COMMAND" ]; then exit 7; fi
`, { mode: 0o755 });
  }
  const env: NodeJS.ProcessEnv = { ...process.env, PATH: `${root}/bin:${process.env.PATH}`, PLEDGE_HOOK_LOG: log };
  // Hooks can inherit an absolute index or worktree path from Git. Child Git
  // commands must never use the caller's repository while building fixtures.
  for (const name of localGitVariables) delete env[name];
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

test("installed hooks correct a documentation commit without running Rust checks", () => {
  const repo = repository();
  expect(repo.hook("install").exitCode).toBe(0);
  expect(repo.git("config", "--local", "core.hooksPath")).toBe("scripts/hooks");

  repo.stage("README.md", "# Updated usage\n");
  const commit = repo.invoke(["git", "commit", "-m", "feat: explain installation"]);

  expect(commit.exitCode).toBe(0);
  expect(repo.git("log", "-1", "--format=%s")).toBe("docs: explain installation");
  expect(repo.commands()).toBe("");
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

test("hook wrappers count as implementation source", () => {
  const repo = repository();
  repo.stage("scripts/hooks/pre-commit", "#!/bin/sh\nexit 1\n");
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
    "cargo test --locked --release --features lsp-tests -- --test-threads=1",
    "cargo llvm-cov --locked --fail-under-lines 69",
  ]);
});

test("branch deletion pushes do not run checks", () => {
  const repo = repository();
  expect(repo.hook("pre-push", [], `(delete) ${"0".repeat(40)} refs/heads/old ${repo.base}\n`).exitCode).toBe(0);
  expect(repo.commands()).toBe("");
});

test.each(["0".repeat(40), "1".repeat(40)])("new or unavailable remote tips run the complete push checks: %s", (remote) => {
  const repo = repository();
  writeFileSync(join(repo.root, "bin/bun"), '#!/bin/sh\nprintf "%s\\n" "bun $*" >> "$PLEDGE_HOOK_LOG"\n', { mode: 0o755 });

  expect(repo.hook("pre-push", [], `refs/heads/main ${repo.base} refs/heads/main ${remote}\n`).exitCode).toBe(0);
  expect(repo.commands()).toContain("bun test ./scripts/hooks/hooks.test.ts");
  expect(repo.commands()).toContain("cargo test --locked --release --features lsp-tests");
  expect(repo.commands()).toContain("cargo llvm-cov --locked --fail-under-lines 69");
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
