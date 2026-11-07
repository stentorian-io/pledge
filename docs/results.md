# Reading results

Use the exit code to decide whether a CI check passed, then use the findings and
warnings to understand why. A finding describes a change in the public contract;
an incomplete result identifies evidence the analyzer could not obtain.

## Exit codes

These are standalone CLI exit codes. The [GitHub Action](github-action.md#read-the-result)
aggregates package results and treats complete breaking comparisons for local
`0.x` packages as advisory, while keeping errors and incomplete analysis blocking.

| Exit | Meaning | Next step |
| --- | --- | --- |
| `0` | Complete analysis without major findings, or first publish without a baseline | Review warnings and the comparison's scope |
| `1` | Complete analysis with major findings | Review the breaking changes and update the package or migration plan |
| `2` | Invalid invocation, acquisition, configuration, or I/O failure | Correct the reported setup problem and rerun |
| `3` | Incomplete evidence, even when major findings are present | Resolve the missing evidence and rerun |

Exit `3` takes precedence over `1`. Findings from completed analysis remain in
the report, but an incomplete run cannot establish compatibility. First-publish
analysis has no previous release to compare; its `baseline_version` is null.

A clean result means no breaking changes were found within the supported static
checks. It does not prove arbitrary runtime behavior is unchanged. Increasing
the package's major version does not change the check's result.

## Finding severities

Each finding includes a stable rule ID, severity, file, position, and message.
Locations refer to files in the packed package, which may be generated outputs
rather than original source files.

| Severity | Meaning |
| --- | --- |
| Major | A breaking public contract change |
| Minor | A compatible addition or change worth reviewing |
| Patch | A cosmetic or non-observable change |
| Warning | An observable change that does not independently fail the check, such as deprecation or an added union variant |
| Info | Context or a documented analysis limitation |

Only major findings independently fail the check. Incomplete analysis is a run
status, not a severity. The [rule reference](rules/README.md) describes the exact
trigger and limits of each documented rule.

## Incomplete analysis and limitations

Missing compiler evidence, unresolved imports, unreadable public sources,
unsupported syntax, and processing limits can make a run incomplete. Read the
warnings for the affected baseline or current package, then follow the
[CI troubleshooting guide](ci.md#troubleshooting).

Some rules also emit informational findings that explain a particular limit:

| Emitted rule ID | What it explains |
| --- | --- |
| `limitation: lsp-required` | Type resolution was unavailable for a `typeof import` reference; the ID retains its historical name |
| `limitation: satisfies-narrowed-type-not-verified` | A `satisfies` removal lacks resolved narrowed-type evidence |
| `limitation: dynamic-static-member-key` | A class key or static assignment depends on unresolved runtime evidence; accompanied by `analysis-incomplete` |
| `limitation: proxy-dispatched-exports` | Proxy or reflective dispatch remains unresolved; accompanied by `analysis-incomplete` |
| `limitation: decorator-runtime-effect-opaque` | A decorator changed, but its arbitrary runtime effects are not proven |
| `limitation: private-brand-access-not-checkable` | A public/private transition was detected, but consumer reliance on private-brand checks is unknown |

An informational finding alone does not determine the exit code. Read the run
status and warnings alongside it; limits on what a rule proves are distinct
from a failure to finish the analysis.

## Text reports

Text is the default. A comparison report names the package and both versions,
then lists findings and a severity summary. First-publish reports identify the
missing baseline explicitly.

Redirected output has no automatic ANSI colors or progress spinner.
`--color always|never|auto` controls text colors; `NO_COLOR` disables automatic
color. Text warnings and tracing diagnostics go to stderr.

## JSON reports

Write the report outside the analyzed package so packing does not include it:

```sh
pledge --against latest --format json > ../pledge-report.json
```

JSON mode writes one document to stdout without ANSI escapes or progress
output. Tracing diagnostics go to stderr. Keep the process exit status when
saving or uploading reports.

This example shows the shape of a complete report without findings:

```json
{
  "schema_version": 1,
  "status": "clean",
  "exit_code": 0,
  "package": {
    "name": "example",
    "current_version": "1.2.0",
    "baseline_version": "1.1.0"
  },
  "summary": {"major": 0, "minor": 0, "patch": 0, "warning": 0, "info": 0},
  "findings": [],
  "warnings": []
}
```

Analysis status is `clean`, `breaking`, or `incomplete`. Findings contain
`severity`, `rule`, `file`, `line`, `col`, and `message`. Warnings contain `message`
and `incomplete`; in comparisons, baseline/current prefixes identify the affected
snapshot.
Finding order is deterministic by severity, file, location, rule, and message.

Operational errors use `schema_version: 1`, `status: "error"`, `exit_code: 2`, and
an `error` string. Successful cache clearing uses `status: "cache-cleared"` and
`exit_code: 0`. Argument parsing errors, help, and version output retain the
usual CLI text format, even if `--format json` is present.
