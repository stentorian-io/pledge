# Test architecture

Integration tests are grouped by language, ecosystem, and observable behavior.
Unit tests stay beside the implementation and cover parsing, matching,
comparison, and invariants that are easier to isolate there.
Large unit suites use private child files such as `syntax/exports/tests.rs`,
registered with `#[cfg(test)] mod tests;`. Source ownership mirrors these
boundaries; see [the source architecture](../docs/architecture.md).

```text
tests/
  core/                     # Shared comparison, diagnostics, and resource limits
  languages/
    javascript/             # Classes, functions, modules, properties, runtime
    typescript/             # Declarations, types, generics, compiler integration
  ecosystems/
    node/                   # Packages, module loading, bundlers, assets, consumers
  support/fixtures.rs       # Language-independent fixture lookup
  fixtures/
    javascript/<behavior>/
    typescript/<behavior>/
    node/<behavior>/
```

## Choosing a suite

Cargo registers four integration targets in `Cargo.toml`: `core`, `javascript`,
`typescript`, and `node`. Each target has a `main.rs` that registers behavior
modules through `mod.rs`. Automatic integration-test discovery is disabled;
register every new module explicitly so Cargo runs its tests.

| Boundary | What belongs here |
| --- | --- |
| `core` | Finding consolidation, source identity, incomplete-analysis reporting, resource limits |
| `languages/javascript` | JavaScript syntax and public runtime contracts, including CommonJS syntax and JSDoc |
| `languages/typescript` | TypeScript syntax, type contracts, declaration merging, and compiler evidence |
| `ecosystems/node` | npm package metadata and entry points, Node resolution/loading, bundler output, assets, and pinned published packages |

Put a test in the language whose contract it checks, even when another language
could express a similar construct. TypeScript compiler tests may use JavaScript
or JSDoc inputs to verify compiler interoperability. Node package tests may use
TypeScript declarations because the behavior under test is package resolution.
Some core tests use the current JS/TS discovery pipeline as setup; their assertions
concern shared findings and limits rather than a language feature.

Extend the module that owns the behavior. Add a descriptive module when it is a
separate concern. Shared test support belongs with its owning language or
ecosystem; introduce cross-language support only when multiple suites need the
same contract. To add a language, create `languages/<language>/main.rs`, register
its Cargo target, and place its fixtures under `fixtures/<language>/`. Keep
runtime and package-manager assumptions in ecosystem suites.

## Focused commands

Use a target and a module or test filter during development:

```sh
cargo test --locked --release --test core
cargo test --locked --release --test javascript modules::reexports
cargo test --locked --release --test typescript classes::
cargo test --locked --release --test node packages::
cargo test --locked --release --test node packages::cli::
cargo test --locked --release --features lsp-tests --test node packages::cli::npm_cli:: -- --test-threads=1
cargo test --locked --release --features lsp-tests --test typescript compiler:: -- --test-threads=1
make reference-suite
```

The default suite includes pinned reference packages and degraded-mode cases.
The `lsp-tests` feature enables compiler, language-server, and runtime consumer
checks that need Node.js, npm, TypeScript, and `typescript-language-server`.
Some checks install fixture dependencies. `make check-lsp` runs the complete
feature-enabled suite serially. Test names include their module paths; when
using `--exact`, use the full name shown by `cargo test --test <target> -- --list`.

The `compiler::version_support` suite checks syntax, library types, and inferred
predicates against the compiler selected on `PATH`, with `tsc` consumer results
as an independent oracle. CI runs it with TypeScript 4.9.5, 5.0.4, 5.2.2, 5.4.5,
5.7.3, 5.9.3, and 6.0.3 on Node 22.12.0 and 24.0.0. The same matrix reruns the
returned-contract, callable, overload, nominal-identity, namespace-default,
CommonJS interop, forwarding and inferred property identities, unresolved factory
and symbol-key evidence, and path-alias consumer cases. These are
targeted version boundaries, not exhaustive ECMAScript or TypeScript
conformance coverage.

The [language coverage inventory](../docs/language-coverage.md) records exact
evidence tests, source revisions, compiler options, and unverified upstream cases.
`compiler::conformance` adapts pinned TypeScript cases to package consumers under
NodeNext, Node16, and CommonJS resolution. TypeScript 6's legacy Node-resolution
deprecation is explicitly acknowledged in the CommonJS profile. JavaScript
consumer tests include Test262-derived namespace-name and symbol-registry cases.
These adaptations exercise Pledge; running an upstream suite solely against an
engine or compiler would not verify Pledge's comparison behavior.

Keep inventory metadata under `docs/coverage/`, tooling under
`scripts/conformance/`, and executable cases in their existing language or
ecosystem suite. Each upstream case attribution includes an immutable revision
and source checksum. Normal test runs do not download upstream replacements.

The `modules::commonjs_names` Node suite compares the installed loader's named
imports with real ESM and `require()` consumers. CI runs it on Node 22.12.0,
24.0.0, and 26.7.0 because these releases use different bundled CommonJS parsers.

`packages::cli` checks real process exit codes and reports using isolated npm and
compiler stand-ins on Unix, without network access. Its opt-in `npm_cli` module
uses real npm and TypeScript with a temporary authenticated loopback registry;
it needs permission to bind a local TCP port, but does not fetch test packages
from an external registry. Its HTTPS configuration regression uses `openssl`
to generate a temporary certificate. Provide the JavaScript TypeScript compiler on `PATH`
or populate the managed toolchain cache before running that check.

Use `make check` for formatting, clippy, and all default tests before committing.
The push hook runs LSP and coverage checks for code or fixture changes. See
[contributing](../CONTRIBUTING.md) for prerequisites and the complete command list.

## Fixtures and assertions

- Mirror the owning language or ecosystem and behavior under `fixtures/`.
  Use small `old/` and `new/` package pairs for compatibility changes.
- Give language-rule fixtures an explicit root `exports` entry when they test a
  single public surface. A legacy `main` also exposes deep imports; exercise
  those distinct identities in the Node resolution suites.
- Name tests as behavior sentences. Keep arrange, act, and assert separate.
- Assert exact rule IDs and severities, covering false positives as well as
  positive detections. Assert message text only when wording is the contract.
- Derive expectations independently of the implementation under test. Consumer
  cases compile or execute the same consumer against both package versions.
  Topic JSON files live with their language or ecosystem fixtures. The JS and
  TS suites reuse `ecosystems/node/consumers/support.rs` for Node package setup
  and process execution.
- Keep tests independent of order. Use temporary directories for generated
  state; default tests must not modify committed fixture inputs.
- Generated `node_modules`, package lockfiles, and synthesized tsconfig files
  in fixtures are ignored. Do not commit them.
- Pin published tarballs and review expected findings as described in
  [reference-suite curation](../docs/reference-suite.md). The Node reference
  suite and snapshot generator share `ecosystems/node/reference/support.rs`.
  The generator lives beside them in `ecosystems/node/reference/dump.rs` and
  is explicitly registered as the manual `reference_suite_dump` Cargo example
  target. Run it through `make reference-suite-regen`.

## Development tooling

`scripts/hooks/hooks.test.ts` exercises hooks in temporary repositories using
command-recording executables. `scripts/releases/releases.test.ts` covers commit
semantics, release timing, holds, and version consistency. Run `make test-tools`
when changing hooks or release tooling; these checks do not require external
services or product tests.

`scripts/conformance/conformance.test.ts` checks extraction, missing classifications,
evidence paths, report generation, and runner failures in isolated repositories.
