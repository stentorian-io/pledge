//! Assertions compatibility checks.

use pledge::languages::typescript::compare::refinements::compare_assertion;
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn assertion_dropped_fires_major() {
    let findings = compare_assertion(Some("Foo"), None, "isFoo", &test_file(), 1, 1);
    assert!(
        findings.iter().any(|f| f.rule == "assertion-widened"),
        "TLF-04: expected assertion-widened. Got: {:?}",
        findings
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "assertion-widened")
        .unwrap();
    assert_eq!(
        f.severity,
        pledge::core::findings::Severity::Major,
        "TLF-04: assertion-widened should be Major"
    );
}
