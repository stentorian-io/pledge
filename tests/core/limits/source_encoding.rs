//! Source encoding compatibility checks.

#[test]
fn invalid_utf8_does_not_abort() {
    use pledge::ecosystems::node::discovery::entry_points::resolve_entry_points;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let pkg = dir.path();
    std::fs::write(
        pkg.join("package.json"),
        r#"{"name":"t","version":"1.0.0","main":"./index.js"}"#,
    )
    .unwrap();
    // Write invalid UTF-8 bytes as the main file
    std::fs::write(pkg.join("index.js"), [0x80u8, 0x81u8, 0x82u8]).unwrap();

    // resolve_entry_points should succeed even with invalid UTF-8 in the main file
    let result = resolve_entry_points(pkg);
    assert!(
        result.is_ok(),
        "ROB-01: resolve_entry_points should not fail on invalid UTF-8. Got: {:?}",
        result
    );
}
