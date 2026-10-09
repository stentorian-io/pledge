use crate::languages::typescript::compare::enums::*;
use crate::languages::typescript::model::{EnumMember, EnumShape, EnumValueKind};
use std::path::PathBuf;

fn make_file() -> PathBuf {
    PathBuf::from("test.ts")
}

fn numeric_member(name: &str, computed: i64) -> EnumMember {
    EnumMember {
        name: name.to_string(),
        raw_value: None,
        computed_value: Some(computed),
        value_kind: EnumValueKind::Numeric,
    }
}

fn explicit_numeric_member(name: &str, raw: &str, computed: i64) -> EnumMember {
    EnumMember {
        name: name.to_string(),
        raw_value: Some(raw.to_string()),
        computed_value: Some(computed),
        value_kind: EnumValueKind::Numeric,
    }
}

fn string_member(name: &str, value: &str) -> EnumMember {
    EnumMember {
        name: name.to_string(),
        raw_value: Some(value.to_string()),
        computed_value: None,
        value_kind: EnumValueKind::StringLiteral,
    }
}

fn other_expr_member(name: &str, raw: &str) -> EnumMember {
    EnumMember {
        name: name.to_string(),
        raw_value: Some(raw.to_string()),
        computed_value: None,
        value_kind: EnumValueKind::OtherExpr,
    }
}

fn enum_shape(members: Vec<EnumMember>, is_const: bool) -> EnumShape {
    EnumShape { members, is_const }
}

fn run(old: &EnumShape, new: &EnumShape) -> Vec<Finding> {
    compare_enum_shapes(old, new, "MyEnum", &make_file(), 1, 1)
}

// ── ENUM-01: member removed ──────────────────────────────────────────────

#[test]
fn test_enum01_member_removed() {
    // old has {A, B, C}, new has {A, C} — B is removed
    let old = enum_shape(
        vec![
            numeric_member("A", 0),
            numeric_member("B", 1),
            numeric_member("C", 2),
        ],
        false,
    );
    let new = enum_shape(vec![numeric_member("A", 0), numeric_member("C", 1)], false);
    let findings = run(&old, &new);
    assert!(
        findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_REMOVED
            && f.message.contains("B")
            && f.severity == Severity::Major),
        "Expected enum-member-removed for B, got: {:?}",
        findings
    );
}

// ── ENUM-02: value changed (string) ──────────────────────────────────────

#[test]
fn test_enum02_string_value_changed() {
    // old A="hello", new A="world" -> Finding with rule "enum-member-value-changed"
    let old = enum_shape(vec![string_member("A", "\"hello\"")], false);
    let new = enum_shape(vec![string_member("A", "\"world\"")], false);
    let findings = run(&old, &new);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED
                && f.message.contains("A")
                && f.severity == Severity::Major),
        "Expected enum-member-value-changed for string change, got: {:?}",
        findings
    );
    let f = findings
        .iter()
        .find(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED)
        .unwrap();
    assert!(
        f.message.contains("hello") && f.message.contains("world"),
        "Message should include old and new values: {}",
        f.message
    );
}

// ── ENUM-02: value changed (numeric) ─────────────────────────────────────

#[test]
fn test_enum02_numeric_value_changed() {
    // old A=1, new A=2 -> Finding with rule "enum-member-value-changed"
    let old = enum_shape(vec![explicit_numeric_member("A", "1", 1)], false);
    let new = enum_shape(vec![explicit_numeric_member("A", "2", 2)], false);
    let findings = run(&old, &new);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED && f.message.contains("A")),
        "Expected enum-member-value-changed for numeric change, got: {:?}",
        findings
    );
}

// ── ENUM-03: reordered with implicit value shift ──────────────────────────

#[test]
fn test_enum03_reorder_with_value_shift() {
    // old {A, B, C} (implicit values 0,1,2), new {B, A, C} (implicit values 0,1,2)
    // After reorder: A was at index 0 (value 0), now at index 1 (value 1) -> shift
    // B was at index 1 (value 1), now at index 0 (value 0) -> shift
    let old = enum_shape(
        vec![
            numeric_member("A", 0),
            numeric_member("B", 1),
            numeric_member("C", 2),
        ],
        false,
    );
    let new = enum_shape(
        vec![
            numeric_member("B", 0),
            numeric_member("A", 1),
            numeric_member("C", 2),
        ],
        false,
    );
    let findings = run(&old, &new);
    // A: was 0, now 1 -> reordered
    assert!(
        findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED
            && f.message.contains("A")
            && f.severity == Severity::Major),
        "Expected enum-member-reordered for A, got: {:?}",
        findings
    );
    // B: was 1, now 0 -> reordered
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED && f.message.contains("B")),
        "Expected enum-member-reordered for B, got: {:?}",
        findings
    );
}

// ── ENUM-03 negative: explicit values unchanged = no finding ──────────────

#[test]
fn test_enum03_negative_explicit_values_unchanged() {
    // old {A=10, B=20}, new {B=20, A=10} — explicit values identical after reordering
    // should NOT be flagged
    let old = enum_shape(
        vec![
            explicit_numeric_member("A", "10", 10),
            explicit_numeric_member("B", "20", 20),
        ],
        false,
    );
    let new = enum_shape(
        vec![
            explicit_numeric_member("B", "20", 20),
            explicit_numeric_member("A", "10", 10),
        ],
        false,
    );
    let findings = run(&old, &new);
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED),
        "Expected no enum-member-reordered for identical explicit values, got: {:?}",
        findings
    );
    assert!(
        findings.is_empty(),
        "Expected no findings for explicit value reorder, got: {:?}",
        findings
    );
}

// ── ENUM-04b: regular to const ───────────────────────────────────────────

#[test]
fn test_enum04b_regular_to_const() {
    // old is_const=false, new is_const=true -> Finding with rule "enum-regular-to-const"
    let old = enum_shape(vec![numeric_member("A", 0)], false);
    let new = enum_shape(vec![numeric_member("A", 0)], true);
    let findings = run(&old, &new);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_REGULAR_TO_CONST && f.severity == Severity::Major),
        "Expected enum-regular-to-const finding, got: {:?}",
        findings
    );
}

#[test]
fn test_enum04b_no_finding_when_both_regular() {
    let old = enum_shape(vec![numeric_member("A", 0)], false);
    let new = enum_shape(vec![numeric_member("A", 0)], false);
    let findings = run(&old, &new);
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_REGULAR_TO_CONST),
        "Expected no enum-regular-to-const finding when both regular"
    );
}

// ── ENUM-04: const to regular ─────────────────────────────────────────────

#[test]
fn test_enum04_const_to_regular() {
    // old is_const=true, new is_const=false -> Finding with rule "enum-const-to-regular"
    let old = enum_shape(vec![numeric_member("A", 0)], true);
    let new = enum_shape(vec![numeric_member("A", 0)], false);
    let findings = run(&old, &new);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_CONST_TO_REGULAR && f.severity == Severity::Major),
        "Expected enum-const-to-regular finding, got: {:?}",
        findings
    );
}

// ── ENUM-05: numeric to string ────────────────────────────────────────────

#[test]
fn test_enum05_numeric_to_string() {
    // old members all Numeric kind, new members all StringLiteral kind
    let old = enum_shape(vec![numeric_member("A", 0), numeric_member("B", 1)], false);
    let new = enum_shape(
        vec![string_member("A", "\"foo\""), string_member("B", "\"bar\"")],
        false,
    );
    let findings = run(&old, &new);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_NUMERIC_TO_STRING && f.severity == Severity::Major),
        "Expected enum-numeric-to-string finding, got: {:?}",
        findings
    );
}

// ── No findings when identical ────────────────────────────────────────────

#[test]
fn test_no_findings_when_identical() {
    let shape = enum_shape(
        vec![
            numeric_member("A", 0),
            explicit_numeric_member("B", "10", 10),
            string_member("C", "\"hello\""),
        ],
        false,
    );
    let findings = run(&shape, &shape);
    assert!(
        findings.is_empty(),
        "Expected no findings for identical enums, got: {:?}",
        findings
    );
}

// ── ENUM-02 with non-evaluable initializers ────────────────────────

#[test]
fn test_enum02_non_evaluable_text_differs() {
    // old A="Foo.Bar", new A="Foo.Baz" — text differs per -> Finding
    let old = enum_shape(vec![other_expr_member("A", "Foo.Bar")], false);
    let new = enum_shape(vec![other_expr_member("A", "Foo.Baz")], false);
    let findings = run(&old, &new);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED && f.message.contains("A")),
        "Expected enum-member-value-changed for non-evaluable text change, got: {:?}",
        findings
    );
    let f = findings
        .iter()
        .find(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED)
        .unwrap();
    assert!(
        f.message.contains("Foo.Bar") && f.message.contains("Foo.Baz"),
        "Message should include old and new values: {}",
        f.message
    );
}

// ── ENUM-02 non-evaluable identical text = no finding ────────────────────

#[test]
fn test_enum02_non_evaluable_text_same_no_finding() {
    // old A="Foo.Bar", new A="Foo.Bar" — text same per -> No finding
    let old = enum_shape(vec![other_expr_member("A", "Foo.Bar")], false);
    let new = enum_shape(vec![other_expr_member("A", "Foo.Bar")], false);
    let findings = run(&old, &new);
    assert!(
        findings.is_empty(),
        "Expected no findings for identical non-evaluable values, got: {:?}",
        findings
    );
}

// ── BEH-04: enum member added ────────────────────────────────────────────

#[test]
fn test_beh04_member_added() {
    let old = enum_shape(vec![numeric_member("A", 0), numeric_member("B", 1)], false);
    let new = enum_shape(
        vec![
            numeric_member("A", 0),
            numeric_member("B", 1),
            numeric_member("C", 2),
        ],
        false,
    );
    let findings = run(&old, &new);
    assert!(
        findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_ADDED
            && f.message.contains("C")
            && f.severity == Severity::Warning),
        "Expected enum-member-added Warning for C, got: {:?}",
        findings
    );
}

#[test]
fn test_beh04_multiple_members_added() {
    let old = enum_shape(vec![numeric_member("A", 0)], false);
    let new = enum_shape(
        vec![
            numeric_member("A", 0),
            numeric_member("B", 1),
            numeric_member("C", 2),
        ],
        false,
    );
    let findings = run(&old, &new);
    let added: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_ENUM_MEMBER_ADDED)
        .collect();
    assert_eq!(
        added.len(),
        2,
        "Expected 2 enum-member-added findings, got: {:?}",
        added
    );
}

#[test]
fn test_beh04_no_finding_when_identical() {
    let shape = enum_shape(vec![numeric_member("A", 0), numeric_member("B", 1)], false);
    let findings = run(&shape, &shape);
    assert!(
        !findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_ADDED),
        "Expected no enum-member-added findings for identical enum"
    );
}

#[test]
fn test_beh04_const_enum_member_added() {
    // Warning regardless of const/regular
    let old = enum_shape(vec![numeric_member("A", 0)], true);
    let new = enum_shape(vec![numeric_member("A", 0), numeric_member("B", 1)], true);
    let findings = run(&old, &new);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_MEMBER_ADDED && f.severity == Severity::Warning),
        "Expected enum-member-added Warning for const enum, got: {:?}",
        findings
    );
}

// ── BEH-03: enum value shifted by insertion ──────────────────────────────

#[test]
fn test_beh03_value_shifted_by_insertion() {
    // old: {A=0, B=1, C=2} (all implicit)
    // new: {X=0, A=1, B=2, C=3} (X inserted at front)
    // A, B, C shifted by insertion -> BEH-03 Warning (not ENUM-03)
    let old = enum_shape(
        vec![
            numeric_member("A", 0),
            numeric_member("B", 1),
            numeric_member("C", 2),
        ],
        false,
    );
    let new = enum_shape(
        vec![
            numeric_member("X", 0),
            numeric_member("A", 1),
            numeric_member("B", 2),
            numeric_member("C", 3),
        ],
        false,
    );
    let findings = run(&old, &new);
    // A, B, C should all fire BEH-03 (shifted by insertion of X)
    let shifted: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_ENUM_VALUE_SHIFTED)
        .collect();
    assert_eq!(
        shifted.len(),
        3,
        "Expected 3 enum-value-shifted findings, got: {:?}",
        shifted
    );
    assert!(shifted.iter().all(|f| f.severity == Severity::Warning));
    // ENUM-03 should NOT fire (no reordering)
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED),
        "ENUM-03 should not fire for insertion-shifted members"
    );
}

#[test]
fn test_beh03_no_double_fire_with_enum03() {
    // old: {A=0, B=1, C=2} (implicit)
    // new: {B=0, A=1, C=2} (reordered A and B — not insertion)
    // A and B should fire ENUM-03 (Major), NOT BEH-03
    let old = enum_shape(
        vec![
            numeric_member("A", 0),
            numeric_member("B", 1),
            numeric_member("C", 2),
        ],
        false,
    );
    let new = enum_shape(
        vec![
            numeric_member("B", 0),
            numeric_member("A", 1),
            numeric_member("C", 2),
        ],
        false,
    );
    let findings = run(&old, &new);
    // ENUM-03 fires for A and B
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED && f.message.contains("A")),
        "Expected ENUM-03 for A"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED && f.message.contains("B")),
        "Expected ENUM-03 for B"
    );
    // BEH-03 should NOT fire
    assert!(
        !findings.iter().any(|f| f.rule == RULE_ENUM_VALUE_SHIFTED),
        "BEH-03 should not fire when ENUM-03 fires, got: {:?}",
        findings
    );
}

#[test]
fn test_beh03_explicit_values_not_affected() {
    // old: {A=10, B=20} (explicit)
    // new: {X=0, A=10, B=20} (insertion, but A and B have explicit values)
    // No BEH-03 because values are explicit (unchanged)
    let old = enum_shape(
        vec![
            explicit_numeric_member("A", "10", 10),
            explicit_numeric_member("B", "20", 20),
        ],
        false,
    );
    let new = enum_shape(
        vec![
            numeric_member("X", 0),
            explicit_numeric_member("A", "10", 10),
            explicit_numeric_member("B", "20", 20),
        ],
        false,
    );
    let findings = run(&old, &new);
    assert!(
        !findings.iter().any(|f| f.rule == RULE_ENUM_VALUE_SHIFTED),
        "BEH-03 should not fire for explicit-value members, got: {:?}",
        findings
    );
}

#[test]
fn test_beh03_unshifted_member_no_finding() {
    // old: {A=0, B=1} (implicit)
    // new: {A=0, B=1, C=2} (C added at end — A and B not shifted)
    // A and B values unchanged -> no BEH-03
    let old = enum_shape(vec![numeric_member("A", 0), numeric_member("B", 1)], false);
    let new = enum_shape(
        vec![
            numeric_member("A", 0),
            numeric_member("B", 1),
            numeric_member("C", 2),
        ],
        false,
    );
    let findings = run(&old, &new);
    assert!(
        !findings.iter().any(|f| f.rule == RULE_ENUM_VALUE_SHIFTED),
        "BEH-03 should not fire when values are unchanged, got: {:?}",
        findings
    );
}

// ── No double-fire: ENUM-02 and ENUM-03 are mutually exclusive ────────────

#[test]
fn test_no_double_fire_enum02_and_enum03() {
    // If a member had explicit values that changed, ENUM-02 fires, not ENUM-03.
    // old {A=1, B=2}, new {B=5, A=3} — explicit values changed
    let old = enum_shape(
        vec![
            explicit_numeric_member("A", "1", 1),
            explicit_numeric_member("B", "2", 2),
        ],
        false,
    );
    let new = enum_shape(
        vec![
            explicit_numeric_member("B", "5", 5),
            explicit_numeric_member("A", "3", 3),
        ],
        false,
    );
    let findings = run(&old, &new);
    // ENUM-02 should fire for both A and B (explicit value changes)
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED && f.message.contains("A")),
        "Expected ENUM-02 for A"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED && f.message.contains("B")),
        "Expected ENUM-02 for B"
    );
    // ENUM-03 should NOT fire since ENUM-02 handled these
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED),
        "ENUM-03 should not fire when ENUM-02 fired for same members, got: {:?}",
        findings
    );
}
