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
cargo test --locked --release --features lsp-tests --test typescript compiler:: -- --test-threads=1
make reference-suite
```

The default suite includes pinned reference packages and degraded-mode cases.
The `lsp-tests` feature enables compiler, language-server, and runtime consumer
checks that need Node.js, npm, TypeScript, and `typescript-language-server`.
Some checks install fixture dependencies. `make check-lsp` runs the complete
feature-enabled suite serially. Test names include their module paths; when
using `--exact`, use the full name shown by `cargo test --test <target> -- --list`.

Use `make check` for formatting, clippy, and all default tests before committing.
The push hook runs LSP and coverage checks for code or fixture changes. See
[contributing](../CONTRIBUTING.md) for prerequisites and the complete command list.

## Fixtures and assertions

- Mirror the owning language or ecosystem and behavior under `fixtures/`.
  Use small `old/` and `new/` package pairs for compatibility changes.
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
command-recording executables. Run `make test-tools` when changing hooks or their
shared scripts; these checks do not require external services or product tests.
