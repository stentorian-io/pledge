//! Disabled targets compatibility checks.

use interlint::ecosystems::node::compare::metadata::compare_metadata;

#[test]
fn null_to_real_target_fires_minor_not_major() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let old_dir = dir.path().join("old");
    let new_dir = dir.path().join("new");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::create_dir_all(&new_dir).unwrap();

    std::fs::write(
        old_dir.join("package.json"),
        r#"{"name":"t","version":"1.0.0","exports":{".":{"types":null,"default":"./index.js"}}}"#,
    )
    .unwrap();
    std::fs::write(
        new_dir.join("package.json"),
        r#"{"name":"t","version":"1.0.0","exports":{".":{"types":"./index.d.ts","default":"./index.js"}}}"#,
    )
    .unwrap();

    let findings = compare_metadata(&old_dir, &new_dir);
    // null -> real should not fire Major condition-target-changed
    assert!(
        !findings.iter().any(|f| {
            f.rule == "condition-target-changed"
                && matches!(f.severity, interlint::core::findings::Severity::Major)
        }),
        "SEM-02: null -> real should not fire Major condition-target-changed. Got: {:?}",
        findings
    );
}
