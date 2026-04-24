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

### Phase 20 rule-IDs (TS type-system features)

| Rule ID | Severity | Trigger |
|---------|----------|---------|
| `type-narrowing-widened` | Minor | A `satisfies T` clause is removed from an exported const/let/var; consumers that relied on the narrowed type will observe the wider declared type |
| `infer-extends-constraint-changed` | Major | The `C` in `T extends infer U extends C ? ... : ...` on an exported conditional type is changed |
| `generic-inference-constraint-removed` | Minor | A `NoInfer<T>` wrapper is removed from a function parameter position or generic constraint on an exported signature |
| `union-variant-added` | Warning | A variant is added to an exported discriminated (tagged) union — may break exhaustive `switch`/`match` consumers |
| `index-signature-key-pattern-changed` | Major (narrowing) / Minor (widening) | The template-literal key pattern of an exported index signature (e.g. `` [key: `on${string}`]: T ``) is narrowed or widened |

Phase 20 also introduces two informational limitations surfaced in finding messages:

- `limitation: satisfies-narrowed-type-not-verified` — TYPE-01 degraded mode (LSP unavailable); rule still fires syntactically but narrowing direction is not confirmed.
- `limitation: noinfer-via-local-alias` — GEN-07; users aliasing `NoInfer<T>` locally (`type MyNoInfer<T> = NoInfer<T>`) evade detection.

See `docs/rules/type-narrowing.md`, `docs/rules/infer-extends.md`, `docs/rules/noinfer-wrapper.md`, `docs/rules/union-variant-added.md`, and `docs/rules/template-index-signature.md` for per-rule detail.

### Phase 21 rule-IDs (class-surface completion)

| Rule ID | Severity | Trigger |
|---------|----------|---------|
| `class-field-made-private` | Major | CLS-11 — public `foo` on an exported class becomes private `#foo` (ES2022); consumers referencing the member will break |
| `class-field-made-public` | Minor | CLS-11 — private `#foo` on an exported class becomes public `foo`; previously-hidden surface is now observable |

A field rename within the private space (`#foo` → `#bar`) is deliberately silent — consumers cannot observe private fields, so a private-space rename is not an API change. A class that legally has BOTH public `foo` AND private `#foo` on both sides also fires no CLS-11 findings (the collision guard detects that both bare names exist on both sides).

See `docs/rules/class-field-visibility-transition.md` for matching heuristic, limitations, and decorator interaction.

CLS-12 (class static initialization block member extraction) reuses the existing `class-method-removed` rule-ID for removed static-block assignments (`class Foo { static { Foo.bar = 1 } }` — removing `Foo.bar` fires Major `class-method-removed` with the `static property` noun) and surfaces the `limitation: dynamic-static-member-key` annotation under the existing `class-static-type-changed` rule-ID for subscript-expression LHS (`Foo[x] = ...`). See [docs/rules/class-static-block.md](docs/rules/class-static-block.md).

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
