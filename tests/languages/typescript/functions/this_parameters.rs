//! This parameters compatibility checks.

use interlint::languages::typescript::compare::refinements::compare_this_param;
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn this_param_change_fires_major() {
    let findings = compare_this_param(Some("Foo"), Some("Bar"), "fn", &test_file(), 1, 1);
    assert!(
        findings.iter().any(|f| f.rule == "this-param-changed"),
        "TLF-03: expected this-param-changed. Got: {:?}",
        findings
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "this-param-changed")
        .unwrap();
    assert_eq!(
        f.severity,
        interlint::core::findings::Severity::Major,
        "TLF-03: this-param-changed should be Major"
    );
}

#[test]
fn this_param_removed_fires_major() {
    let findings = compare_this_param(Some("Foo"), None, "fn", &test_file(), 1, 1);
    assert!(
        findings.iter().any(|f| f.rule == "this-param-changed"),
        "TLF-03: removing this param should fire this-param-changed. Got: {:?}",
        findings
    );
}
