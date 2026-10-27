# Architecture

The `interlint` CLI compares the public surface of a local npm package with its
previously published version. Pledge is the repository name; the crate and
executable still use `interlint`.

## Source layout

Source is grouped by language and ecosystem, then by responsibility. The
[integration suites](../tests/README.md) use the same ownership boundaries.

```text
src/
  main.rs                        # Process startup and exit codes
  app/                           # CLI arguments, orchestration, terminal output
  core/                          # Findings, consolidation, source limits
  languages/
    javascript/
      model.rs                   # JS/TS exports and class surfaces
      syntax/                    # Grammar dispatch and syntax extraction
      compare/                   # Export, class, runtime, structural comparisons
    typescript/
      model.rs                   # Declaration metadata
      config.rs                  # tsconfig paths and JSONC parsing
      syntax/                    # Typed declarations, annotations, directives
      types/                     # Type shapes, parsing, subtyping, reachability
      compare/                   # Declaration and type compatibility rules
      compiler/                  # Compiler bridge, enrichment, managed toolchain
        protocol/                # Language-server lifecycle and requests
  ecosystems/
    node/
      acquire/                   # npm packing, registry access, extraction
      dependencies.rs            # npm dependency preparation
      discovery/                 # Package entry points and module resolution
      compare/                   # Metadata, exports/imports, module loading
        assets/                  # Distributed files, CSS, JSON schemas
```

`core` does not depend on a language or ecosystem. It contains findings and
resource limits that another analyzer can reuse without adopting the JS/TS
export model. `app` selects and coordinates the current analyzers and renders
their findings; `main.rs` maps application outcomes to process exit codes.

JavaScript and TypeScript share an export model and tree-sitter traversal because
TypeScript extends JavaScript. The shared model explicitly carries TypeScript
metadata and type evidence. TypeScript declaration extraction, type operations,
and compiler integration have their own modules. This is a JS/TS implementation
boundary, not a universal model that future languages must implement.

Node owns npm manifests, package acquisition, dependency preparation, entry
points, and module loading. Its discovery resolves package paths and calls the
language syntax extractors. CommonJS syntax belongs to JavaScript; package
`exports`, `imports`, and resolution conditions belong to Node. Compiler
convenience APIs can prepare Node dependencies and rediscover package exports;
that package integration is explicit in their imports.

Comparison rules and their identifiers live together under each owner's
`compare/` directory. Pure comparison code returns findings rather than printing
output or starting subprocesses. Large unit suites are private child modules
beside their implementation; smaller suites remain inline.

## Analysis pipeline

1. `ecosystems::node::acquire` resolves the selected registry version through npm,
   packs built local artifacts with scripts disabled, downloads the published
   package through npm, and extracts both versions into owned temporary
   directories. Project npm configuration is preserved for dependency preparation.
2. `ecosystems::node::dependencies` prepares declared dependencies before
   `ecosystems::node::discovery` resolves entry points and builds export maps
   through the JS/TS syntax extractors. Discovery also captures directives,
   decorators, and release tags. Installation disables scripts and lockfile
   generation; a failed installation leaves analysis incomplete.
3. `languages::typescript::compiler` evaluates public types and declaration
   locations with the JavaScript TypeScript compiler. It uses the original
   workspace's compiler, then PATH or a managed toolchain; normal enrichment does
   not require a language-server process. Source byte columns become UTF-16 before
   resolving identifiers. Its `protocol` module remains available for
   language-server integrations and their regression tests.
4. The JavaScript, TypeScript, and Node `compare` modules compare exports,
   structural declarations, types, metadata, module loading, assets, and
   directives. Related symbol findings are paired and consolidated.
5. `app` combines the passes, deduplicates and sorts findings, derives the exit
   status, and renders text or versioned JSON through `app::report`. Compiler
   failures remain in the report's warnings alongside discovery failures.

First-publish handling skips downloading a previous version and reports the
discovered exports as additions. Missing compiler evidence produces an explicit
degraded result; structural comparisons continue with the evidence available.

Library callers can prepare dependencies with
`ecosystems::node::dependencies::prepare_dependencies`, discover exports with
`ecosystems::node::discovery::discover_exports`, and call
`languages::typescript::compiler::enrich_prepared_exports`. The compiler's
`enrich_exports` convenience function prepares dependencies and refreshes
exports when given a map discovered before installation.

## Type evidence

Compiler shapes preserve deferred conditional branches, generic mapped keys,
key remapping, modifier addition/removal, and intrinsic string operations.
Type parameter identities follow their binding positions so renaming a generic
parameter does not change its contract. The compiler expands private aliases
inside these expressions; unresolved references still make analysis incomplete.
Generic binders retain resolved constraints, defaults, const inference, and
variance annotations, including nested and returned callables. Readonly arrays
and tuples retain their write restrictions; mutable results can satisfy readonly
consumers, while removing accepted readonly inputs is breaking.

## Adding a language or ecosystem

Add a language under `languages/<language>/`, with its own model, syntax, and
comparison modules as needed. Add package-manager or runtime conventions under
`ecosystems/<ecosystem>/`. Register modules explicitly and connect the new
analyzer through `app`; do not add language-specific fields to `core`.

Follow the same ownership in tests and fixtures. Share a model or helper only
when its contract is actually shared. Keep compiler subprocesses and package
I/O separate from pure parsing and comparison. No generic plugin interface is
required to add the next analyzer.

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
