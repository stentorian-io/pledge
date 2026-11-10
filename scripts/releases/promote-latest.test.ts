import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { promoteLatest } from "./promote-latest.cjs";
const { parse } = require("../action/node_modules/yaml");

const digest = (bytes: string | Buffer) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
const revision = "a".repeat(40);
const previousRevision = "b".repeat(40);

function fixture({ current = "", scenario = "", tag = "v0.4.0" } = {}) {
  const updates: unknown[] = [];
  const requests: string[] = [];
  const logs: string[] = [];
  const archive = `pledge-${tag}-x86_64-unknown-linux-gnu.tar.gz`;
  const registry = "ghcr.io/owner/pledge";
  const config = JSON.stringify({ os: "linux", architecture: scenario === "wrong platform" ? "arm64" : "amd64", config: { Labels: {
    "org.opencontainers.image.revision": scenario === "wrong provenance" ? previousRevision : revision,
    "org.opencontainers.image.version": tag.slice(1), "org.opencontainers.image.source": "https://github.com/owner/pledge",
  } } });
  const manifest = JSON.stringify({ schemaVersion: 2, config: { digest: digest(config) }, layers: [] });
  const record = JSON.stringify({ image: `${registry}@${digest(manifest)}`, tag,
    revision: scenario === "wrong record" ? previousRevision : revision, platform: "linux/amd64" });
  const bodies: Record<string, string> = { [archive]: "binary and licenses", "container-image.json": record };
  bodies.SHA256SUMS = Object.entries(bodies).map(([name, body]) => `${digest(body).slice(7)}  ${name}\n`).join("");
  if (scenario === "wrong checksum") bodies[archive] += "changed";
  const release = { draft: scenario === "draft", prerelease: scenario === "prerelease", tag_name: tag,
    published_at: scenario === "unpublished" ? null : "2026-11-10T00:00:00Z",
    assets: Object.keys(bodies).filter(name => scenario !== "missing asset" || name !== archive)
      .map(name => ({ name, state: scenario === "unfinished upload" ? "new" : "uploaded", size: bodies[name].length })),
  };
  const github = { rest: {
    repos: {
      getReleaseByTag: async () => ({ data: release }),
      getContent: async ({ ref }: { ref: string }) => ({ data: { encoding: "base64", content: Buffer.from(
        `runs:\n  using: docker\n  image: docker://${registry}:${ref === previousRevision ? current : scenario === "wrong action version" ? "v0.3.0" : tag} # x-release-please-version\n`,
      ).toString("base64") } }),
    },
    git: {
      getRef: async ({ ref }: { ref: string }) => {
        if (ref === "tags/latest" && (!current || scenario === "ref forbidden")) {
          throw Object.assign(new Error("ref unavailable"), { status: scenario === "ref forbidden" ? 403 : 404 });
        }
        return { data: { object: { type: scenario === "annotated tag" ? "tag" : "commit",
          sha: ref === "tags/latest" && (current !== tag || scenario === "same version different commit") ? previousRevision : revision } } };
      },
      getTag: async () => ({ data: { object: { type: "commit", sha: revision } } }),
      createRef: async (parameters: unknown) => { updates.push(["create", parameters]); },
      updateRef: async (parameters: unknown) => { updates.push(["update", parameters]); },
    },
  } };
  const fetcher = async (url: string, options: RequestInit) => {
    requests.push(url);
    expect(options.signal).toBeInstanceOf(AbortSignal);
    const headers = new Headers(options.headers);
    if (url.startsWith("https://github.com/owner/pledge/releases/download/")) {
      expect(headers.has("authorization")).toBe(false);
      return new Response(bodies[url.split("/").at(-1)!], { status: scenario === "private release asset" ? 404 : 200 });
    }
    if (url === "https://ghcr.io/token?service=ghcr.io&scope=repository:owner/pledge:pull") {
      expect(headers.has("authorization")).toBe(false);
      return new Response(JSON.stringify({ token: "anonymous-token" }), { status: scenario === "private image" ? 403 : 200 });
    }
    expect(headers.get("authorization")).toBe("Bearer anonymous-token");
    if (url === `https://ghcr.io/v2/owner/pledge/manifests/${tag}`) {
      return new Response(scenario === "wrong image tag" ? `${manifest} ` : manifest,
        { headers: { "docker-content-digest": digest(manifest) } });
    }
    expect(url).toBe(`https://ghcr.io/v2/owner/pledge/blobs/${digest(config)}`);
    return new Response(scenario === "corrupt config" ? `${config} ` : config);
  };
  return { updates, requests, logs, run: () => promoteLatest({ github,
    context: { repo: { owner: "owner", repo: "pledge" } }, core: { info: (line: string) => logs.push(line) }, tag, fetcher,
    verifyImage: async (image: string, version: string) => {
      expect(requests).toHaveLength(6);
      expect(updates).toEqual([]);
      expect(image).toBe(`${registry}:${tag}`);
      expect(version).toBe(tag.slice(1));
      if (scenario === "public image smoke failure") throw new Error("image smoke test failed");
    },
  }) };
}

test.each(["", "annotated tag"])("latest is created only after anonymous artifact and provenance verification: %s", async scenario => {
  const setup = fixture({ scenario });
  await setup.run();
  expect(setup.requests).toHaveLength(6);
  expect(setup.updates).toEqual([["create", { owner: "owner", repo: "pledge", ref: "refs/tags/latest", sha: revision }]]);
});

test.each([
  "draft", "prerelease", "unpublished", "wrong action version", "missing asset", "unfinished upload",
  "wrong checksum", "wrong record", "private release asset", "private image", "wrong image tag",
  "corrupt config", "wrong platform", "wrong provenance", "ref forbidden", "public image smoke failure",
])("failed publication verification preserves the existing latest tag: %s", async scenario => {
  const setup = fixture({ current: "v0.3.0", scenario });
  await expect(setup.run()).rejects.toThrow();
  expect(setup.updates).toEqual([]);
});

test("promotion advances only latest and never rewrites an immutable release tag", async () => {
  const setup = fixture({ current: "v0.9.0", tag: "v0.10.0" });
  await setup.run();
  expect(setup.updates).toEqual([["update", { owner: "owner", repo: "pledge", ref: "tags/latest", sha: revision, force: true }]]);
});

test("retrying an older release cannot roll latest backward", async () => {
  const setup = fixture({ current: "v0.10.0", tag: "v0.9.0" });
  await setup.run();
  expect(setup.updates).toEqual([]);
  expect(setup.requests).toEqual([]);
  expect(setup.logs).toEqual(["Keeping latest at v0.10.0; v0.9.0 is older"]);
});

test("retrying the current release verifies its artifacts without rewriting latest", async () => {
  const setup = fixture({ current: "v0.4.0" });
  await setup.run();
  expect(setup.requests).toHaveLength(6);
  expect(setup.updates).toEqual([]);
});

test("a release version cannot silently move to a different commit", async () => {
  const setup = fixture({ current: "v0.4.0", scenario: "same version different commit" });
  await expect(setup.run()).rejects.toThrow("different commits");
  expect(setup.updates).toEqual([]);
});

test.each([0, 1])("image checks leave network event processing active before tag creation: exit %s", status => {
  const directory = mkdtempSync(join(tmpdir(), "pledge-promotion-process-"));
  writeFileSync(join(directory, "docker"), `#!/bin/sh
case "$1" in
  pull) sleep 0.2; exit ${status};;
  create) echo fixture-container;;
  inspect) echo 0;;
esac
`, { mode: 0o755 });
  try {
    const result = spawnSync("node", ["-e", `
      const { verifyPublicImage } = require(process.argv[1]);
      let eventProcessed = false;
      const timer = setTimeout(() => { eventProcessed = true; }, 20);
      (async () => {
        let succeeded = false;
        try { await verifyPublicImage('ghcr.io/owner/pledge:v0.4.0', '0.4.0'); succeeded = true; }
        catch {}
        clearTimeout(timer);
        console.log(JSON.stringify({ eventProcessed, succeeded }));
      })();
    `, fileURLToPath(new URL("./promote-latest.cjs", import.meta.url))], {
      env: { ...process.env, PATH: `${directory}:${process.env.PATH}` }, encoding: "utf8", timeout: 5_000,
    });
    expect(result.status, result.stderr).toBe(0);
    expect(JSON.parse(result.stdout.trim())).toEqual({ eventProcessed: true, succeeded: status === 0 });
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test.each(["latest", "v0.4", "v0.4.0-alpha", "v00.4.0"])("moving and prerelease selectors cannot be promoted: %s", async tag => {
  const setup = fixture({ tag });
  await expect(setup.run()).rejects.toThrow("exact vX.Y.Z");
  expect(setup.updates).toEqual([]);
});

test("publication finishes before promotion and all versions share one promotion concurrency group", () => {
  const publish = parse(readFileSync(new URL("../../.github/workflows/publish.yml", import.meta.url), "utf8"));
  const promotion = parse(readFileSync(new URL("../../.github/workflows/promote-latest.yml", import.meta.url), "utf8"));
  expect(publish.jobs.latest.needs).toBe("publish");
  expect(publish.jobs.latest.uses).toBe("./.github/workflows/promote-latest.yml");
  expect(promotion.concurrency.group).not.toContain("${{");
  expect(promotion.concurrency["cancel-in-progress"]).toBe(false);
});
