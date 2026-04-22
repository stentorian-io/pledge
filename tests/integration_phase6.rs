//! Integration tests for Phase 6 exports diff rules (EXP-01..13).

use interlint::compare::exports_diff;
use interlint::compare::metadata;
use interlint::compare::subtype::{
    RULE_BIN_ENTRY_CHANGED, RULE_CJS_TO_ESM, RULE_CONDITION_REMOVED, RULE_CONDITION_TARGET_CHANGED,
    RULE_ESM_TO_CJS, RULE_EXPORTS_FIELD_ADDED, RULE_IMPORTABLE_FILE_REMOVED,
    RULE_PLATFORM_NARROWED, RULE_SUBPATH_ADDED, RULE_SUBPATH_PATTERN_CHANGED, RULE_SUBPATH_REMOVED,
    RULE_TYPES_CONDITION_REMOVED, RULE_TYPES_RUNTIME_MISMATCH,
};
use interlint::types::Severity;
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn read_pkg(path: &PathBuf) -> serde_json::Value {
    let content = std::fs::read_to_string(path.join("package.json")).unwrap();
    serde_json::from_str(&content).unwrap()
}

fn rule_ids(findings: &[interlint::types::Finding]) -> Vec<&'static str> {
    findings.iter().map(|f| f.rule).collect()
}

#[test]
fn test_exports_diff_subpath_removed() {
    // EXP-01: ./removed subpath removed
    let old_path = fixture_path("exports-diff-pkg/old");
    let new_path = fixture_path("exports-diff-pkg/new");
    let old_pkg = read_pkg(&old_path);
    let new_pkg = read_pkg(&new_path);
    let findings = exports_diff::compare_exports_diff(&old_pkg, &new_pkg, &old_path, &new_path);
    let rules = rule_ids(&findings);
    assert!(
        rules.contains(&RULE_SUBPATH_REMOVED),
        "should detect removed subpath, got: {:?}",
        rules
    );
}

#[test]
fn test_exports_field_added() {
    // EXP-02: old has no exports, new adds exports
    let old_path = fixture_path("no-exports-to-exports-pkg/old");
    let new_path = fixture_path("no-exports-to-exports-pkg/new");
    let old_pkg = read_pkg(&old_path);
    let new_pkg = read_pkg(&new_path);
    let findings = exports_diff::compare_exports_diff(&old_pkg, &new_pkg, &old_path, &new_path);
    let rules = rule_ids(&findings);
    assert!(
        rules.contains(&RULE_EXPORTS_FIELD_ADDED),
        "should detect exports field added, got: {:?}",
        rules
    );
    let exp02: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_EXPORTS_FIELD_ADDED)
        .collect();
    assert!(
        exp02.iter().all(|f| f.severity == Severity::Major),
        "exports-field-added should be Major"
    );
}

#[test]
fn test_exports_diff_condition_removed() {
    // EXP-03: require condition removed from "."
    let old_path = fixture_path("exports-diff-pkg/old");
    let new_path = fixture_path("exports-diff-pkg/new");
    let old_pkg = read_pkg(&old_path);
    let new_pkg = read_pkg(&new_path);
    let findings = exports_diff::compare_exports_diff(&old_pkg, &new_pkg, &old_path, &new_path);
    let rules = rule_ids(&findings);
    assert!(
        rules.contains(&RULE_CONDITION_REMOVED),
        "should detect removed condition, got: {:?}",
        rules
    );
}

#[test]
fn test_exports_diff_condition_target_changed() {
    // EXP-04: ./utils import target changed
    let old_path = fixture_path("exports-diff-pkg/old");
    let new_path = fixture_path("exports-diff-pkg/new");
    let old_pkg = read_pkg(&old_path);
    let new_pkg = read_pkg(&new_path);
    let findings = exports_diff::compare_exports_diff(&old_pkg, &new_pkg, &old_path, &new_path);
    let rules = rule_ids(&findings);
    assert!(
        rules.contains(&RULE_CONDITION_TARGET_CHANGED),
        "should detect changed target, got: {:?}",
        rules
    );
}

#[test]
fn test_types_condition_removed() {
    // EXP-05: types condition removed from ./utils in format-migration-pkg
    let old_path = fixture_path("format-migration-pkg/old");
    let new_path = fixture_path("format-migration-pkg/new");
    let old_pkg = read_pkg(&old_path);
    let new_pkg = read_pkg(&new_path);
    let findings = exports_diff::compare_exports_diff(&old_pkg, &new_pkg, &old_path, &new_path);
    let rules = rule_ids(&findings);
    assert!(
        rules.contains(&RULE_TYPES_CONDITION_REMOVED),
        "should detect types condition removed, got: {:?}",
        rules
    );
    // Verify EXP-05 does not double-fire as EXP-03 for the same "types" condition
    let types_removed: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains("types") && f.message.contains("./utils"))
        .collect();
    assert!(
        !types_removed
            .iter()
            .any(|f| f.rule == RULE_CONDITION_REMOVED),
        "types condition removal should NOT also fire condition-removed (EXP-03)"
    );
}

#[test]
fn test_subpath_pattern_changed() {
    // EXP-06: ./features/*.js wildcard pattern removed
    let old_path = fixture_path("format-migration-pkg/old");
    let new_path = fixture_path("format-migration-pkg/new");
    let old_pkg = read_pkg(&old_path);
    let new_pkg = read_pkg(&new_path);
    let findings = exports_diff::compare_exports_diff(&old_pkg, &new_pkg, &old_path, &new_path);
    let rules = rule_ids(&findings);
    assert!(
        rules.contains(&RULE_SUBPATH_PATTERN_CHANGED),
        "should detect subpath pattern changed, got: {:?}",
        rules
    );
    // Should NOT fire as subpath-removed (EXP-01) for wildcard patterns
    let pattern_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains("features"))
        .collect();
    assert!(
        !pattern_findings
            .iter()
            .any(|f| f.rule == RULE_SUBPATH_REMOVED),
        "wildcard pattern removal should fire subpath-pattern-changed, NOT subpath-removed"
    );
}

#[test]
fn test_exports_diff_subpath_added() {
    // EXP-07: ./new-feature added (Minor)
    let old_path = fixture_path("exports-diff-pkg/old");
    let new_path = fixture_path("exports-diff-pkg/new");
    let old_pkg = read_pkg(&old_path);
    let new_pkg = read_pkg(&new_path);
    let findings = exports_diff::compare_exports_diff(&old_pkg, &new_pkg, &old_path, &new_path);
    let added: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_SUBPATH_ADDED)
        .collect();
    assert!(
        !added.is_empty(),
        "should detect added subpath, got rules: {:?}",
        rule_ids(&findings)
    );
    assert!(
        added.iter().all(|f| f.severity == Severity::Minor),
        "added subpath should be Minor"
    );
}

#[test]
fn test_exports_diff_cjs_to_esm() {
    // EXP-08: require condition removed from "." root in exports-diff-pkg
    let old_path = fixture_path("exports-diff-pkg/old");
    let new_path = fixture_path("exports-diff-pkg/new");
    let old_pkg = read_pkg(&old_path);
    let new_pkg = read_pkg(&new_path);
    let findings = exports_diff::compare_exports_diff(&old_pkg, &new_pkg, &old_path, &new_path);
    let rules = rule_ids(&findings);
    assert!(
        rules.contains(&RULE_CJS_TO_ESM),
        "should detect CJS-to-ESM migration, got: {:?}",
        rules
    );
}

#[test]
fn test_esm_to_cjs() {
    // EXP-09: import condition removed from "." in format-migration-pkg
    let old_path = fixture_path("format-migration-pkg/old");
    let new_path = fixture_path("format-migration-pkg/new");
    let old_pkg = read_pkg(&old_path);
    let new_pkg = read_pkg(&new_path);
    let findings = exports_diff::compare_exports_diff(&old_pkg, &new_pkg, &old_path, &new_path);
    let rules = rule_ids(&findings);
    assert!(
        rules.contains(&RULE_ESM_TO_CJS),
        "should detect ESM-to-CJS migration, got: {:?}",
        rules
    );
}

#[test]
fn test_types_runtime_mismatch() {
    // EXP-10: ./api has types pointing to .d.cts (CJS) but import condition (ESM)
    let old_path = fixture_path("format-migration-pkg/old");
    let new_path = fixture_path("format-migration-pkg/new");
    let old_pkg = read_pkg(&old_path);
    let new_pkg = read_pkg(&new_path);
    let findings = exports_diff::compare_exports_diff(&old_pkg, &new_pkg, &old_path, &new_path);
    let rules = rule_ids(&findings);
    assert!(
        rules.contains(&RULE_TYPES_RUNTIME_MISMATCH),
        "should detect types/runtime mismatch, got: {:?}",
        rules
    );
}

#[test]
fn test_exports_diff_bin_entry_changed() {
    // EXP-11: helper bin entry removed
    let old_path = fixture_path("exports-diff-pkg/old");
    let new_path = fixture_path("exports-diff-pkg/new");
    let findings = metadata::compare_metadata(&old_path, &new_path);
    let rules = rule_ids(&findings);
    assert!(
        rules.contains(&RULE_BIN_ENTRY_CHANGED),
        "should detect removed bin entry, got: {:?}",
        rules
    );
}

#[test]
fn test_exports_diff_platform_narrowed() {
    // EXP-12: win32 removed from os
    let old_path = fixture_path("exports-diff-pkg/old");
    let new_path = fixture_path("exports-diff-pkg/new");
    let findings = metadata::compare_metadata(&old_path, &new_path);
    let rules = rule_ids(&findings);
    assert!(
        rules.contains(&RULE_PLATFORM_NARROWED),
        "should detect platform narrowing, got: {:?}",
        rules
    );
}

#[test]
fn test_no_exports_fallback_importable_file_removed() {
    // EXP-13: no exports field, file removed from packed output
    let old_path = fixture_path("no-exports-pkg/old");
    let new_path = fixture_path("no-exports-pkg/new");
    let old_pkg = read_pkg(&old_path);
    let new_pkg = read_pkg(&new_path);
    let findings = exports_diff::compare_exports_diff(&old_pkg, &new_pkg, &old_path, &new_path);
    let rules = rule_ids(&findings);
    assert!(
        rules.contains(&RULE_IMPORTABLE_FILE_REMOVED),
        "should detect removed importable file, got: {:?}",
        rules
    );
}

#[test]
fn test_exports_findings_have_package_json_path() {
    // All exports diff findings should reference package.json
    let old_path = fixture_path("exports-diff-pkg/old");
    let new_path = fixture_path("exports-diff-pkg/new");
    let old_pkg = read_pkg(&old_path);
    let new_pkg = read_pkg(&new_path);
    let findings = exports_diff::compare_exports_diff(&old_pkg, &new_pkg, &old_path, &new_path);
    for f in &findings {
        assert_eq!(
            f.file,
            PathBuf::from("package.json"),
            "finding {} should use package.json path",
            f.rule
        );
        assert_eq!(f.line, 1);
        assert_eq!(f.col, 1);
    }
}
