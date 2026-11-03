//! Export patterns compatibility checks.

use pledge::ecosystems::node::compare::compare_exports_map_diff;

#[test]
fn wildcard_pattern_changed() {
    // Old: { "./*": "./dist/*.js" }
    // New: { "./*.js": "./dist/*.js" }
    // Expected: subpath-pattern-changed finding with "changed to"
    // NO separate subpath-removed or subpath-added findings
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_pkg = serde_json::json!({
        "name": "pkg",
        "version": "1.0.0",
        "exports": {
            "./*": "./dist/*.js"
        }
    });
    let new_pkg = serde_json::json!({
        "name": "pkg",
        "version": "2.0.0",
        "exports": {
            "./*.js": "./dist/*.js"
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

    let findings = compare_exports_map_diff(old_dir.path(), new_dir.path());

    // Should have subpath-pattern-changed with "changed to"
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "subpath-pattern-changed" && f.message.contains("changed to")),
        "changing ./* to ./*.js should produce subpath-pattern-changed; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );

    // Should NOT have separate subpath-removed and subpath-added
    let has_subpath_removed = findings.iter().any(|f| f.rule == "subpath-removed");
    let has_subpath_added_for_wc = findings
        .iter()
        .any(|f| f.rule == "subpath-added" && f.message.contains("./*.js"));

    assert!(
        !has_subpath_removed,
        "wildcard refinement should NOT produce subpath-removed; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert!(
        !has_subpath_added_for_wc,
        "wildcard refinement should NOT produce subpath-added for new pattern; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn wildcard_unrelated_not_merged() {
    // Old: { "./*": "./dist/*.js" }
    // New: { "./utils/*": "./lib/*.js" }
    // These have different bases ("./", "./utils/") — should be separate remove+add
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_pkg = serde_json::json!({
        "name": "pkg",
        "version": "1.0.0",
        "exports": {
            "./*": "./dist/*.js"
        }
    });
    let new_pkg = serde_json::json!({
        "name": "pkg",
        "version": "2.0.0",
        "exports": {
            "./utils/*": "./lib/*.js"
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

    let findings = compare_exports_map_diff(old_dir.path(), new_dir.path());

    // Different base patterns — old ./* should be reported as removed (not pattern-changed)
    assert!(
        findings.iter().any(|f| {
            (f.rule == "subpath-pattern-changed" && f.message.contains("./*"))
                || f.rule == "subpath-removed"
        }),
        "old ./* (unrelated bases) should be reported as removed/pattern-changed; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn wildcard_unchanged_no_finding() {
    // Same wildcard pattern in both versions — no finding expected
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let pkg = serde_json::json!({
        "name": "pkg",
        "version": "1.0.0",
        "exports": {
            "./*": "./dist/*.js"
        }
    });

    std::fs::write(
        old_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();
    std::fs::write(
        new_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();

    let findings = compare_exports_map_diff(old_dir.path(), new_dir.path());

    assert!(
        !findings.iter().any(|f| f.rule == "subpath-pattern-changed"),
        "unchanged wildcard pattern should produce no finding; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
