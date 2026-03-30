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

### Activate Git Hooks

Quality checks are enforced via git hooks. Activate them after cloning:

```sh
git config core.hooksPath scripts/git-hooks
```

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
