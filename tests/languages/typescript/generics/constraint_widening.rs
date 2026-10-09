//! Constraint widening compatibility checks.

use interlint::compare::interface_diff::compare_interface_members;
use interlint::types::{ClassMember, ClassMemberKind, Visibility};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn loosening_constraint_no_tightening_finding() {
    // Old: fn<T extends string>(x: T): void
    // New: fn<T extends string | number>(x: T): void  (loosening)
    // Assert no interface-generic-constraint-tightened finding.
    let old = vec![ClassMember {
        name: "fn".to_string(),
        kind: ClassMemberKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_readonly: false,
        param_count: 1,
        type_signature: Some("<T extends string>(x: T) => void".to_string()),
        is_optional: false,
        is_accessor: false,
        this_param_type: None,
        decorators: Vec::new(),
        is_private: false,
    }];
    let new = vec![ClassMember {
        name: "fn".to_string(),
        kind: ClassMemberKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_readonly: false,
        param_count: 1,
        type_signature: Some("<T extends string | number>(x: T) => void".to_string()),
        is_optional: false,
        is_accessor: false,
        this_param_type: None,
        decorators: Vec::new(),
        is_private: false,
    }];
    let findings = compare_interface_members(&old, &new, "Iface", &test_file(), 1, 1);
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == "interface-generic-constraint-tightened"),
        "SEM-01: loosening constraint should not fire tightening. Got: {:?}",
        findings
    );
}
