//! Files compatibility checks.

use crate::fixtures::fixture_path;
use interlint::ecosystems::node::compare::assets::files as asset_diff;
use interlint::ecosystems::node::compare::rules::{
    RULE_ASSET_FILE_REMOVED, RULE_CSS_CLASS_REMOVED, RULE_JSON_FIELD_REMOVED,
};

#[test]
fn test_asset_file_removed_integration() {
    let old = fixture_path("node/assets/files/old");
    let new = fixture_path("node/assets/files/new");
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

    let findings = interlint::ecosystems::node::compare::assets::compare_asset_diff(
        dir_old.path(),
        dir_new.path(),
    );

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
