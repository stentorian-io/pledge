# Pledge

Catch breaking changes in JavaScript and TypeScript packages before publishing.

Pledge compares the package you are about to publish with a version from your
npm registry. It reports changes to public exports, types, package requirements,
and distributed files, and returns an exit code you can use in CI.
The command and Cargo package are named `pledge`.

[Quick start](#quick-start) · [Example](#a-change-that-breaks-a-consumer) ·
[What Pledge checks](#what-pledge-checks) · [CI integration](docs/ci.md) ·
[Documentation](docs/README.md)

## Quick start

### Install from source

You need Rust, a C/C++ compiler, and CMake to build the tool. Analysis needs
Node.js 22.12 or newer and npm.

```sh
git clone https://github.com/stentorian-io/pledge.git
cd pledge
cargo install --locked --path .
pledge --version
```

Cargo installs `pledge` into `~/.cargo/bin`; keep that directory on `PATH`.
Source installation is the current distribution method. An npm wrapper and
prebuilt binaries are not published yet. Run `pledge --help` for the available
options.

### Check your package

In an existing, published npm package's checkout, install dependencies and build
its publishable JavaScript and declarations, then run:

```sh
npm ci
npm run build                 # omit if your package needs no build
pledge --against latest
```

The report names the compared versions and lists findings with their rule IDs
and source locations. A complete check exits `0` when no major findings are
found, or `1` when it finds a breaking change. Tool errors exit `2`; incomplete
analysis exits `3`, even when breaking findings are also present.

Build first: Pledge inspects the files selected by `npm pack` and disables
packing and dependency lifecycle scripts. Type analysis uses the JavaScript
TypeScript compiler from your project, `PATH`, or a managed download. See
[compiler setup](docs/ci.md#typescript-and-dependency-preparation) for CI.

For a first publish, omit `--against`. A registry 404 then permits analysis
without a baseline; the report lists new exports. Keep `--against latest` in
an existing package's CI so an unavailable baseline fails the check.

## A change that breaks a consumer

Suppose your published declaration accepts a string or a number:

```typescript
export declare function parseId(value: string | number): string;
```

The next version accepts only strings:

```typescript
export declare function parseId(value: string): string;
```

An existing call to `parseId(42)` no longer type-checks. Pledge reports a major
`param-type-narrowed` finding for `parseId`. With complete analysis,
the command exits `1`, so the CI check fails. Increasing the package's major
version does not suppress that finding.

[Reading results](docs/results.md) explains severities, exit codes, and the
difference between a compatibility finding and missing evidence.

## What Pledge checks

| Change | Examples | Details |
| --- | --- | --- |
| Public exports | Removed names, changed subpaths, and conditional entry points | [Export resolution](docs/rules/export-resolution.md) |
| Type contracts | Narrowed inputs, changed return types, and referenced declarations | [Public types](docs/rules/reachable-types.md) |
| Class contracts | Removed accessors, changed write types, and visibility transitions | [Accessors](docs/rules/accessor-contracts.md), [visibility](docs/rules/class-field-visibility-transition.md) |
| Package requirements | Narrower dependency ranges and reduced platform support | [Package requirements](docs/rules/package-requirements.md) |
| Runtime loading | Lost CommonJS named imports and changes that prevent synchronous loading | [Module loading](docs/rules/module-loading.md) |

The [rule reference](docs/rules/README.md) covers individual findings and their
limits. Analysis is static: a clean result means no breaking change was found
within the supported checks. It does not prove every consumer or runtime
behavior remains compatible. Missing inputs or exhausted analysis limits
produce an incomplete result.

The [language coverage inventory](docs/language-coverage.md) maps pinned language
and host references to empirical tests, partial support, and documented coverage
gaps. It reports tested runtime/compiler profiles separately from line coverage.

## Use it in CI

Select one package, choose a baseline, and keep the command's exit status:

```sh
pledge ./packages/core --against 1.4.2
pledge --against latest --format json > ../pledge-report.json
```

Each command is a separate example. The first checks a workspace member against
an exact version; the second saves a JSON report for the current package. Keep
reports outside the package so packing does not include them.

Pledge uses npm's registry configuration, including scoped registries and
credentials. Yarn, pnpm, and Bun projects can use their normal install and build
steps; Pledge still uses npm to acquire packages and prepare dependencies.

Follow the [CI guide](docs/ci.md) for pinned installation, authentication,
workspace handling, and troubleshooting. See [JSON reports](docs/results.md#json-reports)
for the output contract.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and commit conventions,
[the test guide](tests/README.md) for focused checks, and
[architecture](docs/architecture.md) for the analysis pipeline.

The maintained development flow targets Unix environments. Windows has not
been validated as a release platform. Checks run locally and through hosted
workflows. See [release automation](docs/releases.md) for activation requirements.

## Licensing

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.

Third-party package fixtures retain their respective licenses.
