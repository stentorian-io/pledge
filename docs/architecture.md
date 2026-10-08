# Architecture

The `interlint` CLI compares the public surface of a local npm package with its
previously published version. Pledge is the repository name; the crate and
executable still use `interlint`.

## Analysis pipeline

1. `src/acquire/` packs the local package, reads registry metadata, downloads the
   published tarball, and extracts both versions into temporary directories.
2. Declared dependencies are installed before `src/discover/` resolves package
   entry points and builds export maps with tree-sitter. Discovery also captures
   directives, decorators, and release tags. Installation disables scripts and
   lockfile generation; a failed installation leaves analysis incomplete.
3. `src/lsp/` manages the TypeScript language server and its paired compiler.
   Hover and definition requests supply declaration information; compiler
   evaluation supplies the concrete public type shapes used by comparison.
4. `src/compare/` compares exports, types, metadata, structural declarations,
   assets, and directives. It pairs related symbols and consolidates findings.
5. `src/report/` renders the findings. `src/main.rs` coordinates the passes,
   deduplicates their results, and selects the process exit code.

First-publish handling skips downloading a previous version and reports the
discovered exports as additions. LSP failures produce an explicit degraded
result; structural comparisons can continue with the evidence available.

Library callers can prepare dependencies with `lsp::client::prepare_dependencies`,
discover exports, and call `lsp::enrich_prepared_exports`. The convenience
`lsp::enrich_exports` prepares dependencies and refreshes dependency-backed
exports when given a map discovered before installation.

## Ownership boundaries

| Area | Responsibility |
| --- | --- |
| `src/cli.rs` | Arguments and help text |
| `src/acquire/` | Package and registry I/O |
| `src/discover/` | Source parsing and export resolution |
| `src/lsp/` | Language server lifecycle, requests, and enrichment |
| `src/types/` | Shared export/finding models and type parsing |
| `src/compare/` | Compatibility decisions and rule identifiers |
| `src/report/` | Terminal presentation |
| `src/source_filter.rs` | Shared filtering of oversized or minified sources |

Keep CLI policy and process startup at the application boundary. Comparison
helpers should expose findings in domain terms rather than print output or
start subprocesses. Introduce a module for a cohesive rule family instead of
adding a general-purpose helper collection.

## Public contracts

Finding rule IDs, severities, source locations, and CLI exit codes are observable
behavior. Rule details belong in [the rule reference](rules/README.md).

- Exit `0`: analysis completes without a breaking finding.
- Exit `1`: a breaking change is found.
- Exit `2`: an internal operation fails, such as registry or filesystem I/O.
- Exit `3`: analysis completes in degraded mode, even when breaking findings
  are also present.

An informational limitation describes what the analyzer could not verify.
Keep it distinct from a confirmed compatibility change. Tests must cover both
the successful detection and the relevant silent or degraded path.

The [reference suite](reference-suite.md) exercises real package snapshots in a
deterministic mode. Synthetic fixtures provide focused coverage for individual
rules and edge cases.
