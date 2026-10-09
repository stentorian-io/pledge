//! Enum modifiers compatibility checks.

use interlint::core::findings::{Finding, Severity};
use interlint::languages::typescript::compare::enums as enum_diff;
use interlint::languages::typescript::compare::rules::RULE_ENUM_REGULAR_TO_CONST;
use interlint::languages::typescript::model::{EnumMember, EnumShape, EnumValueKind};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("test.ts")
}

/// BUG 9: `is_const=false` → `is_const=true` (regular to const) should fire
/// `enum-regular-to-const` with Major severity. This is the reverse of
/// ENUM-04 (const-to-regular) which was already tested.
#[test]
fn test_regular_to_const_enum() {
    let member = EnumMember {
        name: "Up".to_string(),
        raw_value: None,
        computed_value: Some(0),
        value_kind: EnumValueKind::Numeric,
    };

    let old_shape = EnumShape {
        members: vec![member.clone()],
        is_const: false,
    };
    let new_shape = EnumShape {
        members: vec![member],
        is_const: true,
    };

    let findings =
        enum_diff::compare_enum_shapes(&old_shape, &new_shape, "Dir", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_REGULAR_TO_CONST),
        "Expected enum-regular-to-const; got: {:?}",
        rule_ids(&findings)
    );

    let finding = findings
        .iter()
        .find(|f| f.rule == RULE_ENUM_REGULAR_TO_CONST)
        .unwrap();
    assert_eq!(
        finding.severity,
        Severity::Major,
        "enum-regular-to-const must be Major severity"
    );
}

fn rule_ids(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule).collect()
}
