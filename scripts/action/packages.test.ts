import { afterEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { discoverPackages } from "./packages.cjs";

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

function fixture() {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "pledge-discovery-")));
  roots.push(root);
  const write = (path: string, content: string) => {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), content);
  };
  const pkg = (path: string, name: string, extra = {}) => write(`${path}/package.json`, JSON.stringify({ name, version: "1.0.0", ...extra }));
  return { root, write, pkg };
}

test.each(["npm", "yarn", "pnpm"])("discovers public %s members and honors exclusions without scanning unrelated manifests", kind => {
  const f = fixture();
  const patterns = ["packages/{one,two,hidden,excluded}", "!packages/excluded"];
  f.pkg(".", "root", { private: true, ...(kind === "npm" ? { workspaces: patterns } : kind === "yarn" ? { workspaces: { packages: patterns } } : {}) });
  if (kind === "pnpm") f.write("pnpm-workspace.yaml", "packages:\n  - 'packages/{one,two,hidden,excluded}'\n  - '!packages/excluded'\n");
  for (const name of ["one", "two", "excluded"]) f.pkg(`packages/${name}`, name);
  f.pkg("packages/hidden", "hidden", { private: true });
  f.pkg("tests/fixtures/example", "unrelated");
  f.pkg("node_modules/installed", "dependency");
  const found = discoverPackages(f.root);
  expect(found.packages.map((pkg: { name: string }) => pkg.name)).toEqual(["one", "two"]);
  expect(found.skipped).toEqual([".: private package", "packages/hidden: private package"]);
});

test("single packages ignore nested fixtures and explicit paths bypass workspace membership", () => {
  const f = fixture();
  f.pkg(".", "root");
  f.pkg("fixtures/example", "fixture");
  expect(discoverPackages(f.root).packages.map((pkg: { name: string }) => pkg.name)).toEqual(["root"]);
  expect(discoverPackages(f.root, "fixtures/example").packages[0].name).toBe("fixture");
});

test("broad workspace globs exclude installed dependencies and Git metadata", () => {
  const f = fixture();
  f.pkg(".", "root", { private: true, workspaces: ["**"] });
  f.pkg("packages/one", "one");
  f.pkg("node_modules/dependency", "dependency");
  f.pkg("packages/one/node_modules/nested", "nested");
  f.pkg(".git/fixture", "metadata");
  expect(discoverPackages(f.root).packages.map((pkg: { name: string }) => pkg.name)).toEqual(["one"]);
});

test("publishable roots are included and overlapping declarations are deduplicated", () => {
  const f = fixture();
  f.pkg(".", "root", { workspaces: ["packages/*", "packages/one", "."] });
  f.pkg("packages/one", "one");
  expect(discoverPackages(f.root).packages.map((pkg: { name: string }) => pkg.name)).toEqual(["root", "one"]);
  expect(discoverPackages(f.root, ".").packages.map((pkg: { name: string }) => pkg.name)).toEqual(["root"]);
});

test.each(["duplicate-name", "invalid-version", "invalid-name", "invalid-json", "invalid-workspaces", "invalid-yaml", "empty", "conflicting", "traversal"])("invalid selections fail visibly: %s", scenario => {
  const f = fixture();
  f.pkg(".", "root", { private: true, workspaces: ["packages/*"] });
  f.pkg("packages/one", "one");
  if (scenario === "duplicate-name") f.pkg("packages/two", "one");
  if (scenario === "invalid-version") f.pkg("packages/one", "one", { version: "latest" });
  if (scenario === "invalid-name") f.pkg("packages/one", "./outside");
  if (scenario === "invalid-json") f.write("packages/one/package.json", "{");
  if (scenario === "invalid-workspaces") f.pkg(".", "root", { workspaces: "packages/*" });
  if (scenario === "invalid-yaml") f.write("pnpm-workspace.yaml", "packages: [\n");
  if (scenario === "empty") f.pkg("packages/one", "one", { private: true });
  if (scenario === "conflicting") f.write("pnpm-workspace.yaml", "packages: []\n");
  if (scenario === "traversal") f.pkg(".", "root", { workspaces: ["../*"] });
  expect(() => discoverPackages(f.root)).toThrow();
});

test("equivalent workspace definitions can coexist", () => {
  const f = fixture();
  f.pkg(".", "root", { private: true, workspaces: [".", "packages/*"] });
  f.pkg("packages/one", "one");
  f.write("pnpm-workspace.yaml", "packages: ['packages/one']\n");
  expect(discoverPackages(f.root).packages[0].name).toBe("one");
});

test("package symlinks cannot select a directory outside the checkout", () => {
  const f = fixture(), outside = fixture();
  f.pkg(".", "root", { workspaces: ["linked"] });
  outside.pkg(".", "outside");
  symlinkSync(outside.root, join(f.root, "linked"));
  expect(() => discoverPackages(f.root, "linked")).toThrow("inside");
  expect(() => discoverPackages(f.root)).toThrow("inside");
});
