use crate::languages::typescript::compare::generics::*;
use crate::languages::typescript::compare::rules as type_rules;
use crate::languages::typescript::types::shape::{PrimitiveKind, TypeShape};
use std::path::PathBuf;

fn file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

fn param(name: &str) -> TypeParam {
    TypeParam {
        name: name.to_string(),
        constraint_text: None,
        default_text: None,
        constraint_shape: None,
        is_const: false,
        variance: None,
    }
}

fn param_with_default(name: &str, default: &str) -> TypeParam {
    TypeParam {
        name: name.to_string(),
        constraint_text: None,
        default_text: Some(default.to_string()),
        constraint_shape: None,
        is_const: false,
        variance: None,
    }
}

fn param_with_constraint_text(name: &str, constraint: &str) -> TypeParam {
    TypeParam {
        name: name.to_string(),
        constraint_text: Some(constraint.to_string()),
        default_text: None,
        constraint_shape: None,
        is_const: false,
        variance: None,
    }
}

fn param_with_shape(name: &str, shape: TypeShape) -> TypeParam {
    TypeParam {
        name: name.to_string(),
        constraint_text: None,
        default_text: None,
        constraint_shape: Some(shape),
        is_const: false,
        variance: None,
    }
}

fn any() -> TypeShape {
    TypeShape::Primitive(PrimitiveKind::Any)
}

fn string_shape() -> TypeShape {
    TypeShape::Primitive(PrimitiveKind::String)
}

fn diff(old: &[TypeParam], new: &[TypeParam]) -> Vec<Finding> {
    compare_type_params(old, new, "MyFn", &file(), 1, 0)
}

// ── Test: empty params produce no findings ─────────────────────────────────

#[test]
fn test_empty_params_no_findings() {
    let findings = diff(&[], &[]);
    assert!(
        findings.is_empty(),
        "empty params should produce no findings"
    );
}

// ── Test: identical params produce no findings ─────────────────────────────

#[test]
fn test_identical_params_no_findings() {
    let old = vec![param("T"), param("U")];
    let new = vec![param("T"), param("U")];
    let findings = diff(&old, &new);
    assert!(
        findings.is_empty(),
        "identical params should produce no findings"
    );
}

// ── GEN-01: added without default → Major ─────────────────────────────────

#[test]
fn test_gen01_param_added_without_default() {
    // old: [T], new: [T, U] where U has no default
    let old = vec![param("T")];
    let new = vec![param("T"), param("U")];
    let findings = diff(&old, &new);

    assert_eq!(
        findings.len(),
        1,
        "should find 1 finding for added required param"
    );
    let f = &findings[0];
    assert_eq!(f.rule, RULE_GENERIC_PARAM_ADDED);
    assert_eq!(f.severity, Severity::Major);
    assert!(f.message.contains("U"), "message should mention param name");
    assert!(f.message.contains("MyFn"), "message should mention symbol");
}

// ── GEN-02: param removed → Major ─────────────────────────────────────────

#[test]
fn test_gen02_param_removed() {
    // old: [T, U], new: [T]
    let old = vec![param("T"), param("U")];
    let new = vec![param("T")];
    let findings = diff(&old, &new);

    assert_eq!(findings.len(), 1, "should find 1 finding for removed param");
    let f = &findings[0];
    assert_eq!(f.rule, RULE_GENERIC_PARAM_REMOVED);
    assert_eq!(f.severity, Severity::Major);
    assert!(
        f.message.contains("U"),
        "message should mention removed param"
    );
    assert!(f.message.contains("MyFn"), "message should mention symbol");
}

// ── GEN-03: reordered → Major ─────────────────────────────────────────────

#[test]
fn test_gen03_params_reordered() {
    // old: [T, U], new: [U, T]
    let old = vec![param("T"), param("U")];
    let new = vec![param("U"), param("T")];
    let findings = diff(&old, &new);

    // Should have exactly 1 reorder finding (no add/remove)
    let reorder_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_GENERIC_PARAM_REORDERED)
        .collect();
    assert_eq!(reorder_findings.len(), 1, "should detect reorder");
    assert_eq!(reorder_findings[0].severity, Severity::Major);
    assert!(reorder_findings[0].message.contains("MyFn"));
}

// ── GEN-04: constraint tightened with shapes ───────────────────────────────

#[test]
fn test_gen04_constraint_tightened_with_shapes() {
    // old constraint: any (wide), new constraint: string (narrow)
    // any is NOT a subtype of string → tightened
    let old = vec![param_with_shape("T", any())];
    let new = vec![param_with_shape("T", string_shape())];
    let findings = diff(&old, &new);

    let tightened: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_GENERIC_CONSTRAINT_TIGHTENED)
        .collect();
    assert_eq!(tightened.len(), 1, "should detect constraint tightening");
    assert_eq!(tightened[0].severity, Severity::Major);
    assert!(tightened[0].message.contains("T"));
    assert!(tightened[0].message.contains("MyFn"));
}

// ── GEN-04: text fallback — different texts → finding ─────────────────────

#[test]
fn test_gen04_constraint_text_fallback_different() {
    // old constraint_text="Foo", new constraint_text="Bar", both constraint_shape=None
    let old = vec![param_with_constraint_text("T", "Foo")];
    let new = vec![param_with_constraint_text("T", "Bar")];
    let findings = diff(&old, &new);

    let tightened: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_GENERIC_CONSTRAINT_TIGHTENED)
        .collect();
    assert_eq!(
        tightened.len(),
        1,
        "different constraint texts should produce finding"
    );
    assert_eq!(tightened[0].severity, Severity::Major);
}

// ── GEN-04: text fallback — same text → no finding ────────────────────────

#[test]
fn test_gen04_constraint_text_fallback_same() {
    // old constraint_text="Foo", new constraint_text="Foo" → no finding
    let old = vec![param_with_constraint_text("T", "Foo")];
    let new = vec![param_with_constraint_text("T", "Foo")];
    let findings = diff(&old, &new);

    let tightened: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_GENERIC_CONSTRAINT_TIGHTENED)
        .collect();
    assert!(
        tightened.is_empty(),
        "same constraint texts should produce no finding"
    );
}

// ── GEN-05: default changed → Major ───────────────────────────────────────

#[test]
fn test_gen05_default_changed() {
    // old default_text="string", new default_text="number"
    let old = vec![param_with_default("T", "string")];
    let new = vec![param_with_default("T", "number")];
    let findings = diff(&old, &new);

    let default_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_GENERIC_DEFAULT_CHANGED)
        .collect();
    assert_eq!(
        default_findings.len(),
        1,
        "changed default should produce finding"
    );
    assert_eq!(default_findings[0].severity, Severity::Major);
    assert!(default_findings[0].message.contains("T"));
    assert!(default_findings[0].message.contains("string"));
    assert!(default_findings[0].message.contains("number"));
    assert!(default_findings[0].message.contains("MyFn"));
}

// ── GEN-06: added with default → Minor ────────────────────────────────────

#[test]
fn test_gen06_param_added_with_default() {
    // old: [T], new: [T, U = string]
    let old = vec![param("T")];
    let new = vec![param("T"), param_with_default("U", "string")];
    let findings = diff(&old, &new);

    let added_with_default: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_GENERIC_PARAM_ADDED_WITH_DEFAULT)
        .collect();
    assert_eq!(
        added_with_default.len(),
        1,
        "should detect added param with default"
    );
    assert_eq!(added_with_default[0].severity, Severity::Minor);
    assert!(added_with_default[0].message.contains("U"));
    assert!(added_with_default[0].message.contains("string"));
    assert!(added_with_default[0].message.contains("MyFn"));
}

// ── const type parameter modifier ──────────────────────────────

#[test]
fn test_const_type_param_removed() {
    let old = vec![TypeParam {
        name: "T".to_string(),
        constraint_text: None,
        default_text: None,
        constraint_shape: None,
        is_const: true,
        variance: None,
    }];
    let new = vec![TypeParam {
        name: "T".to_string(),
        constraint_text: None,
        default_text: None,
        constraint_shape: None,
        is_const: false,
        variance: None,
    }];
    let findings = compare_type_params(&old, &new, "Foo", &file(), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "const-type-param-changed"),
        "should detect const removal; got: {:?}",
        findings
    );
}

#[test]
fn test_const_type_param_added() {
    let old = vec![TypeParam {
        name: "T".to_string(),
        constraint_text: None,
        default_text: None,
        constraint_shape: None,
        is_const: false,
        variance: None,
    }];
    let new = vec![TypeParam {
        name: "T".to_string(),
        constraint_text: None,
        default_text: None,
        constraint_shape: None,
        is_const: true,
        variance: None,
    }];
    let findings = compare_type_params(&old, &new, "Foo", &file(), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "const-type-param-changed"),
        "should detect const addition; got: {:?}",
        findings
    );
}

#[test]
fn test_const_type_param_unchanged() {
    let old = vec![TypeParam {
        name: "T".to_string(),
        constraint_text: None,
        default_text: None,
        constraint_shape: None,
        is_const: true,
        variance: None,
    }];
    let new = vec![TypeParam {
        name: "T".to_string(),
        constraint_text: None,
        default_text: None,
        constraint_shape: None,
        is_const: true,
        variance: None,
    }];
    let findings = compare_type_params(&old, &new, "Foo", &file(), 1, 1);
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == "const-type-param-changed"),
        "should not fire for unchanged const; got: {:?}",
        findings
    );
}

// ── TLF-05: generic variance changed ─────────────────────────────────────

#[test]
fn variance_changed_fires_major() {
    use crate::languages::typescript::model::Variance;
    let mut old_p = param("T");
    old_p.variance = Some(Variance::In);
    let mut new_p = param("T");
    new_p.variance = Some(Variance::Out);
    let findings = diff(&[old_p], &[new_p]);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == type_rules::RULE_GENERIC_VARIANCE_CHANGED),
        "variance change should fire generic-variance-changed, got: {:?}",
        findings
    );
    let f = findings
        .iter()
        .find(|f| f.rule == type_rules::RULE_GENERIC_VARIANCE_CHANGED)
        .unwrap();
    assert_eq!(
        f.severity,
        Severity::Major,
        "generic-variance-changed should be Major"
    );
}

#[test]
fn variance_unchanged_no_finding() {
    use crate::languages::typescript::model::Variance;
    let mut old_p = param("T");
    old_p.variance = Some(Variance::In);
    let mut new_p = param("T");
    new_p.variance = Some(Variance::In);
    let findings = diff(&[old_p], &[new_p]);
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == type_rules::RULE_GENERIC_VARIANCE_CHANGED),
        "unchanged variance should NOT fire"
    );
}

// ── GEN-01 + GEN-06 combined ──────────────────────────────────────────────

#[test]
fn test_gen01_and_gen06_combined() {
    // old: [T], new: [T, U, V = string]
    // U has no default → GEN-01 (Major)
    // V has default → GEN-06 (Minor)
    let old = vec![param("T")];
    let new = vec![param("T"), param("U"), param_with_default("V", "string")];
    let findings = diff(&old, &new);

    let added_required: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_GENERIC_PARAM_ADDED)
        .collect();
    let added_with_default: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_GENERIC_PARAM_ADDED_WITH_DEFAULT)
        .collect();

    assert_eq!(added_required.len(), 1, "U should be GEN-01");
    assert_eq!(added_with_default.len(), 1, "V should be GEN-06");
    assert_eq!(added_required[0].severity, Severity::Major);
    assert_eq!(added_with_default[0].severity, Severity::Minor);
    assert!(added_required[0].message.contains("U"));
    assert!(added_with_default[0].message.contains("V"));
}
