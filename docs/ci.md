# CI integration

Interlint is a command-line check for one built npm package. Run it after your
normal install and build steps, before publishing. It supports public and
private npm registries through npm's own configuration. No hosted CI workflow
is installed by this repository.

## Install and run

Install from a checkout using `cargo install --locked --path .`. To install a
specific revision directly, use:

```sh
cargo install --locked --git https://github.com/stentorian-io/pledge.git --rev <commit-sha> interlint
```

The public repository can be cloned without GitHub credentials. Pin an audited
commit for repeatable CI installation. The crate and binary are named
`interlint`; an npm package with that name is not this project's
installation method. Building requires Rust, a C/C++ toolchain, and CMake.

In the package checkout, with Node.js 22.12+ and npm on `PATH`:

```sh
npm ci
npm run build
interlint --against latest
```

Omit the build step only when the package already contains its publishable
files. Interlint runs `npm pack --ignore-scripts`, so `prepare` and `prepack`
hooks do not build missing outputs. The same applies when using Yarn, pnpm, or
Bun for the initial install/build; interlint still uses npm for acquisition and
analysis dependency installation. Pass a workspace member path, such as
`interlint ./packages/core --against latest`, and check each published member
separately.

An exact version (`--against 1.4.2`) pins the baseline. A dist-tag (`--against
latest` or `--against next`) resolves to a version at the start of each run, and
the report records that resolved version. Version ranges, URLs, and local paths
are not accepted as baselines. An explicit baseline that cannot be found is an
error. Without this flag, a 404 permits first-publish analysis and the report's
`baseline_version` is null; it does not establish compatibility with a release.

## TypeScript and dependency preparation

The compiler is resolved from the analyzed package's original workspace, then
`PATH`, then a managed cache/download. Both snapshots use that compiler. Installing
`typescript@5.9.3` as a dev dependency or globally avoids a toolchain download;
normal CLI analysis does not require `typescript-language-server`. Clear the
managed cache with `interlint --clear-cache` when troubleshooting a damaged
installation.

Runtime, optional, and peer dependencies are installed into each temporary
snapshot with scripts and lockfile generation disabled. The source checkout is
not used as the installation target. Dependency versions follow the packed
manifest's ranges, so a pinned baseline alone does not freeze transitive
dependencies. Unpublished `workspace:` and relative `file:` dependencies must be
converted to registry-resolvable dependencies by your packaging process.
Install failures and missing compiler evidence produce incomplete analysis.

Individual npm operations time out after 120 seconds; compiler evaluation is
bounded to 30 seconds per snapshot. Configure your CI job's overall timeout as
well. Cache Cargo build/install artifacts, npm's cache, and the platform's
`interlint` cache directory if needed. Dependency installation still needs
registry access when required packages are absent from npm's cache.

## Registries and credentials

Use the same npm configuration as your package's normal workflow. For example,
a project `.npmrc` can select a scoped registry and reference an injected token:

```ini
@your-scope:registry=https://registry.npmjs.org/
//registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}
```

Inject the token through your CI secret store. npm handles authentication, proxy,
and certificate settings. Interlint preserves the project's `.npmrc` in temporary
snapshots for dependency preparation; user/global npm configuration and environment
settings remain available. Use absolute paths for settings such as custom CA files,
since installation runs from a temporary directory. See npm's
[configuration documentation](https://docs.npmjs.com/cli/v11/configuring-npm/npmrc/).

An authentication/network failure exits 2. Registries that hide private packages
with a 404 can look like first publishes; use an explicit `--against` in CI.

## Exit codes and output

| Exit | Meaning | CI action |
| --- | --- | --- |
| `0` | Complete analysis with no major findings, or first publish without a baseline | Pass; review warnings as needed |
| `1` | Major findings | Review breaking changes |
| `2` | Invalid invocation, acquisition, configuration, or I/O failure | Fix the job/package setup and rerun |
| `3` | Incomplete evidence, even if major findings are also present | Resolve the reported failures and rerun |

Warnings, minor, patch, and informational findings do not independently fail the
check. Exit 3 takes precedence over exit 1; never turn an incomplete result into
a compatibility pass. The check does not excuse breaking findings just because
you incremented the package's major version.

Text is the default. Redirected output has no automatic ANSI colors or spinner;
`--color always|never|auto` controls text colors. `NO_COLOR` disables automatic
color. Findings use paths relative to the packed package, which may point to
built outputs rather than original source files.

For a machine-readable artifact, keep the report outside the analyzed package
so `npm pack` does not include it:

```sh
interlint --against latest --format json > ../interlint-report.json
```

Keep the command's exit status when uploading reports; do not append `|| true`
to the compatibility gate. JSON mode writes one document to stdout, with no
ANSI escapes or progress output. Diagnostics from tracing go to stderr.

The analysis report has these fields:

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
and `incomplete`; baseline/current prefixes identify the affected snapshot.
Finding order is deterministic by severity, file, location, rule, and message.

Operational errors use `schema_version: 1`, `status: "error"`, `exit_code: 2`, and
an `error` string. Successful cache clearing uses `status: "cache-cleared"` and
`exit_code: 0`. Argument parsing errors, help, and version output retain the
usual CLI text format, even if `--format json` is present.

## Current verification boundary

CLI process checks run without external services. Opt-in integration checks use
real npm and the TypeScript compiler against a local authenticated registry,
including a workspace member, scoped dependencies, disabled lifecycle scripts,
and preservation of existing artifacts. The maintained development flow targets
Unix environments; Windows has not been validated as a release platform. See
[the test guide](../tests/README.md) for commands and
[the rule reference](rules/README.md) for analysis limitations.
