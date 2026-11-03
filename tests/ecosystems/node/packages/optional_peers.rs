//! Optional peers compatibility checks.

use pledge::ecosystems::node::compare::metadata::compare_metadata;

#[test]
fn peer_dep_optional_removed_fires_major() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let old_dir = dir.path().join("old");
    let new_dir = dir.path().join("new");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::create_dir_all(&new_dir).unwrap();

    std::fs::write(
        old_dir.join("package.json"),
        r#"{"name":"t","version":"1.0.0","peerDependenciesMeta":{"react":{"optional":true}}}"#,
    )
    .unwrap();
    std::fs::write(
        new_dir.join("package.json"),
        r#"{"name":"t","version":"1.0.0","peerDependenciesMeta":{"react":{"optional":false}}}"#,
    )
    .unwrap();

    let findings = compare_metadata(&old_dir, &new_dir);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "peer-dep-optional-removed"),
        "META-02: expected peer-dep-optional-removed. Got: {:?}",
        findings
    );
}
