//! Json schema compatibility checks.

use crate::fixtures::fixture_path;
use interlint::core::findings::{Finding, Severity};
use interlint::ecosystems::node::compare::assets::json_schema as json_diff;
use interlint::ecosystems::node::compare::rules::{
    RULE_JSON_FIELD_ADDED, RULE_JSON_FIELD_REMOVED, RULE_JSON_REQUIRED_FIELD_ADDED,
};
use serde_json::{json, Value};

fn compare_schema(old: Value, new: Value) -> Vec<Finding> {
    let before = tempfile::tempdir().unwrap();
    let after = tempfile::tempdir().unwrap();
    std::fs::write(before.path().join("schema.json"), old.to_string()).unwrap();
    std::fs::write(after.path().join("schema.json"), new.to_string()).unwrap();
    json_diff::compare_json_files(before.path(), after.path())
}

#[test]
fn making_an_existing_schema_property_required_is_breaking() {
    let old = json!({"type":"object", "properties":{"email":{"type":"string"}}, "required":[]});
    let mut new = old.clone();
    new["required"] = json!(["email"]);

    let findings = compare_schema(old, new);

    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].rule, RULE_JSON_REQUIRED_FIELD_ADDED);
    assert_eq!(findings[0].severity, Severity::Major);
    assert!(findings[0].message.contains("email"));
}

#[test]
fn adding_a_required_schema_property_is_breaking() {
    let old = json!({"type":"object", "properties":{}});
    let new =
        json!({"type":"object", "properties":{"email":{"type":"string"}}, "required":["email"]});

    let findings = compare_schema(old, new);

    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == RULE_JSON_REQUIRED_FIELD_ADDED
                && finding.severity == Severity::Major),
        "{findings:?}"
    );
}

#[test]
fn relaxing_or_reordering_schema_requirements_is_compatible() {
    let old = json!({"type":"object", "properties":{"email":{"type":"string"}, "name":{"type":"string"}}, "required":["email", "name"]});
    for required in [Some(json!(["name", "email"])), Some(json!(["email"])), None] {
        let mut new = old.clone();
        match required {
            Some(required) => new["required"] = required,
            None => {
                new.as_object_mut().unwrap().remove("required");
            }
        }

        let findings = compare_schema(old.clone(), new);

        assert!(
            !findings
                .iter()
                .any(|finding| finding.severity == Severity::Major),
            "{findings:?}"
        );
    }
}

#[test]
fn nested_schema_requirements_are_compared() {
    let old = json!({"type":"object", "properties":{"profile":{"type":"object", "properties":{"email":{"type":"string"}}}}});
    let mut new = old.clone();
    new["properties"]["profile"]["required"] = json!(["email"]);

    let findings = compare_schema(old, new);

    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == RULE_JSON_REQUIRED_FIELD_ADDED
                && finding.message.contains("properties.profile.email")),
        "{findings:?}"
    );
}

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
