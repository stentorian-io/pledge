//! Entry precedence compatibility checks.

use pledge::ecosystems::node::compare::metadata::compare_metadata;

#[test]
fn main_change_suppressed_when_exports_dot_stable() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let old_dir = dir.path().join("old");
    let new_dir = dir.path().join("new");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::create_dir_all(&new_dir).unwrap();

    // main changes, but exports["."] is stable
    std::fs::write(
        old_dir.join("package.json"),
        r#"{"name":"t","version":"1.0.0","main":"./old.js","exports":{".":"./stable.js"}}"#,
    )
    .unwrap();
    std::fs::write(
        new_dir.join("package.json"),
        r#"{"name":"t","version":"1.0.0","main":"./new.js","exports":{".":"./stable.js"}}"#,
    )
    .unwrap();

    let findings = compare_metadata(&old_dir, &new_dir);
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == "entry-point-changed" && f.message.to_lowercase().contains("main")),
        "SEM-06: main should be suppressed when exports[\".\"] stable. Got: {:?}",
        findings
    );
}
