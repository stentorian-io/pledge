const assert = require("node:assert/strict");
const { execFileSync, spawn, spawnSync } = require("node:child_process");
const { createServer } = require("node:http");
const { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join } = require("node:path");

async function smokeWorkspaces(entrypoint) {
  const root = mkdtempSync(join(tmpdir(), "pledge-workspaces-"));
  const workspace = join(root, "checkout");
  mkdirSync(workspace);
  const manifests = new Map(), tarballs = new Map(), requested = new Set();
  let address, denied = false;
  const server = createServer((request, response) => {
    if (request.headers.authorization !== "Bearer smoke-token") {
      response.writeHead(401); response.end('{"error":"authentication required"}'); return;
    }
    const route = decodeURIComponent(request.url.split("?", 1)[0]);
    if (tarballs.has(route)) { response.end(tarballs.get(route)); return; }
    const name = route.slice(1);
    requested.add(name);
    if (denied && name === "@pledge-smoke/stable") {
      response.writeHead(403); response.end('{"error":"denied"}'); return;
    }
    const manifest = manifests.get(name);
    if (!manifest) { response.writeHead(404); response.end('{"error":"not found"}'); return; }
    response.setHeader("Content-Type", "application/json");
    response.end(JSON.stringify(manifest));
  });
  const consumer = name => ["--input-type=module", "--eval", `import {feature} from '${name}'; if (feature !== 1) process.exit(1);`];
  try {
    await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
    address = `http://127.0.0.1:${server.address().port}`;
    for (const [label, versions, latest] of [
      ["stable", ["2.9.0", "2.10.0", "2.11.0-beta.1", "3.0.0"], "3.0.0"],
      ["early", ["0.4.7", "0.5.2", "0.6.0-beta.1", "1.0.0"], "1.0.0"],
    ]) {
      const name = `@pledge-smoke/${label}`;
      const metadata = { name, "dist-tags": { latest }, versions: {} };
      for (const version of versions) {
        const directory = join(root, `${label}-${version}`);
        const packed = join(directory, "package");
        mkdirSync(packed, { recursive: true });
        const manifest = { name, version, type: "module", exports: "./index.js" };
        writeFileSync(join(packed, "package.json"), JSON.stringify(manifest));
        writeFileSync(join(packed, "index.js"), "export const feature = 1;\n");
        execFileSync("node", consumer(name), { cwd: packed });
        const archive = join(directory, "package.tgz");
        execFileSync("tar", ["-czf", archive, "-C", directory, "package"], { env: { ...process.env, COPYFILE_DISABLE: "1" } });
        const route = `/tarballs/${label}-${version}.tgz`;
        tarballs.set(route, readFileSync(archive));
        metadata.versions[version] = { ...manifest, dist: { tarball: `${address}${route}` } };
      }
      manifests.set(name, metadata);
    }
    const patterns = ["packages/*", "!packages/excluded"];
    const configure = kind => {
      writeFileSync(join(workspace, "package.json"), JSON.stringify({ private: true,
        ...(kind === "npm" ? { workspaces: patterns } : {}) }));
      if (kind === "pnpm") writeFileSync(join(workspace, "pnpm-workspace.yaml"), "packages:\n  - 'packages/*'\n  - '!packages/excluded'\n");
    };
    configure("npm");
    const projectConfig = `@pledge-smoke:registry=${address}/\ncache=${join(root, "cache")}\nfetch-retries=0\nupdate-notifier=false\n`;
    const userConfig = `//127.0.0.1:${server.address().port}/:_authToken=\${PLEDGE_SMOKE_TOKEN}\n`;
    const userConfigPath = join(root, "user.npmrc");
    writeFileSync(join(workspace, ".npmrc"), projectConfig);
    writeFileSync(userConfigPath, userConfig);
    function local(label, version, source, extra = {}) {
      const directory = join(workspace, "packages", label);
      mkdirSync(directory, { recursive: true });
      writeFileSync(join(directory, "package.json"), JSON.stringify({ name: `@pledge-smoke/${label}`, version,
        type: "module", exports: "./index.js", scripts: { prepack: "touch PACK_SCRIPT_EXECUTED" }, ...extra }));
      writeFileSync(join(directory, "index.js"), source);
      return directory;
    }
    local("early", "0.4.8", "export const replacement = 1;\n");
    local("private", "invalid-version", "", { private: true });
    local("excluded", "1.0.0", "");
    let sequence = 0;
    async function run(expected, status, args = []) {
      const output = join(root, `outputs-${sequence}`), summary = join(root, `summary-${sequence++}`);
      const result = await new Promise((resolve, reject) => {
        const child = spawn("node", [entrypoint, ...args], { cwd: workspace, timeout: 180_000,
          env: { ...process.env, GITHUB_WORKSPACE: workspace, GITHUB_OUTPUT: output, GITHUB_STEP_SUMMARY: summary,
            npm_config_userconfig: userConfigPath, PLEDGE_SMOKE_TOKEN: "smoke-token" } });
        let stdout = "", stderr = "";
        child.stdout.on("data", chunk => stdout += chunk);
        child.stderr.on("data", chunk => stderr += chunk);
        child.on("error", reject);
        child.on("close", code => resolve({ code, stdout, stderr }));
      });
      assert.equal(result.code, expected, `${result.stderr}\n${result.stdout}`);
      assert.equal(readFileSync(output, "utf8"), `status=${status}\nexit-code=${expected}\n`);
      assert.match(readFileSync(summary, "utf8"), new RegExp(`Pledge action: ${status}`));
      assert.equal(readFileSync(join(workspace, ".npmrc"), "utf8"), projectConfig);
      assert.equal(readFileSync(userConfigPath, "utf8"), userConfig);
      for (const label of ["stable", "early"]) assert.equal(existsSync(join(workspace, "packages", label, "PACK_SCRIPT_EXECUTED")), false);
      return result.stdout;
    }
    const stable = local("stable", "2.8.0", "export const feature = 1;\n");
    assert.equal(spawnSync("node", consumer("@pledge-smoke/stable"), { cwd: stable }).status, 0);
    assert.equal(spawnSync("node", consumer("@pledge-smoke/early"), { cwd: join(workspace, "packages/early") }).status, 1);
    const advisory = await run(0, "advisory");
    assert.match(advisory, /2\.10\.0 → 2\.8\.0/);
    assert.match(advisory, /0\.5\.2 → 0\.4\.8/);
    assert.match(advisory, /export-removed/);
    local("stable", "2.8.0", "export const replacement = 1;\n");
    assert.equal(spawnSync("node", consumer("@pledge-smoke/stable"), { cwd: stable }).status, 1);
    const breaking = await run(1, "breaking");
    assert.match(breaking, /Advisory:/);
    local("stable", "2.8.0", "export const feature = 1;\n", { exports: { types: "./missing.d.ts", default: "./index.js" } });
    await run(3, "incomplete");
    local("stable", "4.0.0", "export const feature = 1;\n");
    assert.match(await run(3, "incomplete"), /No baseline:/);
    local("stable", "2.8.0", "export const feature = 1;\n");
    denied = true;
    const failed = await run(2, "error");
    assert.match(failed, /Advisory:/);
    denied = false;
    configure("pnpm");
    assert.match(await run(0, "advisory"), /2\.10\.0 → 2\.8\.0/);
    assert.match(await run(0, "advisory", ["packages/early", "0.4.7"]), /0\.4\.7 → 0\.4\.8/);
    assert.deepEqual([...requested].sort(), ["@pledge-smoke/early", "@pledge-smoke/stable"]);
    console.log("Workspace smoke passed: authenticated npm/pnpm discovery, same-major baselines, 0.x advisories, and mixed failures");
  } finally {
    await new Promise(resolve => server.close(resolve));
    rmSync(root, { recursive: true, force: true });
  }
}

if (require.main === module) smokeWorkspaces(process.argv[2]).catch(error => { console.error(error); process.exitCode = 1; });
module.exports = { smokeWorkspaces };
