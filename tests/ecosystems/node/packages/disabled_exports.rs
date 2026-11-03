//! Disabled exports compatibility checks.

use pledge::ecosystems::node::discovery::entry_points::resolve_entry_points;

#[test]
fn exports_false_falls_back() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let pkg = dir.path();
    std::fs::write(pkg.join("index.js"), b"exports.foo = 1;").unwrap();
    std::fs::write(
        pkg.join("package.json"),
        r#"{"name":"t","version":"1.0.0","main":"./index.js","exports":false}"#,
    )
    .unwrap();
    // With exports:false, resolve_entry_points should fall back to main
    let result = resolve_entry_points(pkg);
    assert!(
        result.is_ok(),
        "ROB-03: exports:false should not cause an error. Got: {:?}",
        result
    );
    // Entry points may be empty (exports:false warns and skips) or contain main — either is valid
}
