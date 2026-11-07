import { afterEach, expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { registryEnvironment, resolveBaseline } from "./registry.cjs";

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

function fixture(versions: unknown) {
  const root = mkdtempSync(join(tmpdir(), "pledge-registry-"));
  roots.push(root);
  const bin = join(root, "bin");
  mkdirSync(bin);
  writeFileSync(join(bin, "npm"), `#!/usr/bin/env node
if (process.env.PLEDGE_NPM_FAILURE) { console.error('registry denied'); process.exit(1); }
if (process.argv[2] === 'prefix') console.log(process.cwd());
else if (process.argv[2] === 'config') console.log(process.env.PLEDGE_USERCONFIG);
else console.log(process.env.PLEDGE_VERSIONS);
`, { mode: 0o755 });
  return { root, env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, PLEDGE_VERSIONS: JSON.stringify(versions),
    PLEDGE_USERCONFIG: join(root, "user.npmrc"), PLEDGE_NPM_FAILURE: "" } };
}

test.each([
  [2, ["3.0.0", "2.9.0", "2.10.0", "2.11.0-beta.1"], "2.10.0"],
  [0, ["0.4.7", "0.5.2", "1.0.0", "0.6.0-alpha.1"], "0.5.2"],
  [1, "1.2.3", "1.2.3"],
  [1, ["2.0.0", "1.0.0-beta.1"], null],
  [0, [], null],
])("selects the highest published non-prerelease in major %i", (major, versions, expected) => {
  const f = fixture(versions);
  expect(resolveBaseline({ name: "fixture", directory: f.root, major }, f.env)).toBe(expected);
});

test.each([null, {}, ["bogus"], [12], ["1.0.0", "broken"]].map(versions => [versions]))("invalid registry metadata is an error: %j", versions => {
  const f = fixture(versions);
  expect(() => resolveBaseline({ name: "fixture", directory: f.root, major: 1 }, f.env)).toThrow();
});

test("registry failures remain errors instead of becoming absent baselines", () => {
  const f = fixture([]);
  f.env.PLEDGE_NPM_FAILURE = "1";
  expect(() => resolveBaseline({ name: "fixture", directory: f.root, major: 1 }, f.env)).toThrow("registry denied");
});

test("workspace npm configuration preserves user credentials and is removed after use", () => {
  const f = fixture([]);
  const project = 'registry=https://project.invalid/\n@scope:registry=https://scoped.invalid/\n';
  const user = 'registry=https://user.invalid/\n//scoped.invalid/:_authToken=${REGISTRY_TOKEN}\n';
  writeFileSync(join(f.root, ".npmrc"), project);
  writeFileSync(f.env.PLEDGE_USERCONFIG, user);
  const environment = registryEnvironment(f.root, { ...f.env, NPM_CONFIG_USERCONFIG: f.env.PLEDGE_USERCONFIG });
  try {
    const merged = readFileSync(environment.env.npm_config_userconfig, "utf8");
    expect(merged).toContain("registry=https://project.invalid/");
    expect(merged).toContain("@scope:registry=https://scoped.invalid/");
    expect(merged).toContain("//scoped.invalid/:_authToken=${REGISTRY_TOKEN}");
    expect(environment.env.NPM_CONFIG_USERCONFIG).toBeUndefined();
    expect(readFileSync(join(f.root, ".npmrc"), "utf8")).toBe(project);
    expect(readFileSync(f.env.PLEDGE_USERCONFIG, "utf8")).toBe(user);
  } finally { environment.dispose(); }
  expect(existsSync(environment.env.npm_config_userconfig)).toBe(false);
});
