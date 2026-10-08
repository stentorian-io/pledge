# Contributing

Pledge currently builds the `interlint` crate and executable. Development checks
run locally; this repository does not have hosted CI workflows or pipelines.

## Setup

Install Rust with rustfmt and clippy, Bun for development scripts, and Node.js
with npm for package packing and language server integration. On macOS, the
checked-in `Brewfile` provides Bun, Node.js, and rustup:

```sh
brew bundle
```

With a Rust toolchain selected in rustup, install the components and tools used
by the full local checks:

```sh
rustup component add rustfmt clippy llvm-tools-preview
cargo install cargo-llvm-cov --locked
npm install -g typescript@5.9.3 typescript-language-server@5.1.3
make setup
```

On other platforms, install the same tools with your preferred package manager.
Keep `bun`, `cargo`, `node`, `npm`, and `typescript-language-server` on `PATH`.
Use the JavaScript TypeScript compiler with `lib/tsserver.js`; a native `tsc`
executable alone cannot serve LSP requests. The managed fallback downloads a
complete, pinned language-server 5.1.3 and TypeScript 5.9.3 pair. LSP checks
include a fresh managed installation and therefore require registry access.

`make setup` sets this clone's `core.hooksPath` to `scripts/hooks`. It replaces an
older hook path without overwriting files in `.git/hooks`. The tracked wrappers
call the Bun scripts directly, so later hook changes take effect immediately.
Linked worktrees use the hooks from their own checkout.

## Local checks

| Command | Purpose |
| --- | --- |
| `make check` | rustfmt, clippy across all targets, and the default release test suite |
| `make check-lsp` | Release tests with the real language server, serialized to avoid workspace races |
| `make coverage` | Default-suite line coverage checked against `.coverage-baseline` |
| `make test-tools` | Development hook checks in temporary Git repositories, without building Rust |
| `make check-all` | Tooling checks, default checks, LSP tests, and coverage |
| `make reference-suite` | Only the pinned published-package comparisons |

`make check` includes the reference suite through `cargo test --release`; it does
not run those same cases twice. Cargo checks use `--locked` so validation does
not silently update the lockfile. LSP tests are opt-in because they require
external processes and may install fixture dependencies through npm.

The hooks use these same checks:

- **pre-commit:** tooling checks for script/Makefile changes; formatting, linting,
  and default tests for Rust, Cargo/config, or package fixture changes. Deleted
  inputs count too. Documentation-only commits skip these Rust checks.
- **prepare-commit-msg:** suggests a type and changes `feat`/`fix` to `docs`,
  `test`, or `chore` when the staged changes fall entirely in that category.
- **commit-msg:** validates unscoped Conventional Commits and rejects
  attribution trailers. `feat`/`fix` must change implementation source; `test`
  must change tests or implementation source.
- **pre-push:** examines the difference between each remote and local ref. Code
  and fixture changes run LSP tests and coverage; tooling changes run the hook
  checks. New refs or missing remote objects run all push checks. Ref deletions
  and documentation-only differences skip them.

Hooks run against the current checkout. Keep the code you validate consistent
with the changes being committed or pushed, particularly when staging only
part of a file. Fix failed checks rather than bypassing hooks.

During development, select the smallest relevant test from
[tests/README.md](tests/README.md). Save broad test runs for commit preparation
or an explicit request. Before publishing a code change, run `make check-all`.

## Code structure

Keep acquisition, discovery, LSP enrichment, type representation, comparison,
and reporting in their existing domain modules. Add a new file when it gives a
concept a clear owner; avoid catch-all utility modules. See
[architecture](docs/architecture.md) for the pipeline and boundaries.

Use names that explain the behavior. Prefer borrowing and slices when ownership
is unnecessary, structured errors in domain code, and `anyhow` context at
application boundaries. Keep validation separate from mutation and I/O.
Comments should explain constraints and surprising choices rather than repeat
the code. The complete repository guidance is in [AGENTS.md](AGENTS.md).

Changes to rule IDs, severity, degradation, or exit codes need behavioral tests
and corresponding rule documentation. Keep tests independent and use the
smallest package fixture that proves the change.

## Documentation

- `README.md` covers the product, installation, and usage.
- `CONTRIBUTING.md` covers local setup and verification.
- `tests/README.md` describes test boundaries and fixture choices.
- `docs/README.md` indexes maintained topic documentation.
- `docs/rules/` explains detection, severity, examples, and limitations.

Use descriptive kebab-case names for new topic docs. Link docs to current code
and observable behavior. Planning folders, session logs, phase summaries, and
worktree bookkeeping do not belong in the repository.

## Commit messages

Use unscoped Conventional Commits with a lowercase description and no final
period. Aim for a subject of 72 characters or fewer:

```text
feat: detect removed package exports
fix: preserve private field identity
test: cover conditional exports
docs: explain degraded analysis
chore: update local development tools
```

Choose the type by the change's changelog category. Also use `refactor`, `perf`,
`style`, `build`, `ci`, or `revert` when appropriate. Use `chore` or `build` for
local verification setup; `ci` is reserved for actual CI configuration.

Do not add scopes such as `feat(parser):`, phase identifiers, co-author lines,
sign-offs, reviewed-by/tested-by trailers, or generated-by attribution. A real
public compatibility break can use `!` and a `BREAKING CHANGE:` explanation.
Merge commits follow the same subject convention.

## Baseline maintenance

Reference findings are reviewed contracts, not generated output to accept
automatically. Follow [the curation guide](docs/reference-suite.md) when updating
them. Never fetch new package fixtures during a test run.

`.coverage-baseline` is the minimum default-suite line coverage percentage.
After an intentional coverage improvement, `scripts/update-baseline.sh` measures
and updates it. Review the change; a failing check is not a reason to lower the
threshold.
