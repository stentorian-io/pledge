const { spawnSync } = require("node:child_process");
const { randomUUID } = require("node:crypto");
const { appendFileSync, realpathSync } = require("node:fs");
const { discoverPackages } = require("./packages.cjs");
const { registryEnvironment, resolveBaseline } = require("./registry.cjs");

const statuses = ["clean", "breaking", "error", "incomplete"];
const escapeHtml = text => String(text).replace(/[&<>"']/g, character => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[character]);

function compare(pkg, against, env) {
  const result = spawnSync("pledge", ["--format", "json", `--against=${against}`, pkg.directory], {
    cwd: pkg.directory, env, encoding: "utf8", maxBuffer: 32 * 1024 * 1024,
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

function runAction(path = "", against = "", env = process.env) {
  const results = [], descriptions = [];
  let registry;
  try {
    const workspace = realpathSync(env.GITHUB_WORKSPACE || process.cwd());
    const { packages, skipped } = discoverPackages(workspace, path);
    descriptions.push(...skipped.map(reason => `Skipped ${reason}`));
    registry = registryEnvironment(workspace, env);
    for (const pkg of packages) {
      descriptions.push(`Package ${pkg.name}@${pkg.version} (${pkg.path})`);
      try {
        const baseline = against || resolveBaseline(pkg, registry.env);
        if (!baseline) {
          results.push("incomplete");
          descriptions.push(`No baseline: no published version without a prerelease suffix in major ${pkg.major}; compatibility could not be established.`);
          continue;
        }
        descriptions.push(`Baseline: ${baseline} (${against ? "explicit selector" : `highest published version without a prerelease suffix in major ${pkg.major}`})`);
        const { report, diagnostics } = compare(pkg, baseline, registry.env);
        if (diagnostics) descriptions.push(diagnostics);
        descriptions.push(describe(report));
        if (pkg.major === 0 && report.status === "breaking") {
          results.push("advisory");
          descriptions.push("Advisory: breaking findings do not block while this package remains on 0.x.");
        } else results.push(report.status);
      } catch (error) {
        results.push("error");
        descriptions.push(`Error: ${error.message}`);
      }
    }
  } catch (error) {
    results.push("error");
    descriptions.push(`Error: ${error.message}`);
  } finally {
    registry?.dispose();
  }
  const status = ["error", "incomplete", "breaking", "advisory", "clean"].find(status => results.includes(status));
  const exitCode = { error: 2, incomplete: 3, breaking: 1, advisory: 0, clean: 0 }[status];
  const description = [`Pledge action: ${status} (exit ${exitCode})`, ...descriptions].join("\n\n");
  printLog(description);
  if (env.GITHUB_OUTPUT) appendFileSync(env.GITHUB_OUTPUT, `status=${status}\nexit-code=${exitCode}\n`);
  if (env.GITHUB_STEP_SUMMARY) {
    const summary = escapeHtml(description);
    const displayed = summary.length > 200_000 ? `${summary.slice(0, 200_000)}\n… See the step log for all findings.` : summary;
    appendFileSync(env.GITHUB_STEP_SUMMARY, `### Pledge package compatibility\n\n<pre>${displayed}</pre>\n`);
  }
  return exitCode;
}

if (require.main === module) {
  try {
    process.exitCode = runAction(process.argv[2], process.argv[3]);
  } catch (error) {
    printLog(`Action failed: ${error.message}`);
    process.exitCode = 2;
  }
}
