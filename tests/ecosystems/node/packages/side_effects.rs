//! Side effects compatibility checks.

use interlint::ecosystems::node::compare::metadata::compare_metadata;

#[test]
fn side_effects_false_to_true_fires_warning() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let old_dir = dir.path().join("old");
    let new_dir = dir.path().join("new");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::create_dir_all(&new_dir).unwrap();

    std::fs::write(
        old_dir.join("package.json"),
        r#"{"name":"t","version":"1.0.0","sideEffects":false}"#,
    )
    .unwrap();
    std::fs::write(
        new_dir.join("package.json"),
        r#"{"name":"t","version":"1.0.0","sideEffects":true}"#,
    )
    .unwrap();

    let findings = compare_metadata(&old_dir, &new_dir);
    assert!(
        findings.iter().any(|f| f.rule == "side-effects-widened"
            && matches!(f.severity, interlint::core::findings::Severity::Warning)),
        "META-01: expected side-effects-widened Warning. Got: {:?}",
        findings
    );
}
