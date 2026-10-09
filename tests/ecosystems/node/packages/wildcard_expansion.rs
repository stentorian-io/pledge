//! Wildcard expansion compatibility checks.

use interlint::ecosystems::node::discovery::entry_points::resolve_entry_points;

#[test]
fn subpath_wildcard_expansion() {
    let pkg_dir = tempfile::tempdir().unwrap();

    // Create dist directory with JS files
    std::fs::create_dir_all(pkg_dir.path().join("dist")).unwrap();
    std::fs::write(pkg_dir.path().join("dist/alpha.js"), b"// alpha").unwrap();
    std::fs::write(pkg_dir.path().join("dist/beta.js"), b"// beta").unwrap();

    let pkg = serde_json::json!({
        "name": "mypkg",
        "version": "1.0.0",
        "exports": {
            "./*": "./dist/*.js"
        }
    });
    std::fs::write(
        pkg_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();

    let entry_points = resolve_entry_points(pkg_dir.path()).unwrap();

    let paths: Vec<String> = entry_points
        .iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(
        paths.contains(&"alpha.js".to_string()),
        "subpath wildcard ./* should expand to include alpha.js; got: {:?}",
        paths
    );
    assert!(
        paths.contains(&"beta.js".to_string()),
        "subpath wildcard ./* should expand to include beta.js; got: {:?}",
        paths
    );
}
