# CI integration

Use Pledge to check a built npm package before publishing. Run `pledge` after
your normal install and build steps, and use its exit code as the CI job result.
This guide covers source installation, public and private npm registries,
report retention, and recovery from failed or incomplete checks.

## Install and run

Install from a checkout using `cargo install --locked --path .`. To install a
specific revision directly, use:

```sh
cargo install --locked --git https://github.com/stentorian-io/pledge.git --rev <commit-sha> pledge
```

The public repository can be cloned without GitHub credentials. Pin an audited
commit for repeatable CI installation. The crate and binary are named `pledge`.
Install from source; this project does not currently publish an npm package.
Building requires Rust, a C/C++ toolchain, and CMake.

In the package checkout, with Node.js 22.12+ and npm on `PATH`:

```sh
npm ci
npm run build
pledge --against latest
```

Omit the build step only when the package already contains its publishable
files. Pledge runs `npm pack --ignore-scripts`, so `prepare` and `prepack`
hooks do not build missing outputs. The same applies when using Yarn, pnpm, or
Bun for the initial install/build; pledge still uses npm for acquisition and
analysis dependency installation. Pass a workspace member path, such as
`pledge ./packages/core --against latest`, and check each published member
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
managed cache with `pledge --clear-cache` when troubleshooting a damaged
installation.

Runtime, optional, and peer dependencies are installed into each temporary
snapshot with scripts and lockfile generation disabled. The source checkout is
not used as the installation target. Dependency versions follow the packed
manifest's ranges, so a pinned baseline alone does not freeze transitive
dependencies. Unpublished `workspace:` and relative `file:` dependencies must be
converted to registry-resolvable dependencies by your packaging process.
Install failures and missing compiler evidence produce incomplete analysis.

Dependency preparation resolves a separate manifest containing runtime,
optional, and peer requirements. Development dependencies and their peer
conflicts do not participate. The packed manifest and existing lockfiles remain
unchanged, and project npm configuration also applies to this installation.

Individual npm operations time out after 120 seconds; compiler evaluation is
bounded to 30 seconds per snapshot. Configure your CI job's overall timeout as
well. Cache Cargo build/install artifacts, npm's cache, and the platform's
`pledge` cache directory if needed. Dependency installation still needs
registry access when required packages are absent from npm's cache.

## Registries and credentials

Use the same npm configuration as your package's normal workflow. For example,
a project `.npmrc` can select a scoped registry and reference an injected token:

```ini
@your-scope:registry=https://registry.npmjs.org/
//registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}
```

Inject the token through your CI secret store. npm handles authentication, proxy,
and certificate settings. Pledge preserves the project's `.npmrc` in temporary
snapshots for dependency preparation; user/global npm configuration and environment
settings remain available. Use absolute paths for settings such as custom CA files,
since installation runs from a temporary directory. See npm's
[configuration documentation](https://docs.npmjs.com/cli/v11/configuring-npm/npmrc/).

An authentication or network failure during baseline acquisition exits 2.
Dependency preparation failures make analysis incomplete (exit 3). Registries
that hide private packages with a 404 can look like first publishes; use an
explicit `--against` in CI.

## Exit codes and output

Keep the command's exit status as the job result. Major findings exit `1`, tool
errors exit `2`, and incomplete analysis exits `3`. Do not append `|| true` to
the compatibility gate. See [reading results](results.md) for the complete
exit-code and severity reference.

To retain a machine-readable artifact:

```sh
pledge --against latest --format json > ../pledge-report.json
```

Place the report outside the analyzed package so `npm pack` does not include it.
Configure your CI system to retain available reports after a failed check while
preserving that failure. JSON output, stderr diagnostics, and the process exit
status are separate parts of the result. The [JSON reference](results.md#json-reports)
describes the report fields and early argument-parsing errors.

## Troubleshooting

| What happened | What to check |
| --- | --- |
| The selected baseline cannot be acquired (exit `2`) | Verify the package name, version or tag, and npm registry credentials from the original workspace |
| Dependency preparation failed (exit `3`) | Read the npm diagnostic; verify registry access and that packed dependency references are resolvable |
| Types are missing or compiler evaluation failed (exit `3`) | Build declarations, install the JavaScript TypeScript compiler, and check the reported unresolved imports |
| A source or analysis limit was reached (exit `3`) | Inspect the named file or contract and the relevant [rule's limits](rules/README.md); available findings are only a partial result |
| Files you expected to check are absent | Inspect the package's `files`, `.npmignore`, and generated outputs; Pledge checks the packed artifact |

For a damaged managed toolchain installation, use `pledge --clear-cache` and
rerun with registry access. Clearing the cache does not fix missing declarations
or unsupported syntax.

## Current verification boundary

CLI process checks run without external services. Opt-in integration checks use
real npm and the TypeScript compiler against a local authenticated registry,
including a workspace member, scoped dependencies, disabled lifecycle scripts,
and preservation of existing artifacts. The maintained development flow targets
Unix environments; Windows has not been validated as a release platform. See
[the test guide](../tests/README.md) for commands and
[the rule reference](rules/README.md) for analysis limitations.
