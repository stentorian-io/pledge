//! Path aliases compatibility checks.

use pledge::languages::javascript::model::ExportKey;

#[test]
fn tsconfig_alias_resolution() {
    // Create a package with tsconfig.json having path aliases and use discover_exports.
    // The @/utils alias should resolve to src/utils.ts.
    let pkg_dir = tempfile::tempdir().unwrap();

    std::fs::create_dir_all(pkg_dir.path().join("src")).unwrap();

    // Create src/utils.ts with an exported function
    std::fs::write(
        pkg_dir.path().join("src/utils.ts"),
        b"export function helper() { return 42; }\n",
    )
    .unwrap();

    // Create index.ts that re-exports via alias
    std::fs::write(
        pkg_dir.path().join("index.ts"),
        b"export * from '@/utils';\n",
    )
    .unwrap();

    // Create tsconfig.json with path aliases
    let tsconfig = serde_json::json!({
        "compilerOptions": {
            "baseUrl": ".",
            "paths": {
                "@/*": ["src/*"]
            }
        }
    });
    std::fs::write(
        pkg_dir.path().join("tsconfig.json"),
        serde_json::to_string_pretty(&tsconfig).unwrap(),
    )
    .unwrap();

    // Create package.json pointing to index.ts
    let pkg = serde_json::json!({
        "name": "pkg",
        "version": "1.0.0",
        "main": "./index.ts"
    });
    std::fs::write(
        pkg_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();

    let result = pledge::ecosystems::node::discovery::discover_exports(pkg_dir.path());
    assert!(
        result.is_ok(),
        "discover_exports should not error with tsconfig path aliases; got: {:?}",
        result.err()
    );

    // If tsconfig alias resolved, 'helper' should appear in the export map
    let (export_map, _warnings) = result.unwrap();
    assert!(
        export_map.contains_key(&ExportKey::new_root_value("helper")),
        "tsconfig alias @/utils should resolve to src/utils.ts and export 'helper'; got: {:?}",
        export_map.keys().collect::<Vec<_>>()
    );
}
