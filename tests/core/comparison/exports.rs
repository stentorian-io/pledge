//! Exports compatibility checks.

use crate::fixtures::fixture_path;
use interlint::core::findings::Severity;
use interlint::ecosystems::node::discovery::discover_exports;
use interlint::languages::javascript::compare::compare_exports;

#[test]
fn test_compare_detects_removed_export() {
    let (old, _) = discover_exports(&fixture_path("javascript/modules/basic/old")).unwrap();
    let (new, _) = discover_exports(&fixture_path("javascript/modules/basic/new")).unwrap();
    let findings = compare_exports(&old, &new, std::path::Path::new("."), true);
    let removed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "export-removed")
        .collect();
    assert_eq!(removed.len(), 1, "should detect 1 removed export");
    assert_eq!(removed[0].severity, Severity::Major);
    assert!(
        removed[0].message.contains("farewell"),
        "message should mention 'farewell', got: {}",
        removed[0].message
    );
}

#[test]
fn test_compare_detects_added_export() {
    let (old, _) = discover_exports(&fixture_path("javascript/modules/basic/old")).unwrap();
    let (new, _) = discover_exports(&fixture_path("javascript/modules/basic/new")).unwrap();
    let findings = compare_exports(&old, &new, std::path::Path::new("."), true);
    let added: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "export-added")
        .collect();
    assert_eq!(added.len(), 1, "should detect 1 added export");
    assert_eq!(added[0].severity, Severity::Minor);
    assert!(
        added[0].message.contains("newFunc"),
        "message should mention 'newFunc', got: {}",
        added[0].message
    );
}

#[test]
fn test_compare_identical_no_findings() {
    let (old, _) = discover_exports(&fixture_path("javascript/modules/basic/old")).unwrap();
    let (new, _) = discover_exports(&fixture_path("javascript/modules/basic/old")).unwrap();
    let findings = compare_exports(&old, &new, std::path::Path::new("."), true);
    assert!(
        findings.is_empty(),
        "identical exports should produce no findings"
    );
}
