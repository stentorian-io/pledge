# interlint

A CLI tool that catches breaking changes to public interfaces through static analysis. Compares the current version of a JS/TS package against its previously published version on npm and reports findings with semver-aligned severity -- like ESLint for API compatibility.

## Installation

```sh
cargo build --release
```

## Usage

```sh
interlint [path]
```

Analyzes the package at `path` (defaults to current directory), compares it against the previously published npm version, and reports breaking changes.

## Rules

Interlint emits findings tagged with stable rule-IDs. Severity maps to CLI exit code:

| Severity | Exit code | Meaning |
|----------|-----------|---------|
| Major | 1 | Breaking change — consumers will observe broken behavior |
| Warning | 2 | Observable behavior change — exhaustive-switch breaks, deprecation |
| Degraded | 3 | Analysis could not complete cleanly (LSP unavailable, parse failure) |
| Minor | 0 | Non-breaking change worth noting |
| Patch | 0 | Cosmetic / non-observable change |

### Phase 19 rule-IDs (comment-scan pass)

| Rule ID | Severity | Trigger |
|---------|----------|---------|
| `directive-added` | Major | A `/// <reference ... />`, `/// <amd-module ... />`, or `/// <amd-dependency ... />` directive is added at the top of a source file |
| `directive-removed` | Major | A triple-slash directive present in the old version is dropped in the new version |
| `directive-changed` | Major | A directive's attribute value changes (including the TS 5.5+ `preserve="true"` toggle) |
| `directive-reordered` | Minor | The same set of directives appears in both versions in a different document order |
| `decorator-added` | Major | A stage-3 decorator (class, method, field, or parameter) is added to an exported target |
| `decorator-removed` | Major | A stage-3 decorator is removed from an exported target |
| `decorator-arguments-changed` | Major | A decorator factory's call arguments change (e.g., `@Route('/users')` → `@Route('/people')`) |

Phase 19 also extends the existing `export-deprecated` (Warning) rule: `@deprecated`, `@internal`, `@alpha`, `@beta`, and `@public` TSDoc tags are now detected uniformly across every `RawExport` construction site (default exports, named re-exports, destructured pairs, compound-assignment member exports, star re-exports, and all bundler-CJS variants).

See `docs/rules/directive.md` for INF-05 (triple-slash directives) details and the TypeScript compiler position rule.

## Development Setup

### Prerequisites

- Rust toolchain (rustc, cargo, rustfmt, clippy)
- Node.js with `typescript-language-server` installed globally:
  ```sh
  npm install -g typescript-language-server
  ```
- Coverage tooling:
  ```sh
  rustup component add llvm-tools
  cargo install cargo-llvm-cov
  ```

### Repository Setup

After cloning, run:

```sh
make setup
```

This activates the git hooks that enforce quality checks on every commit and push.

### What the Hooks Check

**Pre-commit** (~4s, runs on every commit):
- `cargo fmt --check` -- formatting
- `cargo clippy -- -D warnings` -- linting
- `cargo test` -- unit tests

**Pre-push** (~45s, runs on every push):
- `cargo test --features lsp-tests` -- full integration suite including LSP tests
- `cargo llvm-cov --fail-under-lines <baseline>` -- coverage must not drop below baseline

### Updating Coverage Baseline

When you intentionally improve test coverage and want to raise the threshold:

```sh
scripts/update-baseline.sh
```

This measures current coverage and updates `.coverage-baseline`. Commit the updated file.

### Running Tests

```sh
# Fast unit tests
cargo test

# Full suite including LSP integration tests
cargo test --features lsp-tests
```
