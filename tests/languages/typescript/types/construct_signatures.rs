//! Construct signatures compatibility checks.

use interlint::compare::interface_diff::compare_construct_signatures;
use interlint::types::ConstructSignature;
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn construct_signature_removed_fires_major() {
    let old = vec![make_construct_sig(1, "Foo")];
    let new: Vec<ConstructSignature> = vec![];
    let findings = compare_construct_signatures(&old, &new, "Iface", &test_file(), 1, 1);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "construct-signature-removed")
            .count(),
        1,
        "TLF-01: expected one construct-signature-removed finding. Got: {:?}",
        findings
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "construct-signature-removed")
        .unwrap();
    assert_eq!(
        f.severity,
        interlint::types::Severity::Major,
        "TLF-01: construct-signature-removed should be Major. Got: {:?}",
        f.severity
    );
}

fn make_construct_sig(param_count: usize, ret: &str) -> ConstructSignature {
    ConstructSignature {
        param_count,
        return_text: Some(ret.to_string()),
        is_abstract: false,
    }
}
