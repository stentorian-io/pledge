# Test architecture

Place tests by observable behavior. Integration suites compare small package
fixtures and assert findings, severity, source identity, and degraded behavior.
Unit tests cover pure parsing, matching, comparisons, and invariants that would
be cumbersome to isolate through a complete package.

## Choosing a suite

| Behavior | Existing entry points |
| --- | --- |
| Basic discovery and comparisons | `integration_test.rs` |
| Signature and package metadata changes | `integration_phase3.rs`, `integration_phase6.rs` |
| Classes, enums, and generics | `integration_class_enum_generics.rs` |
| Type refinements | `integration_ref_rules.rs`, `phase20_*_integration.rs` |
| Asset and schema changes | `integration_asset_schema.rs` |
| Bundler output and export identity | `phase17_integration.rs`, `phase18_integration.rs`, `phase24_*_integration.rs` |
| Directives, release tags, and decorators | `phase19_*_integration.rs` |
| Private fields and static blocks | `phase21_*_integration.rs`, `phase27_class_surface_regression.rs` |
| Tuple labels, Symbol exports, and module aliases | `phase22_*_integration.rs` |
| Specifier and imports-map resolution | `phase23_*_integration.rs` |
| Informational limitations | `phase25_doc01_integration.rs` |
| Real language server enrichment | `lsp_integration.rs`, `phase22_inf04_integration.rs`, `inf04_spike.rs` |
| Pinned published packages | `reference_suite.rs` |
| TypeScript/Node consumer compatibility | `consumer_compatibility.rs`, `module_loading.rs` |

The numbered filenames are inherited from the earlier development process.
Use descriptive behavior names for new suites and fixtures. Extend an existing
suite when it already owns the behavior; don't duplicate a scenario solely to
change its filename.

## Focused commands

During iteration, select one case or the smallest relevant integration target:

```sh
cargo test --locked --release --test integration_test
cargo test --locked --release --test lsp_integration --features lsp-tests renamed_type_alias_same_shape_no_false_positive -- --test-threads=1
make reference-suite
```

The default suite includes the reference suite and degraded-mode cases. The
`lsp-tests` feature enables real language server tests. These require Node.js,
npm, TypeScript, and `typescript-language-server`; they may install dependencies
inside fixtures. Use `make check-lsp` for the complete serialized LSP run.

Use `make check` for formatting, clippy, and all default tests before committing
code. The push hook runs LSP and coverage checks for code or fixture changes.
See [contributing](../CONTRIBUTING.md) for the complete command list.

## Fixtures and assertions

- Keep the fixture small enough to show the behavior directly. Use `old/` and
  `new/` package pairs for compatibility changes.
- Name tests as behavior sentences. Keep arrange, act, and assert separate.
- Assert exact rule IDs and severities, and cover false positives alongside
  positive detections. Assert message text only when wording is the contract.
- Use independently reasoned expected results; don't compute expectations by
  calling the implementation under test.
- Keep tests independent of order. Use temporary directories for generated
  state and don't modify committed fixture inputs during default tests.
- Generated `node_modules`, package lockfiles, and synthesized tsconfig files
  in fixtures are ignored. Do not commit them.
- Pin published tarballs and review expected findings as described in
  [reference-suite curation](../docs/reference-suite.md).

## Development tooling

`scripts/hooks/hooks.test.ts` exercises the hooks in temporary repositories.
Cargo and external services are replaced with command-recording executables,
so it checks Git integration and failure handling without running product tests
or accessing the network. Run it with `make test-tools` when changing hooks or
their shared scripts.
