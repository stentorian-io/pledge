const { existsSync, readFileSync, realpathSync } = require("node:fs");
const { dirname, isAbsolute, join, relative, resolve } = require("node:path");
const glob = require("fast-glob");
const semver = require("semver");
const YAML = require("yaml");

function packageDirectory(workspace, path) {
  const directory = realpathSync(resolve(workspace, path));
  const within = relative(workspace, directory);
  if (within === ".." || within.startsWith("../") || isAbsolute(within)) {
    throw new Error("The package path must be inside the checked-out workspace");
  }
  return directory;
}

function readManifest(directory) {
  const manifest = JSON.parse(readFileSync(join(directory, "package.json"), "utf8"));
  if (!manifest || Array.isArray(manifest) || typeof manifest !== "object") throw new Error(`Invalid package.json in ${directory}`);
  return manifest;
}

function workspaceDirectories(workspace, patterns) {
  if (!Array.isArray(patterns) || patterns.some(pattern => typeof pattern !== "string" || !pattern
    || isAbsolute(pattern) || pattern.split("/").includes("..") || pattern.startsWith("!/"))) {
    throw new Error("Workspace declarations must contain relative directory patterns inside the checkout");
  }
  if (!patterns.length) return [];
  const manifests = glob.sync(patterns.map(pattern => `${pattern.replace(/\/$/, "")}/package.json`), {
    cwd: workspace, onlyFiles: true, unique: true, followSymbolicLinks: false,
    ignore: ["**/node_modules/**", "**/.git/**"],
  });
  return [...new Set(manifests.map(path => packageDirectory(workspace, dirname(path))))]
    .filter(directory => directory !== workspace).sort();
}

function discoverPackages(workspace, path) {
  const rootManifest = existsSync(join(workspace, "package.json")) ? readManifest(workspace) : null;
  let directories;
  if (path) {
    directories = [packageDirectory(workspace, path)];
  } else {
    const declarations = [];
    if (rootManifest?.workspaces !== undefined) {
      const workspaces = rootManifest.workspaces;
      declarations.push(workspaceDirectories(workspace, Array.isArray(workspaces) ? workspaces : workspaces?.packages));
    }
    if (existsSync(join(workspace, "pnpm-workspace.yaml"))) {
      const document = YAML.parseDocument(readFileSync(join(workspace, "pnpm-workspace.yaml"), "utf8"));
      if (document.errors.length || document.warnings.length) throw new Error("Invalid or unsupported pnpm-workspace.yaml");
      const config = document.toJS({ maxAliasCount: 100 });
      if (!config || typeof config !== "object" || Array.isArray(config)) throw new Error("Invalid pnpm-workspace.yaml");
      declarations.push(workspaceDirectories(workspace, config.packages ?? []));
    }
    if (declarations.length === 2 && JSON.stringify(declarations[0]) !== JSON.stringify(declarations[1])) {
      throw new Error("package.json and pnpm-workspace.yaml select different workspaces; use path to select a package explicitly");
    }
    directories = [...new Set([...(rootManifest ? [workspace] : []), ...(declarations[0] || [])])];
  }
  const packages = [], skipped = [], names = new Set();
  for (const directory of directories.sort()) {
    const manifest = readManifest(directory);
    const path = relative(workspace, directory) || ".";
    if (manifest.private === true) {
      skipped.push(`${path}: private package`);
      continue;
    }
    if (typeof manifest.name !== "string" || !/^(?:@[a-zA-Z0-9_~.-]+\/)?[a-zA-Z0-9_~-][a-zA-Z0-9_~.-]*$/.test(manifest.name)) {
      throw new Error(`${path}/package.json requires a valid npm package name`);
    }
    const version = typeof manifest.version === "string" && semver.parse(manifest.version);
    if (!version) throw new Error(`${path}/package.json requires a valid SemVer version`);
    if (names.has(manifest.name)) throw new Error(`Multiple packages use the name ${manifest.name}`);
    names.add(manifest.name);
    packages.push({ directory, path, name: manifest.name, version: manifest.version, major: version.major });
  }
  if (!packages.length) throw new Error(`No publishable packages selected${skipped.length ? ` (${skipped.join("; ")})` : ""}`);
  return { packages, skipped };
}

module.exports = { discoverPackages };
