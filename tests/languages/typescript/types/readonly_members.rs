//! Readonly members compatibility checks.

use interlint::compare::interface_diff::compare_interface_members;
use interlint::types::{ClassMember, ClassMemberKind, Visibility};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn readonly_added_fires_rule() {
    // Old: count: number (not readonly)
    // New: readonly count: number
    let old = vec![make_property("count", false)];
    let new = vec![make_property("count", true)];

    let findings = compare_interface_members(&old, &new, "MyInterface", &test_file(), 1, 0);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "interface-readonly-changed"),
        "Adding readonly should fire interface-readonly-changed; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "interface-readonly-changed")
        .unwrap();
    assert_eq!(
        f.severity,
        interlint::types::Severity::Major,
        "should be Major"
    );
    assert!(
        f.message.contains("added"),
        "message should say 'added'; got: {}",
        f.message
    );
    assert!(
        f.message.contains("count"),
        "message should mention property name; got: {}",
        f.message
    );
}

#[test]
fn readonly_removed_fires_rule() {
    // Old: readonly count: number
    // New: count: number (readonly removed)
    let old = vec![make_property("count", true)];
    let new = vec![make_property("count", false)];

    let findings = compare_interface_members(&old, &new, "MyInterface", &test_file(), 1, 0);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "interface-readonly-changed"),
        "Removing readonly should fire interface-readonly-changed; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "interface-readonly-changed")
        .unwrap();
    assert_eq!(
        f.severity,
        interlint::types::Severity::Major,
        "should be Major"
    );
    assert!(
        f.message.contains("removed"),
        "message should say 'removed'; got: {}",
        f.message
    );
}

#[test]
fn readonly_unchanged_no_finding() {
    // Both sides readonly — no finding expected
    let members = vec![make_property("x", true)];
    let findings = compare_interface_members(&members, &members, "MyInterface", &test_file(), 1, 0);

    assert!(
        !findings
            .iter()
            .any(|f| f.rule == "interface-readonly-changed"),
        "Unchanged readonly should not fire; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

fn make_property(name: &str, is_readonly: bool) -> ClassMember {
    ClassMember {
        name: name.to_string(),
        kind: ClassMemberKind::Property,
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_readonly,
        param_count: 0,
        type_signature: None,
        is_optional: false,
        is_accessor: false,
        this_param_type: None,
        decorators: Vec::new(),
        is_private: false,
    }
}
