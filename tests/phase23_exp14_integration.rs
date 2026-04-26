//! Phase 23 EXP-14 integration tests: package.json `imports` field diff +
//! `#subpath` re-export tracing + exports/imports coexistence.

use std::path::{Path, PathBuf};

use interlint::compare::imports_diff;
use interlint::compare::subtype::{
    RULE_IMPORT_ADDED, RULE_IMPORT_CONDITION_CHANGED, RULE_IMPORT_PATTERN_REFINED,
    RULE_IMPORT_REMOVED, RULE_IMPORT_REROUTE,
};
use interlint::discover::discover_exports;
use interlint::types::Severity;

const FIXTURE_ROOT: &str = "tests/fixtures/phase-23";

fn fixture_dir(case: &str, side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_ROOT)
        .join(case)
        .join(side)
}

fn read_pkg(dir: &Path) -> serde_json::Value {
    let content = std::fs::read_to_string(dir.join("package.json"))
        .unwrap_or_else(|e| panic!("read {:?}: {}", dir.join("package.json"), e));
    serde_json::from_str(&content).expect("parse package.json")
}

#[test]
fn exp_14_imports_field_added_fires_minor() {
    let old = read_pkg(&fixture_dir("exp-14-added", "old"));
    let new = read_pkg(&fixture_dir("exp-14-added", "new"));
    let findings = imports_diff::compare_imports_diff(&old, &new);
    let added: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_IMPORT_ADDED)
        .collect();
    assert!(
        !added.is_empty(),
        "expected at least one import-added finding; got rules {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
    assert!(
        added.iter().all(|f| f.severity == Severity::Minor),
        "import-added severity must be Minor"
    );
}

#[test]
fn exp_14_imports_field_removed_fires_major() {
    let old = read_pkg(&fixture_dir("exp-14-removed", "old"));
    let new = read_pkg(&fixture_dir("exp-14-removed", "new"));
    let findings = imports_diff::compare_imports_diff(&old, &new);
    assert!(findings
        .iter()
        .any(|f| f.rule == RULE_IMPORT_REMOVED && f.severity == Severity::Major));
}

#[test]
fn exp_14_import_rerouted_fires_major() {
    let old = read_pkg(&fixture_dir("exp-14-rerouted", "old"));
    let new = read_pkg(&fixture_dir("exp-14-rerouted", "new"));
    let findings = imports_diff::compare_imports_diff(&old, &new);
    assert!(findings
        .iter()
        .any(|f| f.rule == RULE_IMPORT_REROUTE && f.severity == Severity::Major));
}

#[test]
fn exp_14_import_condition_removed_fires_major() {
    let old = read_pkg(&fixture_dir("exp-14-condition-changed", "old"));
    let new = read_pkg(&fixture_dir("exp-14-condition-changed", "new"));
    let findings = imports_diff::compare_imports_diff(&old, &new);
    assert!(findings
        .iter()
        .any(|f| f.rule == RULE_IMPORT_CONDITION_CHANGED && f.severity == Severity::Major));
}

#[test]
fn exp_14_import_pattern_refined_fires_major() {
    let old = read_pkg(&fixture_dir("exp-14-pattern-refined", "old"));
    let new = read_pkg(&fixture_dir("exp-14-pattern-refined", "new"));
    let findings = imports_diff::compare_imports_diff(&old, &new);
    assert!(findings
        .iter()
        .any(|f| f.rule == RULE_IMPORT_PATTERN_REFINED && f.severity == Severity::Major));
}

#[test]
fn exp_14_subpath_routed_through_imports_map() {
    let dir = fixture_dir("exp-14-subpath-tracing", "new");
    let (exports, warnings) = discover_exports(&dir).expect("discover_exports must not error");

    // Positive: `foo` was reached via #internal → src/internal.ts
    let has_foo = exports.iter().any(|(k, _)| k.name == "foo");
    assert!(
        has_foo,
        "expected `foo` reached through imports map; got exports: {:?}, warnings: {:?}",
        exports.keys().map(|k| &k.name).collect::<Vec<_>>(),
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    // Negative: NO synthetic *<#internal> entry
    let leaked: Vec<_> = exports
        .iter()
        .filter(|(k, _)| k.name.starts_with("*<#"))
        .collect();
    assert!(
        leaked.is_empty(),
        "imports specifier leaked as synthetic entry: {:?}",
        leaked.iter().map(|(k, _)| &k.name).collect::<Vec<_>>()
    );
}

#[test]
fn exp_14_imports_and_exports_coexist() {
    let dir = fixture_dir("exp-14-imports-with-exports", "new");
    let (exports, _warnings) = discover_exports(&dir).expect("discover_exports must not error");

    // The export `x` (from #util → src/util.ts) is reached via the imports map
    // even though the package ALSO has a top-level exports field. Both fields
    // coexist.
    let has_x = exports.iter().any(|(k, _)| k.name == "x");
    assert!(
        has_x,
        "expected `x` reached via imports map even with `exports` field present; got: {:?}",
        exports.keys().map(|k| &k.name).collect::<Vec<_>>()
    );
}

#[test]
fn exp_14_diff_orchestrator_appends_imports_findings() {
    // End-to-end through compare_exports_map_diff: an old_root with no
    // package.json fields and a new_root with both `exports` and `imports`
    // populated should produce findings from BOTH families.
    use std::fs;
    use tempfile::tempdir;

    let old_dir = tempdir().expect("tempdir");
    fs::write(
        old_dir.path().join("package.json"),
        r#"{ "name": "old", "version": "1.0.0" }"#,
    )
    .expect("write old pkg");

    let new_dir = fixture_dir("exp-14-imports-with-exports", "new");

    let findings = interlint::compare::compare_exports_map_diff(old_dir.path(), &new_dir);

    let rules: Vec<&'static str> = findings.iter().map(|f| f.rule).collect();
    assert!(
        rules.contains(&RULE_IMPORT_ADDED),
        "expected import-added when imports field appears in new; got {:?}",
        rules
    );
    // The new pkg also has an `exports` field that didn't exist in old, so
    // RULE_EXPORTS_FIELD_ADDED should also be present — proving both diffs ran.
    assert!(
        rules.contains(&"exports-field-added"),
        "expected exports-field-added to coexist with import-added; got {:?}",
        rules
    );
}

#[test]
fn exp_14_node_rhs_target_does_not_emit_warning() {
    // Negative-control for Pitfall §1: a #subpath whose imports map value is
    // a node:-prefixed builtin in one condition and a real file in another
    // must resolve to the real file and not emit a node:-mentioning warning.
    use std::fs;
    use tempfile::tempdir;

    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("package.json"),
        r##"{
            "name": "p23-exp14-node-rhs",
            "version": "1.0.0",
            "types": "./index.ts",
            "imports": {
                "#crypto": {
                    "node": "node:crypto",
                    "default": "./poly.ts"
                }
            }
        }"##,
    )
    .expect("write pkg");
    fs::write(
        dir.path().join("index.ts"),
        "export { hash } from '#crypto';\n",
    )
    .expect("write index");
    fs::write(
        dir.path().join("poly.ts"),
        "export const hash: () => string = () => 'x';\n",
    )
    .expect("write poly");

    let (exports, warnings) =
        discover_exports(dir.path()).expect("discover_exports must not error");

    // No node:-mentioning warning
    let node_warns: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("node:"))
        .collect();
    assert!(
        node_warns.is_empty(),
        "node:-RHS target leaked into ResolveWarning: {:?}",
        node_warns.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    // hash IS resolved through the default condition
    let has_hash = exports.iter().any(|(k, _)| k.name == "hash");
    assert!(
        has_hash,
        "expected `hash` resolved via default condition (./poly.ts); got: {:?}",
        exports.keys().map(|k| &k.name).collect::<Vec<_>>()
    );
}

#[test]
fn exp_14_imports_field_wins_over_tsconfig_paths() {
    // Phase 23 checker Warning 5 / RESEARCH Pitfall §5 / Open Q6 (RESOLVED):
    // when package.json `imports` AND tsconfig.json `paths` both map the same
    // `#alias`, `imports` wins. The structural property is enforced by the
    // classifier dispatch in resolver.rs — `#`-prefixed specifiers route to
    // SpecifierKind::ImportsField BEFORE the SpecifierKind::Relative | Bare
    // arm where tsconfig fallback runs. This test is the regression guard.
    let dir = fixture_dir("exp-14-tsconfig-imports-overlap", "new");
    let (exports, warnings) = discover_exports(&dir).expect("discover_exports must not error");

    let names: Vec<&str> = exports.keys().map(|k| k.name.as_str()).collect();

    // POSITIVE: the imports-mapped symbol resolves
    assert!(
        names.contains(&"fromImports"),
        "expected `fromImports` (resolved via imports field → src/shared-imports.js); \
         got exports: {:?}, warnings: {:?}",
        names,
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    // NEGATIVE: the tsconfig-mapped symbol must NOT resolve. If `fromTsconfig`
    // appears here, the classifier dispatch has been broken — `#shared`
    // somehow reached the Relative | Bare arm where tsconfig fallback runs.
    assert!(
        !names.contains(&"fromTsconfig"),
        "tsconfig.paths alias took precedence over imports field — Pitfall §5 \
         locked precedence violated. Got exports: {:?}",
        names
    );
}

#[test]
fn exp_14_deep_nesting_does_not_panic() {
    // Phase 23 checker Info 1 (optional): a hostile imports map JSON nested
    // beyond serde_json's default 128-level recursion bound must NOT panic
    // the parser; it must degrade gracefully (parse_imports_map_for_pkg
    // returns None via .ok()? chain → imports field treated as absent).
    use std::fs;
    use tempfile::tempdir;

    // Build a 200-level-deep nested JSON conditions object.
    let depth = 200;
    let mut deep = String::from("\"./leaf.js\"");
    for _ in 0..depth {
        deep = format!("{{ \"default\": {} }}", deep);
    }
    let pkg_json = format!(
        r##"{{ "name": "p23-deep", "version": "1.0.0", "types": "./index.ts", "imports": {{ "#deep": {} }} }}"##,
        deep
    );

    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("package.json"), &pkg_json).expect("write pkg");
    fs::write(dir.path().join("index.ts"), "export const k: number = 1;\n").expect("write index");

    // Must not panic. The contract is graceful degradation, not a specific
    // output shape. A deeply-nested JSON that exceeds serde_json's recursion
    // limit will cause entry_points to return Err (acceptable — the package
    // itself is malformed). What MUST NOT happen is a stack overflow / panic.
    // So we simply call discover_exports and ignore the result.
    let _result = discover_exports(dir.path());
    // If we reach here without a panic, the test passes.
}
