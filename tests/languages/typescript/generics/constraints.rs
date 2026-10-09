//! Constraints compatibility checks.

use interlint::languages::javascript::model::{ClassMember, ClassMemberKind, Visibility};
use interlint::languages::typescript::compare::interfaces::compare_interface_members;
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn generic_constraint_tightened_no_constraint_to_constraint() {
    // Old: <T>(value: T) => void  (no constraint)
    // New: <T extends string>(value: T) => void  (constraint added)
    let old = vec![make_method("process", 1, Some("<T>(value: T) => void"))];
    let new = vec![make_method(
        "process",
        1,
        Some("<T extends string>(value: T) => void"),
    )];

    let findings = compare_interface_members(&old, &new, "MyInterface", &test_file(), 1, 0);

    assert!(
        findings.iter().any(|f| f.rule == "interface-generic-constraint-tightened"),
        "Adding extends constraint should fire interface-generic-constraint-tightened; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "interface-generic-constraint-tightened")
        .unwrap();
    assert_eq!(
        f.severity,
        interlint::core::findings::Severity::Major,
        "should be Major"
    );
    assert!(
        f.message.contains("process"),
        "message should mention method name; got: {}",
        f.message
    );
}

#[test]
fn generic_constraint_changed_fires_rule() {
    // Old: <T extends object>(value: T) => void
    // New: <T extends string>(value: T) => void
    let old = vec![make_method(
        "transform",
        1,
        Some("<T extends object>(value: T) => void"),
    )];
    let new = vec![make_method(
        "transform",
        1,
        Some("<T extends string>(value: T) => void"),
    )];

    let findings = compare_interface_members(&old, &new, "MyInterface", &test_file(), 1, 0);

    assert!(
        findings.iter().any(|f| f.rule == "interface-generic-constraint-tightened"),
        "Changing extends constraint should fire interface-generic-constraint-tightened; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}

#[test]
fn no_constraint_unchanged_no_finding() {
    let sig = "<T>(value: T) => void";
    let members = vec![make_method("process", 1, Some(sig))];
    let findings = compare_interface_members(&members, &members, "MyInterface", &test_file(), 1, 0);

    assert!(
        !findings
            .iter()
            .any(|f| f.rule == "interface-generic-constraint-tightened"),
        "Unchanged signature should not fire; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

fn make_method(name: &str, param_count: usize, type_signature: Option<&str>) -> ClassMember {
    ClassMember {
        name: name.to_string(),
        kind: ClassMemberKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_readonly: false,
        param_count,
        type_signature: type_signature.map(|s| s.to_string()),
        is_optional: false,
        is_accessor: false,
        this_param_type: None,
        decorators: Vec::new(),
        is_private: false,
    }
}
