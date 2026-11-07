const assert = require("node:assert/strict");
const { spawn, spawnSync, execFileSync } = require("node:child_process");
const { createServer } = require("node:http");
const { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join } = require("node:path");

async function smokeImage(version) {
  assert.match(version, /^\d+\.\d+\.\d+$/);
  assert.equal(execFileSync("pledge", ["--version"], { encoding: "utf8" }).trim(), `pledge ${version}`);
  assert.equal(process.arch, "x64");
  assert.equal(process.platform, "linux");
  assert.equal(execFileSync("npm", ["--version"], { encoding: "utf8" }).trim(), "11.19.0");
  assert.equal(execFileSync("tsc", ["--version"], { encoding: "utf8" }).trim(), "Version 5.9.3");
  const root = mkdtempSync(join(tmpdir(), "pledge-image-"));
  const old = join(root, "package");
  const current = join(root, "current");
  mkdirSync(old);
  mkdirSync(current);
  const manifest = { name: "pledge-smoke-fixture", version: "1.0.0", type: "module", exports: "./index.js" };
  writeFileSync(join(old, "package.json"), JSON.stringify(manifest));
  writeFileSync(join(old, "index.js"), "export const feature = 1;\n");
  const consumer = ["--input-type=module", "--eval", "import {feature} from 'pledge-smoke-fixture'; if (feature !== 1) process.exit(1);"];
  execFileSync("node", consumer, { cwd: old });
  const archive = join(root, "old.tgz");
  execFileSync("tar", ["-czf", archive, "-C", root, "package"]);
  const tarball = readFileSync(archive);
  let address;
  const server = createServer((request, response) => {
    if (request.url === "/old.tgz") {
      response.writeHead(200, { "Content-Type": "application/octet-stream" });
      response.end(tarball);
    } else if (request.url?.startsWith("/pledge-smoke-fixture")) {
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ name: manifest.name, "dist-tags": { latest: "1.0.0" },
        versions: { "1.0.0": { ...manifest, dist: { tarball: `${address}/old.tgz` } } } }));
    } else {
      response.writeHead(404);
      response.end();
    }
  });
  try {
    await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
    address = `http://127.0.0.1:${server.address().port}`;
    writeFileSync(join(current, "package.json"), JSON.stringify({ ...manifest, version: "1.0.1" }));
    writeFileSync(join(current, ".npmrc"), `registry=${address}/\ncache=${join(root, "cache")}\nfetch-retries=0\nupdate-notifier=false\n`);
    for (const [source, expected] of [["export const feature = 1;\n", 0], ["export const replacement = 1;\n", 1]]) {
      writeFileSync(join(current, "index.js"), source);
      const consumed = spawnSync("node", consumer, { cwd: current, encoding: "utf8" });
      assert.equal(consumed.status, expected, consumed.stderr);
      const result = await new Promise((resolve, reject) => {
        const child = spawn("pledge", [".", "--against", "latest", "--format", "json"], {
          cwd: current, env: { ...process.env, npm_config_userconfig: join(root, "empty.npmrc") },
          timeout: 120_000,
        });
        let stdout = "", stderr = "";
        child.stdout.on("data", chunk => stdout += chunk);
        child.stderr.on("data", chunk => stderr += chunk);
        child.on("error", reject);
        child.on("close", code => resolve({ code, stdout, stderr }));
      });
      assert.equal(result.code, expected, `${result.stderr}\n${result.stdout}`);
      const report = JSON.parse(result.stdout);
      assert.equal(report.exit_code, expected);
      assert.equal(report.package.baseline_version, "1.0.0");
      assert.equal(report.findings.some(finding => finding.rule === "export-removed"), expected === 1);
    }
    console.log(`Image smoke test passed: Pledge ${version}, compatible and breaking consumers`);
  } finally {
    await new Promise(resolve => server.close(resolve));
    rmSync(root, { recursive: true, force: true });
  }
}

if (require.main === module) smokeImage(process.argv[2]).catch(error => { console.error(error); process.exitCode = 1; });
module.exports = { smokeImage };
