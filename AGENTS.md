# AGENTS.md

This file defines how coding agents should read, edit, test, and review this
repository. Share the relevant boundaries with any subagents when delegating.

## Project Context

Pledge is a Rust 2021 CLI that detects breaking changes in public JavaScript and
TypeScript package interfaces. The Cargo package and executable are currently
named `interlint`. Compare packed local packages with their published npm
versions, discover exports with tree-sitter, enrich types through the TypeScript
compiler API, and report findings by semver impact.

Primary files and boundaries:

- `src/main.rs` and `src/cli.rs`: command arguments and pipeline orchestration.
- `src/acquire/`: npm packing, registry downloads, and tarball extraction.
- `src/discover/`: entry point resolution, export discovery, and comment scans.
- `src/lsp/`: compiler enrichment, toolchain management, and language-server protocol support.
- `src/types/`: shared domain types and TypeScript type parsing.
- `src/compare/`: compatibility rules, pairing, and finding consolidation.
- `src/report/`: terminal output and summaries.
- `src/source_filter.rs`: shared limits for source processing.
- `tests/`: behavioral integration suites and package fixtures.
- `docs/`: maintained architecture, rule references, and reference-suite guidance.
- `scripts/`: local development checks, hooks, and fixture maintenance.

Read `README.md` and `CONTRIBUTING.md` before changing setup, commands, or user
behavior. Read `tests/README.md` before changing test placement or fixtures.

## How To Work

- Read the existing implementation before adding abstractions or changing
  module boundaries. Make the smallest coherent change that solves the task.
- Keep acquisition, discovery, type enrichment, comparison, and reporting
  separate. Keep process startup and CLI decisions out of pure comparison code.
- Remove duplication when the repeated code represents the same concept and
  has the same reason to change.
- Add dependencies only with a clear justification. Prefer the standard library
  and existing dependencies when they fit.
- Prefer direct changes to internal contracts over compatibility layers.
  Preserve documented CLI behavior, finding rule IDs, severity, and exit codes
  unless the requested change calls for a public behavior change.
- Preserve user changes. Never revert unrelated work.
- Ask only when ambiguity materially affects public behavior, data safety, or
  compatibility.
- Keep planning and execution state out of the repository. Do not create GSD
  phase folders, numbered plan summaries, or bookkeeping commits.

## Readable Rust

Code should communicate its purpose through names, structure, and flow.

### Names And Modules

- Name files, modules, functions, types, and tests after their domain or behavior.
  Avoid generic `utils`, `helpers`, and `misc` buckets.
- Avoid vague names such as `data`, `item`, `value`, or `manager` when a domain
  name is available. Include uncertainty or state when it affects behavior.
- Give each module a focused responsibility. Keep imports at the top, merged
  and deduplicated. Nested modules carry their own imports.
- Start visibility private; use `pub(crate)` for internal cross-module access.
  Keep `pub` for the library surface needed by callers and integration tests.
- Name new tests and fixtures by the behavior they exercise, without phase,
  plan, or task numbers. Follow the language and behavior boundaries in
  `tests/README.md`.

### Comments And Shape

- Prefer renaming or restructuring to explaining obvious code with comments.
- Explain surprising behavior, performance constraints, invariants, and reasons
  for a limitation. Refer to rule names and maintained docs rather than plans.
- Let functions gather inputs, validate, derive named values, perform the
  operation, and return. Separate these steps with whitespace.
- Keep validation separate from mutation and I/O. Prefer explicit branches when
  they make the rule clearer; use `match` for meaningful domain variants.
- Avoid deeply nested control flow. Extract a helper for a named decision or
  transformation. Use combinators when they improve readability.
- Use rustfmt, but also reconsider a line's structure when wrapping obscures
  its meaning. Keep one thought on one line when it reads naturally.

### Types And Errors

- Use `Option`, enums, newtypes, and structured errors to encode uncertainty and
  invariants. Prefer making invalid states unrepresentable.
- Prefer enums or clearly named functions to ambiguous boolean parameters.
- Preserve declared error types. Use `thiserror` for domain errors and
  `anyhow` context at application boundaries where those patterns already fit.
- Avoid `unwrap` and `expect` in production unless an invariant is local,
  obvious, and unrecoverable. Tests may use them for clarity.
- Prefer borrowing over cloning and accept `&str` or slices when ownership is
  unnecessary. Keep lifetime relationships simple.
- Keep degraded analysis explicit. Do not silently turn missing type evidence
  or unsupported syntax into a confident compatibility verdict.

## Tests And Verification

- Prefer integration coverage for user-visible behavior. Use unit tests for
  pure parsing, comparisons, path safety, invariants, and awkward failure paths.
- Extend an existing test when it already covers the behavior. Do not repeat
  the same scenario through multiple layers without a distinct assertion.
- Use the smallest relevant fixture and name tests as behavior sentences.
  Separate arrange, act, and assert; keep expected results independently derived.
- Isolate temporary state and avoid test-order dependencies. Keep published
  package tarballs pinned; reference tests must not fetch replacements.
- During implementation, use focused checks. Save broad Rust test runs for
  commit preparation or an explicit request, and let hooks run their configured
  checks. Changes to development tooling should be verified in isolated repos.
- `make check` runs formatting, clippy, and the default release test suite.
  `make check-lsp` exercises the real language server; `make coverage` enforces
  the checked-in coverage baseline. See `CONTRIBUTING.md` for prerequisites.
- Never update expected findings or lower a coverage baseline just to make a
  check pass. Review baseline changes against the intended behavior.
- If a required check cannot run, state the exact reason and remaining risk.

## Documentation And Change Hygiene

- Keep `README.md` focused on the product and usage, `CONTRIBUTING.md` on local
  development, `tests/README.md` on test boundaries, and `docs/rules/` on rules.
- Use descriptive kebab-case names for new topic docs and link them from
  `docs/README.md`. Prefer a short maintained explanation over session logs.
- Update rule docs when detection, severity, or limitations change. Distinguish
  emitted rule IDs from limitations documented without a finding.
- Keep generated files, local state, secrets, and machine-specific paths out of
  commits. There are no hosted CI workflows or pipelines at present; add them
  only when explicitly requested.
- Do not mix broad refactors with behavioral changes. Batch related work into
  coherent commits; commit and push only within the user's authorized scope.
- Use unscoped Conventional Commits: `feat: ...`, `fix: ...`, `test: ...`,
  `docs: ...`, `refactor: ...`, `perf: ...`, `style: ...`, `build: ...`, or
  `chore: ...`. Choose the type by its user-facing changelog category.
- Start descriptions lowercase and omit the final period. Do not include
  scopes, co-author trailers, sign-offs, or generated-by attribution. Use a
  `BREAKING CHANGE:` footer when it conveys a real public contract change.
- Report what changed, the relevant verification, and material limitations.
  Reviews should lead with bugs, regressions, and missing behavioral coverage.
