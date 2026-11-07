const { execFileSync } = require("node:child_process");
const { createHash } = require("node:crypto");
const { mkdtempSync, mkdirSync, readFileSync, writeFileSync, copyFileSync, rmSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join, basename } = require("node:path");

const run = (command, args) => execFileSync(command, args, { stdio: "inherit" });
const output = (command, args) => execFileSync(command, args, { encoding: "utf8" }).trim();
const sha256 = contents => createHash("sha256").update(contents).digest("hex");

async function publishedImage(github, repository) {
  const { data: repo } = await github.rest.repos.get(repository);
  const organization = repo.owner.type === "Organization";
  const route = organization
    ? "GET /orgs/{org}/packages/container/{package_name}/versions"
    : "GET /users/{username}/packages/container/{package_name}/versions";
  try {
    return await github.paginate(route, {
      [organization ? "org" : "username"]: repository.owner,
      package_name: repository.repo.toLowerCase(), per_page: 100,
    });
  } catch (error) {
    if (error.status === 404) return [];
    throw error;
  }
}

async function uploadAssets(github, repository, release, paths) {
  for (const path of paths) {
    const name = basename(path);
    const contents = readFileSync(path);
    const existing = release.assets.find(asset => asset.name === name);
    if (existing) {
      const response = await github.rest.repos.getReleaseAsset({
        ...repository, asset_id: existing.id, headers: { accept: "application/octet-stream" },
      });
      if (sha256(Buffer.from(response.data)) !== sha256(contents)) {
        throw new Error(`Published asset ${name} differs; refusing to overwrite it`);
      }
    } else {
      await github.rest.repos.uploadReleaseAsset({
        ...repository, release_id: release.id, name, data: contents,
        headers: { "content-type": "application/octet-stream", "content-length": contents.length },
      });
    }
  }
}

function smokeImage(image, version) {
  const container = output("docker", ["create", "--platform", "linux/amd64", "--entrypoint", "node", image, "/tmp/pledge-smoke.cjs", version]);
  try {
    run("docker", ["cp", "scripts/releases/smoke-image.cjs", `${container}:/tmp/pledge-smoke.cjs`]);
    run("docker", ["start", "--attach", container]);
    if (output("docker", ["inspect", "--format", "{{.State.ExitCode}}", container]) !== "0") {
      throw new Error("Release image smoke test failed");
    }
  } finally {
    run("docker", ["rm", "--force", container]);
  }
}

async function publishRelease({ github, context, core, tag }) {
  if (!/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag ?? "")) {
    throw new Error("Publishing requires an exact vX.Y.Z release tag");
  }
  run("bun", ["scripts/releases/check-version.ts", tag]);
  const revision = output("git", ["rev-parse", "HEAD"]);
  if (output("git", ["rev-parse", `${tag}^{commit}`]) !== revision) throw new Error("Checkout does not match the release tag");
  const repository = context.repo;
  const { data: release } = await github.rest.repos.getReleaseByTag({ ...repository, tag });
  const registry = `ghcr.io/${repository.owner}/${repository.repo}`.toLowerCase();
  const version = tag.slice(1);
  const source = `https://github.com/${repository.owner}/${repository.repo}`;
  const image = `${registry}:${tag}`;
  const versions = await publishedImage(github, repository);
  const existing = versions.find(entry => entry.metadata?.container?.tags?.includes(tag));
  if (existing) {
    if (!/^sha256:[0-9a-f]{64}$/.test(existing.name)) throw new Error("Registry returned an invalid image digest");
    run("docker", ["pull", "--platform", "linux/amd64", `${registry}@${existing.name}`]);
    run("docker", ["tag", `${registry}@${existing.name}`, image]);
  } else {
    run("docker", ["build", "--platform", "linux/amd64", "--tag", image,
      "--label", `org.opencontainers.image.revision=${revision}`,
      "--label", `org.opencontainers.image.version=${version}`,
      "--label", `org.opencontainers.image.source=${source}`, "."]);
  }
  const [inspected] = JSON.parse(output("docker", ["image", "inspect", image]));
  const labels = inspected.Config.Labels;
  if (inspected.Os !== "linux" || inspected.Architecture !== "amd64"
    || labels["org.opencontainers.image.revision"] !== revision
    || labels["org.opencontainers.image.version"] !== version
    || labels["org.opencontainers.image.source"] !== source) {
    throw new Error("Image platform or provenance differs from the release; refusing to publish");
  }
  smokeImage(image, version);

  const directory = mkdtempSync(join(tmpdir(), "pledge-release-"));
  const container = output("docker", ["create", image]);
  try {
    const staging = join(directory, "package");
    mkdirSync(staging);
    run("docker", ["cp", `${container}:/usr/local/bin/pledge`, join(staging, "pledge")]);
    for (const license of ["LICENSE-MIT", "LICENSE-APACHE"]) copyFileSync(license, join(staging, license));
    const archive = join(directory, `pledge-${tag}-x86_64-unknown-linux-gnu.tar.gz`);
    run("tar", ["--sort=name", "--mtime=@0", "--owner=0", "--group=0", "--numeric-owner",
      "-czf", archive, "-C", staging, "pledge", "LICENSE-MIT", "LICENSE-APACHE"]);
    if (!existing) run("docker", ["push", image]);
    const digests = JSON.parse(output("docker", ["image", "inspect", "--format", "{{json .RepoDigests}}", image]));
    const digest = digests.find(entry => entry.startsWith(`${registry}@sha256:`));
    if (!digest) throw new Error("Published image has no repository digest");
    const imageRecord = join(directory, "container-image.json");
    writeFileSync(imageRecord, JSON.stringify({ image: digest, tag, revision, platform: "linux/amd64" }, null, 2) + "\n");
    const checksums = join(directory, "SHA256SUMS");
    writeFileSync(checksums, [archive, imageRecord].map(path => `${sha256(readFileSync(path))}  ${basename(path)}\n`).join(""));
    await uploadAssets(github, repository, release, [archive, imageRecord, checksums]);
    core.info(`Published ${tag}: ${digest}`);
  } finally {
    run("docker", ["rm", "--force", container]);
    rmSync(directory, { recursive: true, force: true });
  }
}

module.exports = { publishRelease, publishedImage, uploadAssets, smokeImage };
