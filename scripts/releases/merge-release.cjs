const { execFileSync } = require("node:child_process");
const { readFileSync } = require("node:fs");
const { mergeBlocker } = require("./merge-policy.cjs");

module.exports = async function mergeRelease({ github, context, core, appSlug, dryRun = false }) {
  const repository = context.repo;
  const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();
  const mainSha = git("rev-parse", "HEAD");
  const previousVersion = JSON.parse(readFileSync(".release-please-manifest.json", "utf8"))["."];
  const config = JSON.parse(readFileSync("release-please-config.json", "utf8"));
  const baseline = previousVersion === "0.3.0" ? config["bootstrap-sha"] : `v${previousVersion}`;
  const candidates = await github.paginate(github.rest.pulls.list, { ...repository, state: "open", base: "main", per_page: 100 });
  for (const candidate of candidates) {
    if (candidate.user.login !== `${appSlug}[bot]` || candidate.head.ref !== "release-please--branches--main") continue;
    const parameters = { ...repository, pull_number: candidate.number };
    let { data: pull } = await github.rest.pulls.get(parameters);
    if (pull.base.sha !== mainSha || pull.head.repo?.full_name !== pull.base.repo.full_name) continue;
    const head = pull.head.sha;
    git("fetch", "origin", head);
    const files = await github.paginate(github.rest.pulls.listFiles, { ...parameters, per_page: 100 });
    const allowedFiles = new Set(["Cargo.toml", "Cargo.lock", "CHANGELOG.md", ".release-please-manifest.json"]);
    if (files.length === 0 || files.some(file => !allowedFiles.has(file.filename) || file.status !== "modified")) {
      core.info(`PR #${pull.number}: unexpected release files; manual review required`);
      continue;
    }
    const nextVersion = JSON.parse(git("show", `${head}:.release-please-manifest.json`))["."];
    if (pull.title !== `chore: release ${nextVersion}`) continue;
    git("merge-base", "--is-ancestor", baseline, head);
    const messages = git("log", "--format=%B%x00", `${baseline}..${head}`).split("\0");
    const headDate = git("show", "-s", "--format=%cI", head);
    const checks = await github.paginate(github.rest.checks.listForRef, { ...repository, ref: head, per_page: 100 });
    const { data: refreshed } = await github.rest.pulls.get(parameters);
    if (refreshed.head.sha !== head || refreshed.base.sha !== mainSha) continue;
    pull = refreshed;
    const reason = mergeBlocker({ pull, appSlug, messages, previousVersion, nextVersion, headDate, checks });
    if (reason) {
      core.info(`PR #${pull.number}: ${reason}`);
      continue;
    }
    if (dryRun) {
      core.info(`Would merge release PR #${pull.number} at ${head}`);
      continue;
    }
    const result = await github.rest.pulls.merge({
      ...parameters, sha: head, merge_method: "squash", commit_title: pull.title,
      commit_message: "",
    });
    if (!result.data.merged) throw new Error(`Release merge failed: ${result.data.message}`);
    core.info(`Merged release PR #${pull.number} at ${head}`);
  }
};
