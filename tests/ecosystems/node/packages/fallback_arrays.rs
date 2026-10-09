//! Fallback arrays compatibility checks.

#[test]
fn exports_array_does_not_retry_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"exports":["./missing.js","./existing.js"]}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("existing.js"), "export const api = 1;").unwrap();
    let (exports, warnings) =
        interlint::ecosystems::node::discovery::discover_exports(dir.path()).unwrap();
    assert!(exports.is_empty());
    assert!(warnings
        .iter()
        .any(|w| w.incomplete && w.message.contains("missing.js")));
}
