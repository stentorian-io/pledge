const { isBreaking } = require("../lib/commit-message.cjs");

const quietPeriod = 24 * 60 * 60 * 1000;

function mergeBlocker({ pull, appSlug, now = Date.now(), messages, previousVersion, nextVersion, headDate, checks }) {
  if (!appSlug || pull.user.login !== `${appSlug}[bot]` || pull.user.type !== "Bot") return "not the release app";
  if (pull.state !== "open" || pull.draft || pull.base.ref !== "main"
    || pull.head.ref !== "release-please--branches--main"
    || pull.head.repo?.full_name !== pull.base.repo.full_name) return "not an open release PR";
  const labels = pull.labels.map(label => label.name);
  if (!labels.includes("autorelease: pending") || labels.includes("release: hold")) return "release is held or not pending";
  if (!/^\d+\.\d+\.\d+$/.test(previousVersion) || !/^\d+\.\d+\.\d+$/.test(nextVersion)) return "unknown version transition";
  const previous = previousVersion.split(".").map(Number);
  const next = nextVersion.split(".").map(Number);
  const increased = next.some((part, index) => part > previous[index] && next.slice(0, index).every((prefix, offset) => prefix === previous[offset]));
  if (!increased) return "version must increase";
  if (next[0] !== previous[0] || messages.some(isBreaking)
    || /BREAKING[ -]CHANGES?/i.test(pull.body ?? "")) return "breaking releases require manual merge";
  const changedAt = Math.max(Date.parse(pull.updated_at), Date.parse(headDate));
  if (!Number.isFinite(changedAt) || now - changedAt < quietPeriod) return "waiting for 24 quiet hours";
  if (pull.mergeable !== true || pull.mergeable_state !== "clean") return "GitHub merge requirements are not satisfied";
  if (!checks.some(check => check.name === "Release checks" && check.app?.slug === "github-actions"
    && check.status === "completed" && check.conclusion === "success")) return "release checks have not passed";
  if (checks.some(check => check.status !== "completed" || !["success", "neutral", "skipped"].includes(check.conclusion))) {
    return "a check is pending or failed";
  }
  return null;
}

module.exports = { mergeBlocker };
