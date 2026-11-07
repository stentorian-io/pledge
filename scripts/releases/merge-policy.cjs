const { isBreaking } = require("../lib/commit-message.cjs");

const quietPeriod = 24 * 60 * 60 * 1000;

function releasePullBlocker(pull) {
  if (pull.user.login !== "github-actions[bot]" || pull.user.type !== "Bot") return "not the release bot";
  if (pull.state !== "open" || pull.draft || pull.base.ref !== "main"
    || pull.head.ref !== "release-please--branches--main"
    || pull.head.repo?.full_name !== pull.base.repo.full_name) return "not an open release PR";
  return null;
}

function hasOnlyReleaseFiles(files) {
  const allowed = new Set(["Cargo.toml", "Cargo.lock", "CHANGELOG.md", ".release-please-manifest.json", "action.yml"]);
  return files.length > 0 && files.every(file => allowed.has(file.filename) && file.status === "modified");
}

function mergeBlocker({ pull, now = Date.now(), messages, previousVersion, nextVersion, headDate, checks }) {
  const pullBlocker = releasePullBlocker(pull);
  if (pullBlocker) return pullBlocker;
  const labels = pull.labels.map(label => label.name);
  if (!labels.includes("autorelease: pending") || labels.includes("release: hold")) return "release is held or not pending";
  if (!/^\d+\.\d+\.\d+$/.test(previousVersion) || !/^\d+\.\d+\.\d+$/.test(nextVersion)) return "unknown version transition";
  const previous = previousVersion.split(".").map(Number);
  const next = nextVersion.split(".").map(Number);
  const increased = next.some((part, index) => part > previous[index] && next.slice(0, index).every((prefix, offset) => prefix === previous[offset]));
  if (!increased) return "version must increase";
  const breaking = messages.some(isBreaking) || /BREAKING[ -]CHANGES?/i.test(pull.body ?? "");
  if (next[0] !== previous[0] || (previous[0] > 0 && breaking)) {
    return "stable breaking releases and major transitions require manual merge";
  }
  if (breaking && next[1] <= previous[1]) return "breaking pre-1.0 releases must increase the minor version";
  const changedAt = Math.max(Date.parse(pull.updated_at), Date.parse(headDate));
  if (!Number.isFinite(changedAt) || now - changedAt < quietPeriod) return "waiting for 24 quiet hours";
  if (pull.mergeable !== true || pull.mergeable_state !== "clean") return "GitHub merge requirements are not satisfied";
  for (const name of ["Release checks", "Release image"]) {
    if (!checks.some(check => check.name === name && check.app?.slug === "github-actions"
      && check.status === "completed" && check.conclusion === "success")) return "release checks have not passed";
  }
  if (checks.some(check => check.status !== "completed" || !["success", "neutral", "skipped"].includes(check.conclusion))) {
    return "a check is pending or failed";
  }
  return null;
}

module.exports = { mergeBlocker, releasePullBlocker, hasOnlyReleaseFiles };
