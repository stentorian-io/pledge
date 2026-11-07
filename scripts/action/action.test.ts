import { afterEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const entrypoint = resolve(import.meta.dir, "action.cjs");
const directories: string[] = [];
afterEach(() => {
  for (const directory of directories.splice(0)) rmSync(directory, { recursive: true, force: true });
});

function fixture(code = 0) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "pledge-action-")));
  directories.push(root);
  const workspace = join(root, "workspace");
  const bin = join(root, "bin");
  mkdirSync(workspace);
  mkdirSync(bin);
  writeFileSync(join(workspace, "package.json"), JSON.stringify({ name: "fixture", version: "1.1.0" }));
  const report = { schema_version: 1, status: ["clean", "breaking", "error", "incomplete"][code], exit_code: code,
    findings: [{ file: "index.js", line: 1, col: 1, severity: "major", rule: "export-removed", message: "<script>bad()</script>\n::error::untrusted" }],
    warnings: [{ message: "<b>missing evidence</b>" }] };
  const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, GITHUB_WORKSPACE: workspace,
    GITHUB_OUTPUT: join(root, "outputs"), GITHUB_STEP_SUMMARY: join(root, "summary"),
    PLEDGE_TEST_REPORT: JSON.stringify(report), PLEDGE_TEST_ARGS: join(root, "args") };
  writeFileSync(join(bin, "npm"), `#!/usr/bin/env node
console.log(process.argv[2] === 'prefix' ? process.cwd() : JSON.stringify(['2.0.0', '1.0.0', '1.2.0-beta.1']));
`, { mode: 0o755 });
  writeFileSync(join(bin, "pledge"), `#!/usr/bin/env node
require('node:fs').writeFileSync(process.env.PLEDGE_TEST_ARGS, JSON.stringify(process.argv.slice(2)));
console.error('::error::untrusted diagnostic');
console.log(process.env.PLEDGE_TEST_REPORT);
process.exitCode = ${code};
`, { mode: 0o755 });
  return { root, workspace, bin, env, run: (...args: string[]) => spawnSync("node", [entrypoint, ...args], { env, encoding: "utf8" }) };
}

test.each([0, 1, 2, 3])("the action preserves CLI status %i and reports safely", code => {
  const action = fixture(code);
  const result = action.run();
  expect(result.status).toBe(code);
  expect(readFileSync(action.env.GITHUB_OUTPUT, "utf8")).toBe(`status=${["clean", "breaking", "error", "incomplete"][code]}\nexit-code=${code}\n`);
  expect(JSON.parse(readFileSync(action.env.PLEDGE_TEST_ARGS, "utf8"))).toEqual(["--format", "json", "--against=1.0.0", action.workspace]);
  const summary = readFileSync(action.env.GITHUB_STEP_SUMMARY, "utf8");
  expect(summary).toContain("&lt;script&gt;");
  expect(summary).not.toContain("<script>");
  const lines = result.stdout.trim().split("\n");
  expect(lines[0]).toMatch(/^::stop-commands::[0-9a-f-]{36}$/);
  const token = lines[0].slice("::stop-commands::".length);
  expect(lines.at(-1)).toBe(`::${token}::`);
  expect(lines.slice(1, -1).join("\n")).toContain("::error::untrusted diagnostic");
  expect(result.stderr).toBe("");
});

test("package paths and baselines are passed literally without shell expansion", () => {
  const action = fixture();
  const path = "package with spaces; $(touch injected)";
  mkdirSync(join(action.workspace, path));
  writeFileSync(join(action.workspace, path, "package.json"), '{"name":"nested","version":"1.0.0"}');
  const baseline = "--clear-cache; $(touch injected)";
  expect(action.run(path, baseline).status).toBe(0);
  expect(JSON.parse(readFileSync(action.env.PLEDGE_TEST_ARGS, "utf8"))).toEqual(["--format", "json", `--against=${baseline}`, join(action.workspace, path)]);
});

test.each(["invalid-json", "wrong-code", "missing-cli", "signal"])("invalid CLI execution fails closed: %s", scenario => {
  const action = fixture();
  if (scenario === "invalid-json") action.env.PLEDGE_TEST_REPORT = "not JSON";
  if (scenario === "wrong-code") action.env.PLEDGE_TEST_REPORT = '{"schema_version":1,"status":"breaking","exit_code":1}';
  if (scenario === "missing-cli") writeFileSync(join(action.bin, "pledge"), "#!/nonexistent/interpreter\n", { mode: 0o755 });
  if (scenario === "signal") writeFileSync(join(action.bin, "pledge"), '#!/usr/bin/env node\nprocess.kill(process.pid, "SIGTERM");\n', { mode: 0o755 });
  expect(action.run().status).toBe(2);
  expect(readFileSync(action.env.GITHUB_OUTPUT, "utf8")).toBe("status=error\nexit-code=2\n");
});

test("missing or external package paths fail before invoking Pledge", () => {
  const action = fixture();
  symlinkSync(action.root, join(action.workspace, "outside"));
  for (const path of ["missing", "..", "outside"]) expect(action.run(path).status).toBe(2);
  expect(() => readFileSync(action.env.PLEDGE_TEST_ARGS)).toThrow();
});

test("large findings retain the full log while bounding the job summary", () => {
  const action = fixture(1);
  const report = JSON.parse(action.env.PLEDGE_TEST_REPORT);
  report.findings[0].message = "<".repeat(60_000) + "end of finding";
  action.env.PLEDGE_TEST_REPORT = JSON.stringify(report);
  const result = action.run();
  expect(result.status).toBe(1);
  expect(result.stdout).toContain("end of finding");
  const summary = readFileSync(action.env.GITHUB_STEP_SUMMARY, "utf8");
  expect(Buffer.byteLength(summary)).toBeLessThan(201_000);
  expect(summary).toContain("See the step log for all findings");
});

test.each([0, 1, 2, 3])("0.x permits only complete breaking comparisons: CLI status %i", code => {
  const action = fixture(code);
  writeFileSync(join(action.workspace, "package.json"), '{"name":"fixture","version":"0.4.8"}');
  const result = action.run("", "0.5.2");
  expect(result.status).toBe(code === 1 ? 0 : code);
  const status = ["clean", "advisory", "error", "incomplete"][code];
  expect(readFileSync(action.env.GITHUB_OUTPUT, "utf8")).toContain(`status=${status}\n`);
  if (code === 1) {
    expect(result.stdout).toContain("export-removed");
    expect(result.stdout).toContain("Pledge: breaking (exit 1)");
    expect(result.stdout).toContain("Advisory:");
  }
});

test("a first stable release cannot use the 0.x exemption", () => {
  const action = fixture(1);
  writeFileSync(join(action.workspace, "package.json"), '{"name":"fixture","version":"1.0.0"}');
  expect(action.run("", "0.9.0").status).toBe(1);
});

test("a missing same-major baseline is incomplete and does not invoke the comparator", () => {
  const action = fixture();
  writeFileSync(join(action.workspace, "package.json"), '{"name":"fixture","version":"3.0.0"}');
  expect(action.run().status).toBe(3);
  expect(readFileSync(action.env.GITHUB_OUTPUT, "utf8")).toBe("status=incomplete\nexit-code=3\n");
  expect(() => readFileSync(action.env.PLEDGE_TEST_ARGS)).toThrow();
});

test.each([
  [1, 0, 1, "breaking"],
  [1, 1, 1, "breaking"],
  [0, 1, 0, "advisory"],
  [3, 1, 3, "incomplete"],
  [1, 2, 2, "error"],
  [0, 0, 0, "clean"],
])("all workspace members run and aggregate stable %i / early %i", (stable, early, code, status) => {
  const action = fixture();
  writeFileSync(join(action.workspace, "package.json"), '{"private":true,"workspaces":["packages/*"]}');
  for (const [name, version, code] of [["a-stable", "1.2.0", stable], ["b-early", "0.4.0", early]]) {
    const directory = join(action.workspace, "packages", String(name));
    mkdirSync(directory, { recursive: true });
    writeFileSync(join(directory, "package.json"), JSON.stringify({ name, version, code }));
  }
  writeFileSync(join(action.bin, "pledge"), `#!/usr/bin/env node
const fs = require('node:fs');
const manifest = JSON.parse(fs.readFileSync('package.json', 'utf8'));
fs.appendFileSync(process.env.PLEDGE_TEST_ARGS, manifest.name + '\\n');
console.log(JSON.stringify({schema_version:1,status:['clean','breaking','error','incomplete'][manifest.code],exit_code:manifest.code}));
process.exitCode=manifest.code;
`, { mode: 0o755 });
  expect(action.run("", "latest").status).toBe(code);
  expect(readFileSync(action.env.PLEDGE_TEST_ARGS, "utf8")).toBe("a-stable\nb-early\n");
  expect(readFileSync(action.env.GITHUB_OUTPUT, "utf8")).toBe(`status=${status}\nexit-code=${code}\n`);
});
