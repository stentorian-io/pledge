const { createHash } = require("node:crypto");
const { spawn } = require("node:child_process");

const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");
const versionParts = tag => {
  if (!/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag ?? "")) {
    throw new Error("Promotion requires an exact vX.Y.Z release tag");
  }
  return tag.slice(1).split(".").map(BigInt);
};

async function commitForRef(github, repository, ref) {
  let { data: { object } } = await github.rest.git.getRef({ ...repository, ref });
  for (let depth = 0; object.type === "tag" && depth < 8; depth++) {
    ({ data: { object } } = await github.rest.git.getTag({ ...repository, tag_sha: object.sha }));
  }
  if (object.type !== "commit" || !/^[a-f0-9]{40}$/.test(object.sha)) throw new Error(`Invalid commit for ${ref}`);
  return object.sha;
}

async function actionVersion(github, repository, revision) {
  const { data } = await github.rest.repos.getContent({ ...repository, path: "action.yml", ref: revision });
  if (data.encoding !== "base64" || typeof data.content !== "string") throw new Error("Missing action metadata");
  const metadata = Buffer.from(data.content, "base64").toString("utf8");
  const image = metadata.match(/^\s+image:\s+docker:\/\/([^\s#]+)\s*(?:#.*)?$/m)?.[1];
  const registry = `ghcr.io/${repository.owner}/${repository.repo}`.toLowerCase();
  if (!image?.startsWith(`${registry}:`)) throw new Error("Action image does not match its repository");
  const tag = image.slice(registry.length + 1);
  versionParts(tag);
  return tag;
}

async function publicBytes(url, fetcher, headers = {}) {
  const response = await fetcher(url, { headers, signal: AbortSignal.timeout(120_000) });
  if (!response.ok) throw new Error(`Public artifact unavailable: ${url} (HTTP ${response.status})`);
  return { bytes: Buffer.from(await response.arrayBuffer()), headers: response.headers };
}

async function verifyPublicArtifacts(repository, release, tag, revision, fetcher) {
  const archive = `pledge-${tag}-x86_64-unknown-linux-gnu.tar.gz`;
  const names = [archive, "container-image.json", "SHA256SUMS"];
  const contents = {};
  for (const name of names) {
    const assets = release.assets.filter(asset => asset.name === name && asset.state === "uploaded" && asset.size > 0);
    if (assets.length !== 1) throw new Error(`Missing or ambiguous release asset: ${name}`);
    const url = `https://github.com/${repository.owner}/${repository.repo}/releases/download/${tag}/${name}`;
    contents[name] = (await publicBytes(url, fetcher)).bytes;
  }
  const checksums = contents.SHA256SUMS.toString("utf8").trim().split("\n");
  for (const name of [archive, "container-image.json"]) {
    if (checksums.filter(line => line === `${sha256(contents[name])}  ${name}`).length !== 1) {
      throw new Error(`Release checksum mismatch: ${name}`);
    }
  }
  const record = JSON.parse(contents["container-image.json"].toString("utf8"));
  const registry = `ghcr.io/${repository.owner}/${repository.repo}`.toLowerCase();
  const digest = record.image?.slice(`${registry}@`.length);
  if (!record.image?.startsWith(`${registry}@`) || !/^sha256:[a-f0-9]{64}$/.test(digest ?? "")
    || record.tag !== tag || record.revision !== revision || record.platform !== "linux/amd64") {
    throw new Error("Published image record does not match the release");
  }

  // Request an anonymous pull token; the publisher's GITHUB_TOKEN must not mask a private image.
  const imagePath = `${repository.owner}/${repository.repo}`.toLowerCase();
  const tokenUrl = `https://ghcr.io/token?service=ghcr.io&scope=repository:${imagePath}:pull`;
  const token = JSON.parse((await publicBytes(tokenUrl, fetcher)).bytes.toString("utf8")).token;
  if (typeof token !== "string" || !token) throw new Error("Anonymous registry token is missing");
  const headers = { Authorization: `Bearer ${token}`, Accept: "application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json" };
  const manifest = await publicBytes(`https://ghcr.io/v2/${imagePath}/manifests/${tag}`, fetcher, headers);
  if (`sha256:${sha256(manifest.bytes)}` !== digest || manifest.headers.get("docker-content-digest") !== digest) {
    throw new Error("Public image tag does not match the released digest");
  }
  const configDigest = JSON.parse(manifest.bytes.toString("utf8")).config?.digest;
  if (!/^sha256:[a-f0-9]{64}$/.test(configDigest ?? "")) throw new Error("Image config digest is missing");
  const config = await publicBytes(`https://ghcr.io/v2/${imagePath}/blobs/${configDigest}`, fetcher, headers);
  if (`sha256:${sha256(config.bytes)}` !== configDigest) throw new Error("Public image config checksum mismatch");
  const image = JSON.parse(config.bytes.toString("utf8"));
  const labels = image.config?.Labels ?? {};
  if (image.os !== "linux" || image.architecture !== "amd64"
    || labels["org.opencontainers.image.revision"] !== revision
    || labels["org.opencontainers.image.version"] !== tag.slice(1)
    || labels["org.opencontainers.image.source"] !== `https://github.com/${repository.owner}/${repository.repo}`) {
    throw new Error("Public image provenance does not match the release");
  }
}

function verifyPublicImage(image, version) {
  // Keep GitHub's HTTP client responsive while Docker runs the synchronous smoke suite.
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [require.resolve("./verify-published-image.cjs"), image, version], { stdio: "inherit" });
    child.once("error", reject);
    child.once("close", (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`Public image verification failed: ${signal ?? `exit ${code}`}`));
    });
  });
}

async function promoteLatest({ github, context, core, tag, fetcher = fetch, verifyImage = verifyPublicImage }) {
  const next = versionParts(tag);
  const repository = context.repo;
  const { data: release } = await github.rest.repos.getReleaseByTag({ ...repository, tag });
  if (release.draft || release.prerelease || release.tag_name !== tag || !release.published_at) {
    throw new Error("Only a published non-prerelease can become latest");
  }
  const revision = await commitForRef(github, repository, `tags/${tag}`);
  if (await actionVersion(github, repository, revision) !== tag) throw new Error("Action version does not match the release");
  let current;
  try { current = await commitForRef(github, repository, "tags/latest"); }
  catch (error) { if (error.status !== 404) throw error; }
  if (current) {
    const currentTag = await actionVersion(github, repository, current);
    const previous = versionParts(currentTag);
    const difference = next.map((part, index) => part - previous[index]).find(part => part !== 0n) ?? 0n;
    if (difference < 0n) {
      core.info(`Keeping latest at ${currentTag}; ${tag} is older`);
      return;
    }
    if (difference === 0n && current !== revision) throw new Error("Same release version points to different commits");
  }
  await verifyPublicArtifacts(repository, release, tag, revision, fetcher);
  await verifyImage(`ghcr.io/${repository.owner}/${repository.repo}:${tag}`.toLowerCase(), tag.slice(1));
  if (!current) await github.rest.git.createRef({ ...repository, ref: "refs/tags/latest", sha: revision });
  else if (current !== revision) await github.rest.git.updateRef({ ...repository, ref: "tags/latest", sha: revision, force: true });
  core.info(`latest points to ${tag} (${revision}); public release artifacts verified`);
}

module.exports = { promoteLatest, verifyPublicImage };
