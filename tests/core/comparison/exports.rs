//! Exports compatibility checks.

use crate::fixtures::fixture_path;
use pledge::core::findings::Severity;
use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare::compare_exports;

#[test]
fn test_compare_detects_removed_export() {
    let (old, _) = discover_exports(&fixture_path("javascript/modules/basic/old")).unwrap();
    let (new, _) = discover_exports(&fixture_path("javascript/modules/basic/new")).unwrap();
    let findings = compare_exports(&old, &new, std::path::Path::new("."), true);
    let removed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "export-removed")
        .collect();
    assert_eq!(removed.len(), 2, "root and ./index.js both lose farewell");
    assert!(
        removed
            .iter()
            .all(|finding| finding.severity == Severity::Major
                && finding.message.contains("farewell")),
        "{removed:?}"
    );
    assert_eq!(
        removed
            .iter()
            .filter(|finding| finding.message.contains("./index.js"))
            .count(),
        1
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
    assert_eq!(added.len(), 2, "root and ./index.js both gain newFunc");
    assert!(
        added
            .iter()
            .all(|finding| finding.severity == Severity::Minor
                && finding.message.contains("newFunc")),
        "{added:?}"
    );
    assert_eq!(
        added
            .iter()
            .filter(|finding| finding.message.contains("./index.js"))
            .count(),
        1
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
