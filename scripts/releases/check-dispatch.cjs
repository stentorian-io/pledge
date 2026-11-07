const { releasePullBlocker } = require("./merge-policy.cjs");
const { validateMessage } = require("../lib/commit-message.cjs");

module.exports = async function checkDispatch({ github, context }) {
  // Reusable checks retain the caller's event, including a manual Release run.
  if (context.eventName !== "workflow_dispatch" || context.workflow !== "Checks") return;
  const expected = context.payload.inputs?.["expected-sha"];
  const number = context.payload.inputs?.["pull-request"];
  if (!/^[0-9a-f]{40}$/.test(expected ?? "") || context.sha !== expected) {
    throw new Error("Dispatched checks must run at the requested release PR commit");
  }
  if (!/^[1-9]\d*$/.test(number ?? "")) throw new Error("A release PR number is required");
  const { data: pull } = await github.rest.pulls.get({ ...context.repo, pull_number: Number(number) });
  if (releasePullBlocker(pull) || pull.head.sha !== expected || context.ref !== `refs/heads/${pull.head.ref}`) {
    throw new Error("The release PR changed or is not eligible for dispatched checks");
  }
  const error = validateMessage(`${pull.title}\n\n${pull.body ?? ""}`);
  if (error) throw new Error(error);
};
