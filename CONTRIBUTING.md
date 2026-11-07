# Contributing

Pledge builds the `pledge` crate and executable. Development checks
run locally and through the hosted checks workflow. Release automation and its
activation requirements are described in [Releasing](docs/releases.md).
For installing and using the linter in another project's CI, see
[CI integration](docs/ci.md).

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
npm install -g npm@11.19.0
npm install -g typescript@5.9.3 typescript-language-server@5.1.3
make setup
```

On other platforms, install the same tools with your preferred package manager.
Keep `bun`, `cargo`, `node`, `npm`, and `typescript-language-server` on `PATH`.
The npm consumer profile pins npm 11.19.0 so packing and installation evidence
records a reproducible package-manager version.
Public type evaluation uses the JavaScript TypeScript compiler API; a native
`tsc` executable alone does not provide it. Normal analysis can run with Node.js
and the JavaScript compiler without a language server. The full integration
suite additionally exercises language-server protocol behavior. The managed fallback downloads a
complete, pinned language-server 5.1.3 and TypeScript 5.9.3 pair. LSP checks
include a fresh managed installation and therefore require registry access.
The local HTTPS registry check also requires `openssl` on `PATH` to generate an
ephemeral certificate; it binds only to loopback and fetches no external packages.
Consumer tests use strict TypeScript with NodeNext resolution and modern Node.js
with synchronous ESM loading (Node 22.12+). Dependency installation disables
scripts and lockfile generation; installation failure or timeout degrades analysis.
The CLI also disables packing lifecycle scripts; build package outputs before
analysis. Acquisition and dependency installation use npm configuration, including
the original project's `.npmrc`, with 120-second subprocess timeouts.

`make setup` sets this clone's `core.hooksPath` to `scripts/hooks`. It replaces an
older hook path without overwriting files in `.git/hooks`. The tracked wrappers
call the Bun scripts directly, so later hook changes take effect immediately.
Linked worktrees use the hooks from their own checkout.

## Local checks

| Command | Purpose |
| --- | --- |
| `make check` | Language coverage inventory, rustfmt, clippy across all targets, and the default release test suite |
| `make check-lsp` | Release tests with the real language server, serialized to avoid workspace races |
| `make coverage` | Default-suite line coverage checked against `.coverage-baseline` |
| `make test-tools` | Hook, release, and language-coverage tooling checks in temporary repositories, without building Rust |
| `make language-coverage` | Offline reference accounting, evidence links, and public-report freshness |
| `make language-coverage-update` | Regenerate the public report after reviewing inventory changes |
| `make check-all` | Tooling checks, default checks, LSP tests, and coverage |
| `make reference-suite` | Only the pinned published-package comparisons |

`make check` includes the reference suite through `cargo test --release`; it does
not run those same cases twice. Cargo checks use `--locked` so validation does
not silently update the lockfile. LSP tests are opt-in because they require
external processes and may install fixture dependencies through npm.

See [language coverage](docs/language-coverage.md#reproduce-and-maintain-coverage)
for upstream baseline audits, version profiles, and consumer-result artifacts.
Documented limitations and unverified cases do not count as passing conformance.

The hooks use these same checks:

- **pre-commit:** tooling checks for script/Makefile changes; formatting, linting,
  and default tests for Rust, embedded compiler, Cargo/config, or package fixture
  changes. Deleted inputs count too. Documentation-only commits skip these Rust
  checks.
- **prepare-commit-msg:** suggests a type and reminds authors to consider
  changelog usefulness and compatibility, without rewriting their message.
- **commit-msg:** validates unscoped Conventional Commits and rejects
  attribution trailers. `feat`/`fix` must change implementation source; `test`
  must change tests or implementation source. The embedded compiler's `.cjs`
  source counts as implementation alongside Rust.
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

Source follows the same language and ecosystem boundaries as the tests.
`src/app/` owns the CLI and orchestration; `src/core/` contains language-independent
findings and limits. Language models, syntax, and rules live under
`src/languages/<language>/`; package and runtime conventions live under
`src/ecosystems/<ecosystem>/`. Keep acquisition, discovery, compiler enrichment,
comparison, and reporting separate within those owners. Add a new file when it
gives a concept a clear owner; avoid catch-all utility modules. See
[architecture](docs/architecture.md) for the pipeline and extension guidance.

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
- `docs/README.md` routes readers to a guide by the task they want to complete.
- `docs/results.md` owns report fields, severities, and exit-code semantics.
- `docs/rules/` explains detection, severity, examples, and limitations.

Write for the person trying to check or publish a package:

- Start with the task and expected outcome. Put the shortest useful command
  near the beginning, with its prerequisites and working directory.
- Show a concrete change and its effect on a consumer before explaining the
  implementation. Use small examples and label illustrative output.
- Separate what the tool detected, what it could not verify, and what remains
  untested. A successful static check does not prove every runtime behavior.
- Explain failures with a next step. Keep baseline selection, first-publish
  behavior, and incomplete analysis explicit in CI examples.
- Keep an exact contract in one place and link to it from task guides. The
  README introduces the product and first check; rule catalogs, JSON fields,
  and internal architecture belong in their focused references.
- Describe current behavior. Verify commands, rule IDs, and claims against
  source and relevant tests; distinguish planned features from shipped ones.
- Check relative links, heading anchors, and examples for documentation-only
  changes. Run the smallest relevant behavior check when an example needs it.

Use descriptive kebab-case names for new topic docs and link them from the
index. Planning folders, session logs, phase summaries, and worktree bookkeeping
do not belong in the repository.

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

Before every commit, read the staged diff from a changelog reader's perspective:
describe the user-visible outcome and classify its compatibility impact.
Internal maintenance is not a product feature. Follow the
[release policy](docs/releases.md#classify-changes-for-readers) for versioning,
breaking changes, and squash-merge requirements.

Do not add scopes such as `feat(parser):`, phase identifiers, co-author lines,
sign-offs, reviewed-by/tested-by trailers, or generated-by attribution. A real
public compatibility break uses `!` and a `BREAKING CHANGE:` explanation.
Merge commits follow the same subject convention.

## Baseline maintenance

Reference findings are reviewed contracts, not generated output to accept
automatically. Follow [the curation guide](docs/reference-suite.md) when updating
them. Never fetch new package fixtures during a test run.

`.coverage-baseline` is the minimum default-suite line coverage percentage.
After an intentional coverage improvement, `scripts/update-baseline.sh` measures
and updates it. Review the change; a failing check is not a reason to lower the
threshold.
