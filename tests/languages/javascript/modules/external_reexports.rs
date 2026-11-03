//! External reexports compatibility checks.

use pledge::languages::javascript::compare::compare_exports;
use pledge::languages::javascript::model::ExportKey;

#[test]
fn external_reexport_synthetic_key() {
    // export * from 'lodash' should produce a synthetic *<lodash> entry in the export map.
    // The synthetic key is created in collect_exports (resolver), not parse_exports.
    // Use discover_exports on a temp package.
    let pkg_dir = tempfile::tempdir().unwrap();

    std::fs::write(
        pkg_dir.path().join("index.ts"),
        b"export * from 'lodash';\nexport function foo() {}\n",
    )
    .unwrap();

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

    let (map, _) = pledge::ecosystems::node::discovery::discover_exports(pkg_dir.path()).unwrap();

    // The export map should contain *<lodash> for the external re-export
    assert!(
        map.contains_key(&ExportKey::new_root_value("*<lodash>")),
        "export * from 'lodash' should produce synthetic *<lodash> entry; got: {:?}",
        map.keys().collect::<Vec<_>>()
    );
}

#[test]
fn external_reexport_removal() {
    // Old: export * from 'lodash' + export function foo() {}
    // New: export function foo() {}
    // Expected: export-removed finding for *<lodash>
    // Use discover_exports to get the real ExportMap with synthetic keys.
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_pkg = serde_json::json!({"name": "pkg", "version": "1.0.0", "main": "./index.ts"});
    let new_pkg = serde_json::json!({"name": "pkg", "version": "2.0.0", "main": "./index.ts"});

    std::fs::write(
        old_dir.path().join("index.ts"),
        b"export * from 'lodash';\nexport function foo() {}\n",
    )
    .unwrap();
    std::fs::write(
        new_dir.path().join("index.ts"),
        b"export function foo() {}\n",
    )
    .unwrap();
    std::fs::write(
        old_dir.path().join("package.json"),
        serde_json::to_string_pretty(&old_pkg).unwrap(),
    )
    .unwrap();
    std::fs::write(
        new_dir.path().join("package.json"),
        serde_json::to_string_pretty(&new_pkg).unwrap(),
    )
    .unwrap();

    let (old_map, _) =
        pledge::ecosystems::node::discovery::discover_exports(old_dir.path()).unwrap();
    let (new_map, _) =
        pledge::ecosystems::node::discovery::discover_exports(new_dir.path()).unwrap();

    // Verify the synthetic key is present in old
    assert!(
        old_map.contains_key(&ExportKey::new_root_value("*<lodash>")),
        "old exports should contain *<lodash>; got: {:?}",
        old_map.keys().collect::<Vec<_>>()
    );

    let findings = compare_exports(&old_map, &new_map, std::path::Path::new("."), true);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "export-removed" && f.message.contains("*<lodash>")),
        "removing external re-export from 'lodash' should produce export-removed; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn external_reexport_is_reexport_flag() {
    // export * from 'lodash' should set is_reexport = true on the synthetic entry.
    // is_reexport is set during collect_exports (discover_exports), not in parse_exports.
    // Use discover_exports via a temp dir.
    let pkg_dir = tempfile::tempdir().unwrap();

    std::fs::write(
        pkg_dir.path().join("index.ts"),
        b"export * from 'lodash';\nexport function foo() {}\n",
    )
    .unwrap();

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
        "discover_exports should succeed; got: {:?}",
        result.err()
    );
    let (map, _warnings) = result.unwrap();

    let lodash_entry = map.get(&ExportKey::new_root_value("*<lodash>"));
    assert!(
        lodash_entry.is_some(),
        "discover_exports should produce *<lodash> for external re-export; got: {:?}",
        map.keys().collect::<Vec<_>>()
    );
    assert!(
        lodash_entry.unwrap().is_reexport,
        "*<lodash> entry should have is_reexport = true"
    );
}
