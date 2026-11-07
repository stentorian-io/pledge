import { expect, test } from "bun:test";
import dispatchChecks from "./dispatch-checks.cjs";
import checkDispatch from "./check-dispatch.cjs";

function fixture() {
  const repo = { owner: "stentorian-io", repo: "pledge" };
  const sha = "a".repeat(40);
  const pull = {
    number: 7, title: "chore: release 0.4.0", body: "Release notes",
    user: { login: "github-actions[bot]", type: "Bot" }, state: "open", draft: false,
    base: { ref: "main", repo: { full_name: "stentorian-io/pledge" } },
    head: { ref: "release-please--branches--main", sha, repo: { full_name: "stentorian-io/pledge" } },
  };
  const context = {
    repo, sha, ref: `refs/heads/${pull.head.ref}`, eventName: "workflow_dispatch", workflow: "Checks",
    payload: { inputs: { "expected-sha": sha, "pull-request": "7" } },
  };
  return { pull, context };
}

test.each(["new", "already-checked", "failed-checks", "unexpected-files", "new-head", "closed", "wrong-author", "fork"])(
  "release validation dispatch respects the PR identity and existing runs: %s", async scenario => {
    const { pull, context } = fixture();
    if (scenario === "wrong-author") pull.user.login = "someone";
    if (scenario === "fork") pull.head.repo.full_name = "someone/pledge";
    const calls: unknown[] = [];
    const listPulls = () => {};
    const listFiles = () => {};
    const listRuns = () => {};
    const github = {
      paginate: async (method: unknown, parameters: Record<string, unknown>) => {
        if (method === listPulls) return [pull];
        if (method === listFiles) return [{ filename: scenario === "unexpected-files" ? ".github/workflows/checks.yml" : "Cargo.toml", status: "modified" }];
        if (method === listRuns) {
          expect(parameters.head_sha).toBe(context.sha);
          expect(parameters.event).toBe("workflow_dispatch");
          return ["already-checked", "failed-checks"].includes(scenario)
            ? [{ head_sha: context.sha, conclusion: scenario === "failed-checks" ? "failure" : "success" }] : [];
        }
        throw new Error("Unexpected endpoint");
      },
      rest: {
        pulls: {
          list: listPulls, listFiles,
          get: async () => {
            const refreshed = structuredClone(pull);
            if (scenario === "new-head") refreshed.head.sha = "b".repeat(40);
            if (scenario === "closed") refreshed.state = "closed";
            return { data: refreshed };
          },
        },
        actions: { listWorkflowRuns: listRuns, createWorkflowDispatch: async (parameters: unknown) => calls.push(parameters) },
      },
    };
    await dispatchChecks({ github, context, core: { info() {} } });
    expect(calls).toEqual(scenario === "new" ? [{
      ...context.repo, workflow_id: "checks.yml", ref: pull.head.ref,
      inputs: { "expected-sha": context.sha, "pull-request": "7" },
    }] : []);
  },
);

test("dispatched validation requires the requested commit and current release PR", async () => {
  const { pull, context } = fixture();
  const github = { rest: { pulls: { get: async () => ({ data: pull }) } } };
  await checkDispatch({ github, context });
  await checkDispatch({ github: {}, context: { eventName: "pull_request" } });
  await checkDispatch({ github: {}, context: { eventName: "workflow_dispatch", workflow: "Release", payload: {} } });
  for (const change of [
    { sha: "b".repeat(40) },
    { ref: "refs/heads/main" },
    { payload: { inputs: { "expected-sha": context.sha, "pull-request": "7; echo unwanted" } } },
  ]) await expect(checkDispatch({ github, context: { ...context, ...change } })).rejects.toThrow();
  pull.head.sha = "b".repeat(40);
  await expect(checkDispatch({ github, context })).rejects.toThrow("changed");
  pull.head.sha = context.sha;
  pull.title = "release with an invalid title";
  await expect(checkDispatch({ github, context })).rejects.toThrow();
});
