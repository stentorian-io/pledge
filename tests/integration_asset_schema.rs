//! Integration tests for Phase 10 asset & schema comparison rules (AST-01..06).
//!
//! These tests exercise the full pipeline:
//!   JSON diff -> CSS diff -> asset diff -> Finding output
//!
//! Each test covers one or more of the 6 AST rule IDs:
//!   AST-01: json-field-removed
//!   AST-02: json-required-field-added
//!   AST-03: css-custom-property-removed
//!   AST-04: css-class-removed
//!   AST-05: asset-file-removed
//!   AST-06: json-field-added

use std::path::PathBuf;

use interlint::compare::subtype::{
    RULE_ASSET_FILE_REMOVED, RULE_CSS_CLASS_REMOVED, RULE_CSS_CUSTOM_PROPERTY_REMOVED,
    RULE_JSON_FIELD_ADDED, RULE_JSON_FIELD_REMOVED, RULE_JSON_REQUIRED_FIELD_ADDED,
};
use interlint::compare::{asset_diff, css_diff, json_diff};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn test_json_field_removed_integration() {
    let old = fixture_path("json-diff-pkg/old");
    let new = fixture_path("json-diff-pkg/new");
    let findings = json_diff::compare_json_files(&old, &new);
    // "age" field removed from schema.json -> AST-01
    assert!(
        findings.iter().any(|f| f.rule == RULE_JSON_FIELD_REMOVED),
        "Expected json-field-removed finding, got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

#[test]
fn test_json_required_field_added_integration() {
    let old = fixture_path("json-diff-pkg/old");
    let new = fixture_path("json-diff-pkg/new");
    let findings = json_diff::compare_json_files(&old, &new);
    // "email" added and in required array -> AST-02
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_JSON_REQUIRED_FIELD_ADDED),
        "Expected json-required-field-added finding, got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

#[test]
fn test_json_field_added_optional_integration() {
    // This test verifies AST-06 using a tempdir scenario.
    // The fixture's new schema.json adds "email" which IS in required,
    // so we need a separate scenario for optional field addition.
    let dir_old = tempfile::tempdir().unwrap();
    let dir_new = tempfile::tempdir().unwrap();
    std::fs::write(
        dir_old.path().join("package.json"),
        r#"{"name":"x","version":"1.0.0"}"#,
    )
    .unwrap();
    std::fs::write(
        dir_new.path().join("package.json"),
        r#"{"name":"x","version":"2.0.0"}"#,
    )
    .unwrap();
    std::fs::write(dir_old.path().join("data.json"), r#"{"a":1}"#).unwrap();
    std::fs::write(dir_new.path().join("data.json"), r#"{"a":1,"b":2}"#).unwrap();
    let findings = json_diff::compare_json_files(dir_old.path(), dir_new.path());
    assert!(
        findings.iter().any(|f| f.rule == RULE_JSON_FIELD_ADDED),
        "Expected json-field-added finding for optional field"
    );
}

#[test]
fn test_css_custom_property_removed_integration() {
    let old = fixture_path("css-diff-pkg/old");
    let new = fixture_path("css-diff-pkg/new");
    let findings = css_diff::compare_css_files(&old, &new);
    // --color-primary removed -> AST-03
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_CSS_CUSTOM_PROPERTY_REMOVED),
        "Expected css-custom-property-removed finding, got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

#[test]
fn test_css_class_removed_integration() {
    let old = fixture_path("css-diff-pkg/old");
    let new = fixture_path("css-diff-pkg/new");
    let findings = css_diff::compare_css_files(&old, &new);
    // .card removed -> AST-04
    assert!(
        findings.iter().any(|f| f.rule == RULE_CSS_CLASS_REMOVED),
        "Expected css-class-removed finding, got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

#[test]
fn test_asset_file_removed_integration() {
    let old = fixture_path("asset-diff-pkg/old");
    let new = fixture_path("asset-diff-pkg/new");
    let findings = asset_diff::compare_asset_files(&old, &new);
    // icon.png removed -> AST-05
    assert!(
        findings.iter().any(|f| f.rule == RULE_ASSET_FILE_REMOVED),
        "Expected asset-file-removed finding, got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

#[test]
fn test_orchestrator_combines_all_findings() {
    // Use tempdir with JSON + CSS + asset changes to verify compare_asset_diff
    let dir_old = tempfile::tempdir().unwrap();
    let dir_new = tempfile::tempdir().unwrap();
    // package.json
    std::fs::write(
        dir_old.path().join("package.json"),
        r#"{"name":"x","version":"1.0.0"}"#,
    )
    .unwrap();
    std::fs::write(
        dir_new.path().join("package.json"),
        r#"{"name":"x","version":"2.0.0"}"#,
    )
    .unwrap();
    // JSON change (field removed)
    std::fs::write(dir_old.path().join("data.json"), r#"{"a":1,"b":2}"#).unwrap();
    std::fs::write(dir_new.path().join("data.json"), r#"{"a":1}"#).unwrap();
    // CSS change (class removed)
    std::fs::write(dir_old.path().join("style.css"), ".foo { } .bar { }").unwrap();
    std::fs::write(dir_new.path().join("style.css"), ".foo { }").unwrap();
    // Asset removal
    std::fs::write(dir_old.path().join("logo.svg"), "<svg></svg>").unwrap();

    let findings = interlint::compare::compare_asset_diff(dir_old.path(), dir_new.path());

    let rules: Vec<&str> = findings.iter().map(|f| f.rule).collect();
    assert!(
        rules.contains(&RULE_JSON_FIELD_REMOVED),
        "Missing json-field-removed"
    );
    assert!(
        rules.contains(&RULE_CSS_CLASS_REMOVED),
        "Missing css-class-removed"
    );
    assert!(
        rules.contains(&RULE_ASSET_FILE_REMOVED),
        "Missing asset-file-removed"
    );
}
