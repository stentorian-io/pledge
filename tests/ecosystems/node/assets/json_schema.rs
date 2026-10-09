//! Json schema compatibility checks.

use interlint::compare::json_diff;
use interlint::compare::subtype::{
    RULE_JSON_FIELD_ADDED, RULE_JSON_FIELD_REMOVED, RULE_JSON_REQUIRED_FIELD_ADDED,
};

use crate::fixtures::fixture_path;

#[test]
fn test_json_field_removed_integration() {
    let old = fixture_path("node/assets/json/old");
    let new = fixture_path("node/assets/json/new");
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
    let old = fixture_path("node/assets/json/old");
    let new = fixture_path("node/assets/json/new");
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
