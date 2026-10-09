//! Optionality compatibility checks.

use interlint::compare::interface_diff::compare_interface_members;
use interlint::types::{ClassMember, ClassMemberKind, Visibility};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn interface_property_optional_to_required_fires_major() {
    let old = vec![make_property("x", true, false)];
    let new = vec![make_property("x", false, false)];
    let findings = compare_interface_members(&old, &new, "Iface", &test_file(), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "interface-property-made-required"),
        "IFT-01: expected interface-property-made-required. Got: {:?}",
        findings
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "interface-property-made-required")
        .unwrap();
    assert_eq!(
        f.severity,
        interlint::types::Severity::Major,
        "IFT-01: interface-property-made-required should be Major"
    );
}

#[test]
fn required_to_optional_does_not_fire() {
    let old = vec![make_property("x", false, false)];
    let new = vec![make_property("x", true, false)];
    let findings = compare_interface_members(&old, &new, "Iface", &test_file(), 1, 1);
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == "interface-property-made-required"),
        "IFT-01: required->optional should NOT fire. Got: {:?}",
        findings
    );
}

fn make_property(name: &str, is_optional: bool, is_readonly: bool) -> ClassMember {
    ClassMember {
        name: name.to_string(),
        kind: ClassMemberKind::Property,
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_readonly,
        param_count: 0,
        type_signature: None,
        is_optional,
        is_accessor: false,
        this_param_type: None,
        decorators: Vec::new(),
        is_private: false,
    }
}
