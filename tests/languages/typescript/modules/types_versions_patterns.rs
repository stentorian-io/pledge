//! Types versions patterns compatibility checks.

use pledge::ecosystems::node::discovery::entry_points::resolve_entry_points;

#[test]
fn types_versions_wildcard_discovers_d_ts_files() {
    let pkg_dir = tempfile::tempdir().unwrap();

    // Create dist/types directory with .d.ts files
    std::fs::create_dir_all(pkg_dir.path().join("dist/types")).unwrap();
    std::fs::write(
        pkg_dir.path().join("dist/types/index.d.ts"),
        b"export declare function helper(): void;\n",
    )
    .unwrap();
    std::fs::write(
        pkg_dir.path().join("dist/types/utils.d.ts"),
        b"export declare function util(): string;\n",
    )
    .unwrap();
    std::fs::write(
        pkg_dir.path().join("index.js"),
        b"exports.helper = function() {};\n",
    )
    .unwrap();

    let pkg = serde_json::json!({
        "name": "mypkg",
        "version": "1.0.0",
        "main": "./index.js",
        "typesVersions": {
            ">=4.0": { "*": ["dist/types/*"] }
        }
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
        file_names.contains(&"index.d.ts".to_string()),
        "typesVersions wildcard should expand to include index.d.ts; got: {:?}",
        file_names
    );
    assert!(
        file_names.contains(&"utils.d.ts".to_string()),
        "typesVersions wildcard should expand to include utils.d.ts; got: {:?}",
        file_names
    );
}
