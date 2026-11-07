const { releasePullBlocker, hasOnlyReleaseFiles } = require("./merge-policy.cjs");

module.exports = async function dispatchChecks({ github, context, core }) {
  const repository = context.repo;
  const pulls = await github.paginate(github.rest.pulls.list, {
    ...repository, state: "open", base: "main", per_page: 100,
  });
  for (const pull of pulls) {
    if (releasePullBlocker(pull)) continue;
    const parameters = { ...repository, pull_number: pull.number };
    const files = await github.paginate(github.rest.pulls.listFiles, { ...parameters, per_page: 100 });
    if (!hasOnlyReleaseFiles(files)) {
      core.info(`PR #${pull.number}: unexpected release files; validation needs manual review`);
      continue;
    }
    const runs = await github.paginate(github.rest.actions.listWorkflowRuns, {
      ...repository, workflow_id: "checks.yml", head_sha: pull.head.sha,
      event: "workflow_dispatch", per_page: 100,
    });
    // Do not repeatedly run expensive checks or conceal a failed run with a retry.
    if (runs.some(run => run.head_sha === pull.head.sha)) continue;
    const { data: refreshed } = await github.rest.pulls.get(parameters);
    if (releasePullBlocker(refreshed) || refreshed.head.sha !== pull.head.sha) continue;
    await github.rest.actions.createWorkflowDispatch({
      ...repository, workflow_id: "checks.yml", ref: pull.head.ref,
      inputs: { "expected-sha": pull.head.sha, "pull-request": String(pull.number) },
    });
    core.info(`Requested checks for release PR #${pull.number} at ${pull.head.sha}`);
  }
};
