//! Types versions compatibility checks.

use pledge::ecosystems::node::compare::metadata::compare_metadata;

#[test]
fn types_versions_removed_finding() {
    // Old has typesVersions: { ">=4.0": { "*": ["dist/types/*"] } }
    // New has no typesVersions
    // Expected: types-versions-changed Major finding
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_pkg = serde_json::json!({
        "name": "pkg",
        "version": "1.0.0",
        "typesVersions": {
            ">=4.0": { "*": ["dist/types/*"] }
        }
    });
    let new_pkg = serde_json::json!({
        "name": "pkg",
        "version": "2.0.0"
    });

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

    let findings = compare_metadata(old_dir.path(), new_dir.path());

    assert!(
        findings.iter().any(|f| f.rule == "types-versions-changed"),
        "removing typesVersions should produce types-versions-changed finding; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn types_versions_range_removed_finding() {
    // Old has typesVersions with two ranges; New has only one
    // Expected: types-versions-changed finding for the removed range
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_pkg = serde_json::json!({
        "name": "pkg",
        "version": "1.0.0",
        "typesVersions": {
            ">=4.0": { "*": ["dist/types/*"] },
            ">=3.0": { "*": ["dist/types-legacy/*"] }
        }
    });
    let new_pkg = serde_json::json!({
        "name": "pkg",
        "version": "2.0.0",
        "typesVersions": {
            ">=4.0": { "*": ["dist/types/*"] }
        }
    });

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

    let findings = compare_metadata(old_dir.path(), new_dir.path());

    assert!(
        findings.iter().any(|f| f.rule == "types-versions-changed" && f.message.contains(">=3.0")),
        "removing typesVersions range '>=3.0' should produce types-versions-changed; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}

#[test]
fn types_versions_entry_discovery() {
    // Create a temp package with typesVersions field and corresponding files.
    // discover_exports should run without errors.
    let pkg_dir = tempfile::tempdir().unwrap();

    // Create the package structure
    std::fs::create_dir_all(pkg_dir.path().join("dist/types")).unwrap();
    std::fs::write(
        pkg_dir.path().join("dist/types/index.d.ts"),
        b"export declare function helper(): void;\n",
    )
    .unwrap();
    std::fs::write(
        pkg_dir.path().join("index.js"),
        b"exports.helper = function() {};\n",
    )
    .unwrap();

    let pkg = serde_json::json!({
        "name": "pkg",
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

    // discover_exports should complete without errors
    let result = pledge::ecosystems::node::discovery::discover_exports(pkg_dir.path());
    assert!(
        result.is_ok(),
        "discover_exports should not error on package with typesVersions; got: {:?}",
        result.err()
    );
}
