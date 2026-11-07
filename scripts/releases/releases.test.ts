import { afterEach, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { isBreaking, validateMessage } from "../lib/commit-message.cjs";
import { checkVersion } from "./check-version";
import { mergeBlocker } from "./merge-policy.cjs";
import mergeRelease from "./merge-release.cjs";

const temporaryRepos: string[] = [];
afterEach(() => {
  for (const root of temporaryRepos.splice(0)) rmSync(root, { recursive: true, force: true });
});

function candidate() {
  return {
    now: Date.parse("2026-11-03T12:00:00Z"),
    previousVersion: "0.3.0", nextVersion: "0.3.1",
    messages: ["fix: avoid duplicate findings"],
    headDate: "2026-11-02T11:00:00Z",
    checks: ["Release checks", "Release image"].map(name => ({ name, app: { slug: "github-actions" }, status: "completed", conclusion: "success" })),
    pull: {
      user: { login: "github-actions[bot]", type: "Bot" },
      state: "open", draft: false,
      base: { ref: "main", repo: { full_name: "stentorian-io/pledge" } },
      head: { ref: "release-please--branches--main", repo: { full_name: "stentorian-io/pledge" } },
      labels: [{ name: "autorelease: pending" }],
      body: "Bug Fixes", updated_at: "2026-11-02T11:00:00Z",
      mergeable: true, mergeable_state: "clean",
    },
  };
}

test("routine releases can merge after the quiet period", () => {
  expect(mergeBlocker(candidate())).toBeNull();
  expect(mergeBlocker({ ...candidate(), nextVersion: "0.4.0" })).toBeNull();
});

test.each([
  "feat!: remove a command\n\nBREAKING CHANGE: use compare instead",
  "fix: change exit status\n\nBREAKING CHANGE: handle status 3",
  "refactor(core)!: change a public contract",
  "build: change support\n\nBREAKING-CHANGE: upgrade your runtime",
])("breaking commits can release automatically before 1.0: %s", (message) => {
  expect(isBreaking(message)).toBe(true);
  expect(mergeBlocker({ ...candidate(), messages: [message], nextVersion: "0.4.0" })).toBeNull();
  expect(mergeBlocker({ ...candidate(), messages: [message] })).toContain("minor version");
  expect(mergeBlocker({ ...candidate(), messages: [message], previousVersion: "1.3.0", nextVersion: "1.4.0" })).toContain("manual merge");
});

test("major transitions require manual merge without a breaking marker", () => {
  expect(mergeBlocker({ ...candidate(), nextVersion: "1.0.0" })).toContain("manual merge");
});

test("hold labels and recent activity block automatic merging", () => {
  const release = candidate();
  release.pull.labels.push({ name: "release: hold" });
  expect(mergeBlocker(release)).toContain("held");
  release.pull.labels.pop();
  release.pull.updated_at = "2026-11-03T11:59:00Z";
  expect(mergeBlocker(release)).toContain("quiet hours");
  expect(mergeBlocker({ ...candidate(), headDate: "2026-11-03T11:00:00Z" })).toContain("quiet hours");
  expect(mergeBlocker({ ...candidate(), headDate: "invalid" })).toContain("quiet hours");
});

test("the quiet period expires at exactly 24 hours", () => {
  const release = candidate();
  release.now = Date.parse(release.headDate) + 24 * 60 * 60 * 1000;
  expect(mergeBlocker(release)).toBeNull();
  expect(mergeBlocker({ ...release, now: release.now - 1 })).toContain("quiet hours");
});

test("unknown authors, forks, drafts, and unready branches cannot auto-merge", () => {
  for (const modify of [
    (release: ReturnType<typeof candidate>) => { release.pull.user.login = "someone-else"; },
    (release: ReturnType<typeof candidate>) => { release.pull.head.repo.full_name = "someone/pledge"; },
    (release: ReturnType<typeof candidate>) => { release.pull.draft = true; },
    (release: ReturnType<typeof candidate>) => { release.pull.mergeable_state = "blocked"; },
    (release: ReturnType<typeof candidate>) => { release.pull.labels = []; },
    (release: ReturnType<typeof candidate>) => { release.pull.body = "### BREAKING CHANGES\nMigrate settings"; },
  ]) {
    const release = candidate();
    modify(release);
    expect(mergeBlocker(release)).not.toBeNull();
  }
});

test("missing, failed, spoofed, or pending checks block merging", () => {
  for (const checks of [
    [],
    [candidate().checks[0]],
    [{ ...candidate().checks[0], conclusion: "failure" }],
    [{ ...candidate().checks[0], app: { slug: "someone-else" } }],
    [...candidate().checks, { name: "Security", status: "in_progress", conclusion: null }],
  ]) expect(mergeBlocker({ ...candidate(), checks })).not.toBeNull();
});

test.each(["0.3.0", "0.2.9", "0.4.0-beta.1", "garbage"])("rejects invalid next version %s", (nextVersion) => {
  expect(mergeBlocker({ ...candidate(), nextVersion })).not.toBeNull();
});

test("breaking subjects need migration context and internal commits remain valid", () => {
  expect(validateMessage("feat!: remove old command")).toContain("footer");
  expect(validateMessage("feat!: remove old command\n\nBREAKING CHANGE: use pledge compare instead")).toBeNull();
  expect(validateMessage("chore: update development tooling")).toBeNull();
  expect(validateMessage("fix: correct behavior\n\nBREAKING CHANGE:")).not.toBeNull();
});

test("version checks reject manifest, lockfile, and tag drift", () => {
  const root = mkdtempSync(join(tmpdir(), "pledge-release-"));
  temporaryRepos.push(root);
  writeFileSync(join(root, "Cargo.toml"), '[package]\nname = "pledge"\nversion = "0.3.0"\n');
  writeFileSync(join(root, "Cargo.lock"), '[[package]]\nname = "pledge"\nversion = "0.3.0"\n');
  writeFileSync(join(root, ".release-please-manifest.json"), '{".":"0.3.0"}');
  writeFileSync(join(root, "action.yml"), 'runs:\n  image: docker://ghcr.io/stentorian-io/pledge:v0.3.0 # x-release-please-version\n');
  expect(checkVersion(root, "v0.3.0")).toBe("0.3.0");
  expect(() => checkVersion(root, "v0.3.1")).toThrow("expected tag");
  writeFileSync(join(root, ".release-please-manifest.json"), '{".":"0.3.1"}');
  expect(() => checkVersion(root)).toThrow("same version");
  writeFileSync(join(root, ".release-please-manifest.json"), '{".":"0.3.0"}');
  writeFileSync(join(root, "Cargo.lock"), '[[package]]\nname = "pledge"\nversion = "0.2.0"\n');
  expect(() => checkVersion(root)).toThrow("same version");
  writeFileSync(join(root, "Cargo.lock"), '[[package]]\nname = "pledge"\nversion = "0.3.0"\n');
  writeFileSync(join(root, "action.yml"), 'runs:\n  image: docker://ghcr.io/stentorian-io/pledge:v0.2.0\n');
  expect(() => checkVersion(root)).toThrow("action image");
});

test.each(["eligible", "dry-run", "new-head", "held", "unexpected-file", "breaking-history", "dispatch-failed"])(
  "the scheduler checks the actual release history and exact head: %s", async (scenario) => {
    const originalDirectory = process.cwd();
    const root = mkdtempSync(join(tmpdir(), "pledge-release-git-"));
    temporaryRepos.push(root);
    const localVariables = execFileSync("git", ["rev-parse", "--local-env-vars"], { encoding: "utf8" }).trim().split("\n");
    const savedVariables = Object.fromEntries(localVariables.map(name => [name, process.env[name]]));
    for (const name of localVariables) delete process.env[name];
    const git = (...args: string[]) => execFileSync("git", args, {
      cwd: root, encoding: "utf8",
      env: { ...process.env, GIT_AUTHOR_DATE: "2000-01-01T00:00:00Z", GIT_COMMITTER_DATE: "2000-01-01T00:00:00Z" },
    }).trim();
    const write = (path: string, content: string) => writeFileSync(join(root, path), content);
    try {
      git("init", "-q", "-b", "main");
      git("config", "user.name", "Release fixture");
      git("config", "user.email", "fixture@example.invalid");
      git("config", "commit.gpgsign", "false");
      git("config", "core.hooksPath", "/dev/null");
      git("remote", "add", "origin", root);
      write(".release-please-manifest.json", '{".":"0.3.0"}');
      git("add", ".");
      git("commit", "-qm", "chore: initialize fixture");
      const baseline = git("rev-parse", "HEAD");
      write("release-please-config.json", JSON.stringify({ "bootstrap-sha": baseline }));
      git("add", ".");
      git("commit", "-qm", scenario === "breaking-history" ? "feat!: remove a command" : "fix: correct comparison");
      const baseSha = git("rev-parse", "HEAD");
      git("checkout", "-qb", "release-please--branches--main");
      const nextVersion = scenario === "breaking-history" ? "0.4.0" : "0.3.1";
      write(".release-please-manifest.json", JSON.stringify({ ".": nextVersion }));
      git("add", ".");
      git("commit", "-qm", `chore: release ${nextVersion}`);
      const headSha = git("rev-parse", "HEAD");
      git("checkout", "-q", "main");
      process.chdir(root);

      const pull = {
        ...candidate().pull, number: 1, title: `chore: release ${nextVersion}`,
        updated_at: "2000-01-01T00:00:00Z",
        base: { ...candidate().pull.base, sha: baseSha },
        head: { ...candidate().pull.head, sha: headSha },
      };
      const merges: Record<string, unknown>[] = [];
      const dispatches: Record<string, unknown>[] = [];
      const logs: string[] = [];
      let reads = 0;
      const listPulls = () => {};
      const listFiles = () => {};
      const listChecks = () => {};
      const github = {
        paginate: async (method: unknown) => {
          if (method === listPulls) return [pull];
          if (method === listFiles) return [{ filename: scenario === "unexpected-file" ? "src/main.rs" : ".release-please-manifest.json", status: "modified" }];
          if (method === listChecks) return candidate().checks;
          throw new Error("Unexpected pagination endpoint");
        },
        rest: {
          pulls: {
            list: listPulls, listFiles,
            get: async () => {
              reads++;
              const refreshed = structuredClone(pull);
              if (reads > 1 && scenario === "new-head") refreshed.head.sha = "f".repeat(40);
              if (reads > 1 && scenario === "held") refreshed.labels.push({ name: "release: hold" });
              return { data: refreshed };
            },
            merge: async (parameters: Record<string, unknown>) => {
              merges.push(parameters);
              return { data: { merged: true } };
            },
          },
          checks: { listForRef: listChecks },
          actions: { createWorkflowDispatch: async (parameters: Record<string, unknown>) => {
            dispatches.push(parameters);
            if (scenario === "dispatch-failed") throw new Error("dispatch unavailable");
          } },
        },
      };
      const run = mergeRelease({ github, context: { repo: { owner: "stentorian-io", repo: "pledge" } },
        core: { info: (message: string) => logs.push(message) }, dryRun: scenario === "dry-run" });
      if (scenario === "dispatch-failed") await expect(run).rejects.toThrow("dispatch unavailable");
      else await run;
      if (["eligible", "breaking-history", "dispatch-failed"].includes(scenario)) {
        expect(merges).toEqual([{ owner: "stentorian-io", repo: "pledge", pull_number: 1,
          sha: headSha, merge_method: "squash", commit_title: `chore: release ${nextVersion}`, commit_message: "" }]);
        expect(dispatches).toEqual([{ owner: "stentorian-io", repo: "pledge", workflow_id: "release.yml", ref: "main" }]);
      } else {
        expect(merges).toHaveLength(0);
        expect(dispatches).toHaveLength(0);
      }
      if (scenario === "dry-run") expect(logs.join("\n")).toContain("Would merge");
    } finally {
      process.chdir(originalDirectory);
      for (const name of localVariables) {
        if (savedVariables[name] === undefined) delete process.env[name];
        else process.env[name] = savedVariables[name];
      }
    }
  },
);
