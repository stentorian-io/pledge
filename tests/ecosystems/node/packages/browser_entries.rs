//! Browser entries compatibility checks.

use interlint::ecosystems::node::discovery::entry_points::resolve_entry_points;

#[test]
fn browser_field_contributes_entry_point() {
    let pkg_dir = tempfile::tempdir().unwrap();

    std::fs::write(pkg_dir.path().join("browser.js"), b"// browser entry point").unwrap();

    let pkg = serde_json::json!({
        "name": "mypkg",
        "version": "1.0.0",
        "browser": "./browser.js"
    });
    std::fs::write(
        pkg_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();

    let entry_points = resolve_entry_points(pkg_dir.path()).unwrap();
    let file_names: Vec<String> = entry_points
        .iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(
        file_names.contains(&"browser.js".to_string()),
        "browser field should contribute browser.js as entry point; got: {:?}",
        file_names
    );
}

#[test]
fn browser_field_combined_with_main() {
    let pkg_dir = tempfile::tempdir().unwrap();

    std::fs::write(pkg_dir.path().join("index.js"), b"// main").unwrap();
    std::fs::write(pkg_dir.path().join("browser.js"), b"// browser").unwrap();

    let pkg = serde_json::json!({
        "name": "mypkg",
        "version": "1.0.0",
        "main": "./index.js",
        "browser": "./browser.js"
    });
    std::fs::write(
        pkg_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();

    let entry_points = resolve_entry_points(pkg_dir.path()).unwrap();
    let file_names: Vec<String> = entry_points
        .iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(
        file_names.contains(&"index.js".to_string()),
        "main field should still be present; got: {:?}",
        file_names
    );
    assert!(
        file_names.contains(&"browser.js".to_string()),
        "browser field should also be present; got: {:?}",
        file_names
    );
}
