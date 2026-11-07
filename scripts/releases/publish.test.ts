import { afterEach, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { publishedImage, uploadAssets } from "./publish-release.cjs";

const directories: string[] = [];
afterEach(() => {
  for (const directory of directories.splice(0)) rmSync(directory, { recursive: true, force: true });
});

test.each(["Organization", "User"])("image lookup uses the package owner's API: %s", async type => {
  const versions = [{ name: `sha256:${"a".repeat(64)}`, metadata: { container: { tags: ["v0.4.0"] } } }];
  const github = {
    rest: { repos: { get: async () => ({ data: { owner: { type } } }) } },
    paginate: async (route: string, parameters: Record<string, unknown>) => {
      expect(route).toContain(type === "Organization" ? "/orgs/{org}/" : "/users/{username}/");
      expect(parameters).toEqual({ [type === "Organization" ? "org" : "username"]: "owner", package_name: "pledge", per_page: 100 });
      return versions;
    },
  };
  expect(await publishedImage(github, { owner: "owner", repo: "Pledge" })).toEqual(versions);
});

test.each([404, 401, 403, 500])("only an absent package permits a fresh image: HTTP %s", async status => {
  const github = {
    rest: { repos: { get: async () => ({ data: { owner: { type: "Organization" } } }) } },
    paginate: async () => { throw Object.assign(new Error(`registry ${status}`), { status }); },
  };
  const lookup = publishedImage(github, { owner: "owner", repo: "pledge" });
  if (status === 404) expect(await lookup).toEqual([]);
  else await expect(lookup).rejects.toThrow(`registry ${status}`);
});

test.each(["new", "matching", "different"])("release uploads preserve existing asset bytes: %s", async scenario => {
  const directory = mkdtempSync(join(tmpdir(), "pledge-publish-"));
  directories.push(directory);
  const path = join(directory, "binary.tar.gz");
  const bytes = Buffer.from([0, 1, 255, 254, 13, 10]);
  writeFileSync(path, bytes);
  const uploads: unknown[] = [];
  const github = { rest: { repos: {
    getReleaseAsset: async (parameters: Record<string, unknown>) => {
      expect(parameters.asset_id).toBe(8);
      expect(parameters.headers).toEqual({ accept: "application/octet-stream" });
      return { data: scenario === "different" ? Buffer.from("other") : Uint8Array.from(bytes).buffer };
    },
    uploadReleaseAsset: async (parameters: unknown) => uploads.push(parameters),
  } } };
  const release = { id: 7, assets: scenario === "new" ? [] : [{ id: 8, name: "binary.tar.gz" }] };
  const upload = uploadAssets(github, { owner: "owner", repo: "pledge" }, release, [path]);
  if (scenario === "different") await expect(upload).rejects.toThrow("refusing to overwrite");
  else await upload;
  expect(uploads).toEqual(scenario === "new" ? [{ owner: "owner", repo: "pledge", release_id: 7,
    name: "binary.tar.gz", data: bytes, headers: { "content-type": "application/octet-stream", "content-length": bytes.length } }] : []);
});
