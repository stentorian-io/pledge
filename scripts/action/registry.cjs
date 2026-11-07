const { spawnSync } = require("node:child_process");
const { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join } = require("node:path");
const ini = require("ini");
const semver = require("semver");

function npm(args, directory, env) {
  const result = spawnSync("npm", args, { cwd: directory, env, encoding: "utf8", timeout: 120_000, maxBuffer: 16 * 1024 * 1024 });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`npm ${args[0]} failed: ${result.stderr || result.signal || result.status}`);
  return result.stdout.trim();
}

function registryEnvironment(workspace, env) {
  const projectConfig = join(workspace, ".npmrc");
  if (!existsSync(projectConfig)) return { env, dispose() {} };
  const userConfig = npm(["config", "get", "userconfig", "--workspaces=false"], workspace, env);
  const readConfig = path => existsSync(path) ? ini.parse(readFileSync(path, "utf8")) : {};
  const config = { ...readConfig(userConfig), ...readConfig(projectConfig) };
  const directory = mkdtempSync(join(tmpdir(), "pledge-action-npm-"));
  try {
    const path = join(directory, ".npmrc");
    writeFileSync(path, ini.stringify(config), { mode: 0o600 });
    // pnpm/Yarn members may not be npm workspaces. Preserve their root config
    // as user configuration so it also reaches Pledge's temporary snapshots.
    const inherited = Object.fromEntries(Object.entries(env).filter(([key]) => key.toLowerCase() !== "npm_config_userconfig"));
    return { env: { ...inherited, npm_config_userconfig: path }, dispose: () => rmSync(directory, { recursive: true, force: true }) };
  } catch (error) {
    rmSync(directory, { recursive: true, force: true });
    throw error;
  }
}

function resolveBaseline(pkg, env) {
  const project = npm(["prefix"], pkg.directory, env);
  const response = JSON.parse(npm(["view", "--json", "--workspaces=false", "--", pkg.name, "versions"], project, env));
  const versions = typeof response === "string" ? [response] : response;
  if (!Array.isArray(versions) || versions.some(version => typeof version !== "string" || !semver.valid(version))) {
    throw new Error(`Registry returned invalid published versions for ${pkg.name}`);
  }
  return versions.filter(version => semver.major(version) === pkg.major && !semver.prerelease(version))
    .sort((left, right) => semver.rcompare(left, right) || left.localeCompare(right))[0] || null;
}

module.exports = { registryEnvironment, resolveBaseline };
