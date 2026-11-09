const { execFileSync } = require("node:child_process");
const { readFileSync } = require("node:fs");

const publishedFiles = new Set([
  "Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain", "rust-toolchain.toml",
  "Dockerfile", ".dockerignore", "action.yml", "LICENSE-MIT", "LICENSE-APACHE",
  "scripts/action/package.json", "scripts/action/package-lock.json",
  "scripts/releases/publish-release.cjs", ".github/workflows/publish.yml",
]);

function isPublishedInput(path) {
  if (path.startsWith("src/")) {
    return !/(^|\/)tests\//.test(path) && !/(^|\/)(tests|[^/]+[_-]tests)\.rs$/.test(path) && !path.endsWith(".md");
  }
  if (path.startsWith(".cargo/")) return true;
  if (/^scripts\/action\/[^/]+\.cjs$/.test(path)) return true;
  return publishedFiles.has(path);
}

function changedPublishedInputs(baseline, revision = "HEAD", cwd = process.cwd()) {
  const git = (...args) => execFileSync("git", args, { cwd, encoding: "utf8" });
  const base = git("rev-parse", "--verify", "--end-of-options", `${baseline}^{commit}`).trim();
  const head = git("rev-parse", "--verify", "--end-of-options", `${revision}^{commit}`).trim();
  git("merge-base", "--is-ancestor", base, head);
  return git("diff", "--name-only", "--no-renames", "-z", base, head, "--")
    .split("\0").filter(path => path && isPublishedInput(path));
}

async function releaseInputs({ github, context, core }) {
  let baseline;
  try {
    const { data: release } = await github.rest.repos.getLatestRelease(context.repo);
    if (!/^v\d+\.\d+(?:\.\d+)?$/.test(release.tag_name)) throw new Error("latest release has an unexpected version tag");
    baseline = release.tag_name;
  } catch (error) {
    if (error.status !== 404) throw error;
    baseline = JSON.parse(readFileSync("release-please-config.json", "utf8"))["bootstrap-sha"];
    if (!/^[0-9a-f]{40}$/.test(baseline ?? "")) throw new Error("release bootstrap revision is missing or invalid");
  }
  const paths = changedPublishedInputs(baseline);
  core.info(paths.length ? `Published inputs changed since ${baseline}: ${paths.join(", ")}`
    : `No published inputs changed since ${baseline}; skipping release PR creation`);
  return paths.length > 0;
}

module.exports = { isPublishedInput, changedPublishedInputs, releaseInputs };
