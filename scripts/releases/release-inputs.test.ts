import { afterEach, expect, test } from "bun:test";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { changedPublishedInputs, isPublishedInput, releaseInputs } from "./release-inputs.cjs";

test.each([
  "src/app/mod.rs", "src/languages/typescript/compiler/compiler.cjs", "Cargo.toml", "Cargo.lock",
  "Dockerfile", ".dockerignore", "build.rs", ".cargo/config.toml", "rust-toolchain.toml",
  "scripts/action/action.cjs", "scripts/action/package-lock.json", "action.yml", "LICENSE-MIT",
  "scripts/releases/publish-release.cjs", ".github/workflows/publish.yml",
])("published code and artifact build inputs remain release eligible: %s", path => {
  expect(isPublishedInput(path)).toBe(true);
});

test.each([
  "scripts/coverage/report.ts", "scripts/hooks/pre-push.ts", "scripts/lib/checks.ts", "scripts/check.ts",
  "scripts/releases/merge-release.cjs", "scripts/releases/smoke-image.cjs", "scripts/action/action.test.ts",
  ".github/workflows/checks.yml", ".github/workflows/release.yml", ".coverage-baseline", "Makefile",
  "README.md", "docs/releases.md", "tests/fixtures/package.json", "release-please-config.json",
  "src/languages/typescript/compiler/worker/tests.rs", "src/languages/javascript/model/tests/identity.rs",
])("repository maintenance cannot alone request a release: %s", path => {
  expect(isPublishedInput(path)).toBe(false);
});

const temporaryRepos: string[] = [];
afterEach(() => { for (const root of temporaryRepos.splice(0)) rmSync(root, { recursive: true, force: true }); });

function repository() {
  const root = mkdtempSync(join(tmpdir(), "pledge-release-inputs-"));
  temporaryRepos.push(root);
  const env = { ...process.env };
  for (const name of execFileSync("git", ["rev-parse", "--local-env-vars"], { encoding: "utf8" }).trim().split("\n")) delete env[name];
  const git = (...args: string[]) => execFileSync("git", args, { cwd: root, env, encoding: "utf8" }).trim();
  const write = (path: string, content: string) => {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), content);
  };
  const commit = (message: string) => { git("add", "."); git("commit", "-qm", message); return git("rev-parse", "HEAD"); };
  git("init", "-q", "-b", "main");
  git("config", "user.name", "Release fixture");
  git("config", "user.email", "fixture@example.invalid");
  git("config", "commit.gpgsign", "false");
  git("config", "core.hooksPath", "/dev/null");
  write("src/lib.rs", "pub fn answer() -> u8 { 42 }\n");
  const baseline = commit("feat: initial interface");
  git("tag", "v0.3.0");
  write("release-please-config.json", JSON.stringify({ "bootstrap-sha": baseline }));
  commit("chore: configure releases");
  return { root, git, write, commit, baseline };
}

test("net published changes include deletions and renames but exclude tooling and reverted changes", () => {
  const repo = repository();
  repo.write("scripts/coverage/report.ts", "// improved checks\n");
  repo.commit("fix: improve coverage tooling");
  expect(changedPublishedInputs(repo.baseline, "HEAD", repo.root)).toEqual([]);
  repo.write("src/lib.rs", "pub fn answer() -> u8 { 43 }\n");
  repo.commit("fix: correct the answer");
  expect(changedPublishedInputs(repo.baseline, "HEAD", repo.root)).toEqual(["src/lib.rs"]);
  repo.write("src/lib.rs", "pub fn answer() -> u8 { 42 }\n");
  repo.commit("revert: restore the released behavior");
  expect(changedPublishedInputs(repo.baseline, "HEAD", repo.root)).toEqual([]);
  repo.git("mv", "src/lib.rs", "src/renamed.rs");
  repo.commit("refactor: move implementation");
  expect(changedPublishedInputs(repo.baseline, "HEAD", repo.root)).toEqual(["src/lib.rs", "src/renamed.rs"]);
  repo.git("rm", "src/renamed.rs");
  repo.commit("fix: remove an obsolete entry");
  expect(changedPublishedInputs(repo.baseline, "HEAD", repo.root)).toEqual(["src/lib.rs"]);
});

test.each(["published", "bootstrap", "api-failure", "bad-tag", "missing-tag"])("release requests use verified history: %s", async scenario => {
  const repo = repository();
  const directory = process.cwd();
  const names = execFileSync("git", ["rev-parse", "--local-env-vars"], { encoding: "utf8" }).trim().split("\n");
  const saved = Object.fromEntries(names.map(name => [name, process.env[name]]));
  for (const name of names) delete process.env[name];
  const logs: string[] = [];
  const github = { rest: { repos: { getLatestRelease: async () => {
    if (scenario === "bootstrap") throw Object.assign(new Error("not found"), { status: 404 });
    if (scenario === "api-failure") throw Object.assign(new Error("unavailable"), { status: 503 });
    return { data: { tag_name: scenario === "bad-tag" ? "unrelated" : scenario === "missing-tag" ? "v9.9.9" : "v0.3.0" } };
  } } } };
  try {
    process.chdir(repo.root);
    const options = { github, context: { repo: { owner: "fixture", repo: "pledge" } }, core: { info: (text: string) => logs.push(text) } };
    if (["api-failure", "bad-tag", "missing-tag"].includes(scenario)) {
      await expect(releaseInputs(options)).rejects.toThrow();
    } else {
      expect(await releaseInputs(options)).toBe(false);
      repo.write("scripts/action/action.cjs", "module.exports = 'changed runtime';\n");
      repo.commit("fix: correct packaged action behavior");
      expect(await releaseInputs(options)).toBe(true);
    }
  } finally {
    process.chdir(directory);
    for (const name of names) {
      if (saved[name] === undefined) delete process.env[name];
      else process.env[name] = saved[name];
    }
  }
});

test("release finalization remains enabled when only new PR creation is suppressed", () => {
  const workflow = readFileSync(".github/workflows/release.yml", "utf8");
  expect(workflow).toContain("skip-github-pull-request: ${{ steps.revision.outputs.publishable != 'true' }}");
  expect(workflow).not.toContain("skip-github-release:");
});
