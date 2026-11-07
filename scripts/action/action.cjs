const { spawnSync } = require("node:child_process");
const { randomUUID } = require("node:crypto");
const { appendFileSync, realpathSync } = require("node:fs");
const { resolve, relative, isAbsolute } = require("node:path");

const statuses = ["clean", "breaking", "error", "incomplete"];
const escapeHtml = text => String(text).replace(/[&<>"']/g, character => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[character]);

function compare(path, against, env) {
  const workspace = realpathSync(env.GITHUB_WORKSPACE || process.cwd());
  const directory = realpathSync(resolve(workspace, path || "."));
  const within = relative(workspace, directory);
  if (within === ".." || within.startsWith("../") || isAbsolute(within)) {
    throw new Error("The package path must be inside the checked-out workspace");
  }
  const result = spawnSync("pledge", ["--format", "json", `--against=${against || "latest"}`, directory], {
    cwd: directory, env, encoding: "utf8", maxBuffer: 32 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.signal) throw new Error(`Pledge terminated by ${result.signal}`);
  let report;
  try {
    report = JSON.parse(result.stdout);
  } catch {
    throw new Error(`Pledge did not return JSON. ${result.stderr || ""}`);
  }
  if (report.schema_version !== 1 || report.exit_code !== result.status
    || statuses[result.status] !== report.status) throw new Error("Pledge returned an invalid report or exit status");
  return { report, diagnostics: result.stderr };
}

function printLog(message) {
  // Use one stream so runner commands stay disabled around all package-controlled text.
  const token = randomUUID();
  const lines = message.split(/[\r\n]+/).map(line => `pledge: ${line}`).join("\n");
  console.log(`::stop-commands::${token}\n${lines}\n::${token}::`);
}

function describe(report) {
  const lines = [`Pledge: ${report.status} (exit ${report.exit_code})`];
  if (report.package) lines.push(`${report.package.name}: ${report.package.baseline_version} → ${report.package.current_version}`);
  if (report.error) lines.push(report.error);
  if (report.status === "incomplete") lines.push("Analysis is incomplete; compatibility could not be established.");
  for (const finding of report.findings || []) {
    lines.push(`${finding.file}:${finding.line}:${finding.col} [${finding.severity}] ${finding.rule}: ${finding.message}`);
  }
  for (const warning of report.warnings || []) lines.push(`warning: ${warning.message}`);
  return lines.join("\n");
}

function runAction(path = ".", against = "latest", env = process.env) {
  let report;
  let diagnostics = "";
  try {
    ({ report, diagnostics } = compare(path, against, env));
  } catch (error) {
    report = { schema_version: 1, status: "error", exit_code: 2, error: error.message };
  }
  const description = describe(report);
  printLog([diagnostics, description].filter(Boolean).join("\n"));
  if (env.GITHUB_OUTPUT) appendFileSync(env.GITHUB_OUTPUT, `status=${report.status}\nexit-code=${report.exit_code}\n`);
  if (env.GITHUB_STEP_SUMMARY) {
    const summary = escapeHtml(description);
    const displayed = summary.length > 200_000 ? `${summary.slice(0, 200_000)}\n… See the step log for all findings.` : summary;
    appendFileSync(env.GITHUB_STEP_SUMMARY, `### Pledge package compatibility\n\n<pre>${displayed}</pre>\n`);
  }
  return report.exit_code;
}

if (require.main === module) {
  try {
    process.exitCode = runAction(process.argv[2], process.argv[3]);
  } catch (error) {
    printLog(`Action failed: ${error.message}`);
    process.exitCode = 2;
  }
}
