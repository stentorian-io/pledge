//! Findings compatibility checks.

use interlint::compare::{compare_exports, has_breaking_changes};
use interlint::discover::discover_exports;
use interlint::types::Severity;

use crate::fixtures::fixture_path;

#[test]
fn test_has_breaking_changes() {
    let (old, _) = discover_exports(&fixture_path("javascript/modules/basic/old")).unwrap();
    let (new, _) = discover_exports(&fixture_path("javascript/modules/basic/new")).unwrap();
    let findings = compare_exports(&old, &new, std::path::Path::new("."), true);
    assert!(
        has_breaking_changes(&findings),
        "should detect breaking changes when export removed"
    );
}

#[test]
fn test_findings_sorted_by_severity() {
    let (old, _) = discover_exports(&fixture_path("javascript/modules/basic/old")).unwrap();
    let (new, _) = discover_exports(&fixture_path("javascript/modules/basic/new")).unwrap();
    let findings = compare_exports(&old, &new, std::path::Path::new("."), true);
    // major findings should come before minor
    let first_major_idx = findings.iter().position(|f| f.severity == Severity::Major);
    let last_minor_idx = findings.iter().rposition(|f| f.severity == Severity::Minor);
    if let (Some(maj), Some(min)) = (first_major_idx, last_minor_idx) {
        assert!(maj < min, "major findings should be sorted before minor");
    }
}

#[test]
fn test_finding_line_numbers_are_1_indexed() {
    let (exports, _) = discover_exports(&fixture_path("javascript/modules/basic/old")).unwrap();
    for (key, sym) in &exports {
        let name = &key.name;
        assert!(
            sym.line >= 1,
            "line for '{}' should be >= 1 (1-indexed), got {}",
            name,
            sym.line
        );
        assert!(
            sym.col >= 1,
            "col for '{}' should be >= 1 (1-indexed), got {}",
            name,
            sym.col
        );
    }
}
