use crate::languages::javascript::model as js_model;
use crate::languages::typescript::compare::types::*;
use crate::languages::typescript::types::shape::{
    FieldShape, LiteralKind, ParamShape, PrimitiveKind,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn dummy_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

fn diff(old: TypeShape, new: TypeShape) -> Vec<Finding> {
    diff_types(&old, &new, "MyType", &dummy_file(), 1, 1)
}

fn rule_ids(findings: &[Finding]) -> Vec<&'static str> {
    findings.iter().map(|f| f.rule).collect()
}

// Test 1: Identical shapes produce no findings
#[test]
fn test_identical_primitives_no_findings() {
    let findings = diff(
        TypeShape::Primitive(PrimitiveKind::String),
        TypeShape::Primitive(PrimitiveKind::String),
    );
    assert!(
        findings.is_empty(),
        "identical shapes should produce no findings"
    );
}

// Test 2: Different primitives produce type-incompatible (no more "type-changed")
#[test]
fn test_different_primitives_type_changed() {
    let findings = diff(
        TypeShape::Primitive(PrimitiveKind::String),
        TypeShape::Primitive(PrimitiveKind::Number),
    );
    assert_eq!(rule_ids(&findings), vec!["type-incompatible"]);
    assert_eq!(findings[0].severity, Severity::Major);
    // message includes old and new type values
    assert!(
        findings[0].message.contains("string"),
        "message should contain old type 'string'"
    );
    assert!(
        findings[0].message.contains("number"),
        "message should contain new type 'number'"
    );
}

// Test 3: Object field removed
#[test]
fn test_field_removed() {
    let mut old_fields = BTreeMap::new();
    old_fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::Number),
        },
    );
    old_fields.insert(
        "y".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        },
    );
    let mut new_fields = BTreeMap::new();
    new_fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::Number),
        },
    );

    let findings = diff(TypeShape::Object(old_fields), TypeShape::Object(new_fields));
    assert!(rule_ids(&findings).contains(&"field-removed"));
    let removed = findings.iter().find(|f| f.rule == "field-removed").unwrap();
    assert!(
        removed.message.contains("y"),
        "message should mention field 'y'"
    );
    assert_eq!(removed.severity, Severity::Major);
}

// Test 4: Object field type changed — should produce type-incompatible at leaf level
#[test]
fn test_field_type_changed() {
    let mut old_fields = BTreeMap::new();
    old_fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::Number),
        },
    );
    let mut new_fields = BTreeMap::new();
    new_fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        },
    );

    let findings = diff(TypeShape::Object(old_fields), TypeShape::Object(new_fields));
    assert!(
        !findings.is_empty(),
        "should have findings for changed field type"
    );
    // Should produce type-incompatible (from leaf-level primitive diff)
    assert!(
        findings.iter().any(|f| f.rule == "type-incompatible"),
        "field type change should produce type-incompatible finding, got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
    // All findings should reference the field path "x" in message
    assert!(
        findings.iter().any(|f| f.message.contains("x")),
        "findings should reference field 'x', got: {:?}",
        findings.iter().map(|f| &f.message).collect::<Vec<_>>()
    );
}

// Test 5: Object field added (required) — Major
#[test]
fn test_field_added_required_major() {
    let mut old_fields = BTreeMap::new();
    old_fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::Number),
        },
    );
    let mut new_fields = BTreeMap::new();
    new_fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::Number),
        },
    );
    new_fields.insert(
        "y".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        },
    );

    let findings = diff(TypeShape::Object(old_fields), TypeShape::Object(new_fields));
    assert!(rule_ids(&findings).contains(&"field-added"));
    let added = findings.iter().find(|f| f.rule == "field-added").unwrap();
    assert_eq!(added.severity, Severity::Major);
    assert!(added.message.contains("required"));
}

// Test 6: Object field added (optional) — Minor
#[test]
fn test_field_added_optional_minor() {
    let mut old_fields = BTreeMap::new();
    old_fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::Number),
        },
    );
    let mut new_fields = BTreeMap::new();
    new_fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::Number),
        },
    );
    new_fields.insert(
        "y".to_string(),
        FieldShape {
            write_type: None,
            optional: true,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        },
    );

    let findings = diff(TypeShape::Object(old_fields), TypeShape::Object(new_fields));
    let added = findings.iter().find(|f| f.rule == "field-added").unwrap();
    assert_eq!(added.severity, Severity::Minor);
    assert!(added.message.contains("optional"));
}

// Test 7: Nested object diff — path mentions "config.timeout"
#[test]
fn test_nested_object_path() {
    let mut inner_old = BTreeMap::new();
    inner_old.insert(
        "timeout".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::Number),
        },
    );
    let mut inner_new = BTreeMap::new();
    inner_new.insert(
        "timeout".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        },
    );

    let mut old_fields = BTreeMap::new();
    old_fields.insert(
        "config".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Object(inner_old),
        },
    );
    let mut new_fields = BTreeMap::new();
    new_fields.insert(
        "config".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Object(inner_new),
        },
    );

    let findings = diff(TypeShape::Object(old_fields), TypeShape::Object(new_fields));
    assert!(!findings.is_empty());
    // Should mention the nested path "config.timeout"
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("config.timeout")),
        "finding should mention 'config.timeout', got: {:?}",
        findings.iter().map(|f| &f.message).collect::<Vec<_>>()
    );
}

// Test 8: Array element type changed
#[test]
fn test_array_element_type_changed() {
    let findings = diff(
        TypeShape::Array(Box::new(TypeShape::Primitive(PrimitiveKind::String))),
        TypeShape::Array(Box::new(TypeShape::Primitive(PrimitiveKind::Number))),
    );
    assert!(!findings.is_empty());
    assert!(findings.iter().any(|f| f.rule == "type-incompatible"));
}

// Test 9: Union widened — no finding (safe)
#[test]
fn test_union_widened_no_finding() {
    let findings = diff(
        TypeShape::Union(vec![
            TypeShape::Primitive(PrimitiveKind::String),
            TypeShape::Primitive(PrimitiveKind::Number),
        ]),
        TypeShape::Union(vec![
            TypeShape::Primitive(PrimitiveKind::String),
            TypeShape::Primitive(PrimitiveKind::Number),
            TypeShape::Primitive(PrimitiveKind::Boolean),
        ]),
    );
    // Widening is safe — no Major findings
    assert!(
        !findings.iter().any(|f| f.severity == Severity::Major),
        "union widening should not produce Major findings"
    );
}

// Test 10: Union narrowed — Major with type-incompatible (not "type-changed")
#[test]
fn test_union_narrowed_major() {
    let findings = diff(
        TypeShape::Union(vec![
            TypeShape::Primitive(PrimitiveKind::String),
            TypeShape::Primitive(PrimitiveKind::Number),
            TypeShape::Primitive(PrimitiveKind::Boolean),
        ]),
        TypeShape::Union(vec![
            TypeShape::Primitive(PrimitiveKind::String),
            TypeShape::Primitive(PrimitiveKind::Number),
        ]),
    );
    assert!(findings.iter().any(|f| f.severity == Severity::Major));
    assert!(
        findings.iter().any(|f| f.rule == "type-incompatible"),
        "narrowed union should use type-incompatible, not type-changed"
    );
    // message should include old and new type values
    let f = findings
        .iter()
        .find(|f| f.rule == "type-incompatible")
        .unwrap();
    assert!(
        f.message.contains("string") || f.message.contains("boolean"),
        "message should reference type values: {}",
        f.message
    );
}

// Test 11: Opaque on either side — CRITICAL: behavior change
// Opaque now produces type-incompatible Major (not empty findings)
#[test]
fn changed_opaque_types_report_incomplete_analysis() {
    // Opaque on old side
    let findings = diff(
        TypeShape::Opaque("x".to_string()),
        TypeShape::Primitive(PrimitiveKind::String),
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "analysis-incomplete" && f.severity == Severity::Warning),
        "Opaque old + Primitive new should produce incomplete analysis"
    );

    // Opaque on new side
    let findings = diff(
        TypeShape::Primitive(PrimitiveKind::String),
        TypeShape::Opaque("x".to_string()),
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "analysis-incomplete" && f.severity == Severity::Warning),
        "Primitive old + Opaque new should produce incomplete analysis"
    );

    // Both Opaque but different — still incomplete
    let findings = diff(
        TypeShape::Opaque("x".to_string()),
        TypeShape::Opaque("y".to_string()),
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "analysis-incomplete" && f.severity == Severity::Warning),
        "different Opaques should produce incomplete analysis"
    );
}

// Test 11b: Both Opaque same value — no findings (identical)
#[test]
fn test_opaque_same_value_no_findings() {
    let findings = diff(
        TypeShape::Opaque("x".to_string()),
        TypeShape::Opaque("x".to_string()),
    );
    assert!(
        findings.is_empty(),
        "identical Opaques should produce no findings"
    );
}

// Test 12: Max recursion depth 20 — stops at depth 20 without panicking
#[test]
fn test_max_recursion_depth() {
    // Build a deeply nested object: {a: {a: {a:... (21 levels)}}}
    fn nested_object(depth: usize) -> TypeShape {
        if depth == 0 {
            TypeShape::Primitive(PrimitiveKind::String)
        } else {
            let mut fields = BTreeMap::new();
            fields.insert(
                "a".to_string(),
                FieldShape {
                    write_type: None,
                    optional: false,
                    readonly: false,
                    shape: nested_object(depth - 1),
                },
            );
            TypeShape::Object(fields)
        }
    }

    // Old: 22 levels deep with String leaf
    // New: same structure but Number leaf — triggers recursion all the way down
    fn nested_object_num(depth: usize) -> TypeShape {
        if depth == 0 {
            TypeShape::Primitive(PrimitiveKind::Number)
        } else {
            let mut fields = BTreeMap::new();
            fields.insert(
                "a".to_string(),
                FieldShape {
                    write_type: None,
                    optional: false,
                    readonly: false,
                    shape: nested_object_num(depth - 1),
                },
            );
            TypeShape::Object(fields)
        }
    }

    // Should not panic, should produce at most MAX_DEPTH-bounded findings
    let findings = diff(nested_object(22), nested_object_num(22));
    assert!(
        !findings.is_empty(),
        "should find some change in deeply nested object"
    );
    // The truncation finding should be present
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "analysis-incomplete" && f.severity == Severity::Warning),
        "should have truncation finding at depth 20"
    );
}

// Test 13: Function return type changed — incompatible primitives produce type-incompatible
// Old: returns number, new: returns string — these are incompatible (neither is subtype)
// Neither number <: string nor string <: number → type-incompatible (not return-type-widened)
#[test]
fn test_function_return_type_changed() {
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Number)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::String)),
        },
    );
    assert!(
        !findings.is_empty(),
        "return type change should produce findings"
    );
    assert!(
            findings.iter().any(|f| f.rule == "type-incompatible"),
            "incompatible return type change (number->string) should produce type-incompatible, got: {:?}",
            findings.iter().map(|f| f.rule).collect::<Vec<_>>()
        );
}

// Test 14: Function param added (required) — Major with param-added
#[test]
fn test_function_required_param_added() {
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![
                ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                },
                ParamShape {
                    name: "y".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::Number),
                },
            ],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    );
    assert!(
        !findings.is_empty(),
        "adding required param should produce findings"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "param-added" && f.severity == Severity::Major),
        "required param added should be Major param-added"
    );
}

// Test 15: Literal type changed — type-incompatible (not "type-changed")
#[test]
fn test_literal_type_changed() {
    let findings = diff(
        TypeShape::Literal(LiteralKind::String("foo".to_string())),
        TypeShape::Literal(LiteralKind::String("bar".to_string())),
    );
    assert!(!findings.is_empty());
    assert!(
        findings.iter().any(|f| f.rule == "type-incompatible"),
        "literal change should use type-incompatible, not type-changed"
    );
}

// Test 16: identical complex object — no findings
#[test]
fn test_identical_objects_no_findings() {
    let mut fields = BTreeMap::new();
    fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::Number),
        },
    );
    fields.insert(
        "y".to_string(),
        FieldShape {
            write_type: None,
            optional: true,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        },
    );
    let findings = diff(TypeShape::Object(fields.clone()), TypeShape::Object(fields));
    assert!(
        findings.is_empty(),
        "identical objects should produce no findings"
    );
}

// ─── New signature detection tests (SIG-01..05) ────────────────────────────

// Test 17: Param type narrowed (SIG-02)
// Old: (x: string | number) => void, New: (x: string) => void
// new param type (string) does NOT accept everything old param type (string | number) accepted
// is_subtype_of(string | number, string) = false → param-type-narrowed Major
#[test]
fn test_param_type_narrowed_major() {
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Union(vec![
                    TypeShape::Primitive(PrimitiveKind::String),
                    TypeShape::Primitive(PrimitiveKind::Number),
                ]),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "param-type-narrowed" && f.severity == Severity::Major),
        "narrowed param type should produce param-type-narrowed Major, got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, f.severity.clone()))
            .collect::<Vec<_>>()
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "param-type-narrowed")
        .unwrap();
    // message should include old and new type values
    assert!(
        f.message.contains("string | number") || f.message.contains("number | string"),
        "message should contain old type 'string | number': {}",
        f.message
    );
    assert!(
        f.message.contains("string"),
        "message should contain new type 'string': {}",
        f.message
    );
}

// Test 18: Param type widened (SIG-02 inverse — safe)
// Old: (x: string) => void, New: (x: string | number) => void
// is_subtype_of(string, string | number) = true → new param accepts more → safe
#[test]
fn test_param_type_widened_safe() {
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Union(vec![
                    TypeShape::Primitive(PrimitiveKind::String),
                    TypeShape::Primitive(PrimitiveKind::Number),
                ]),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    );
    // Safe: widening a param type — no Major findings
    assert!(
        !findings.iter().any(|f| f.severity == Severity::Major),
        "widening param type should produce no Major findings, got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, f.severity.clone()))
            .collect::<Vec<_>>()
    );
}

// Test 19: Return type widened (SIG-03) — BREAKING
// Old: () => string, New: () => string | number
// old_ret <: new_ret (string <: string|number) but new_ret !<: old_ret → NEW is strictly wider
// Callers that expected string now may get number → BREAKING → return-type-widened Major
#[test]
fn test_return_type_widened_major() {
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::String)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![],
            return_type: Box::new(TypeShape::Union(vec![
                TypeShape::Primitive(PrimitiveKind::String),
                TypeShape::Primitive(PrimitiveKind::Number),
            ])),
        },
    );
    assert!(
            findings.iter().any(|f| f.rule == "return-type-widened" && f.severity == Severity::Major),
            "old=string, new=string|number: return type widened should produce return-type-widened Major, got: {:?}",
            findings.iter().map(|f| (f.rule, f.severity.clone())).collect::<Vec<_>>()
        );
}

// Test 20: Return type narrowed (SIG-03 inverse — safe)
// Old: () => string | number, New: () => string
// new_ret <: old_ret (string <: string|number) → new is strictly narrower → SAFE
// Callers that received string|number now always get string — they still work fine
#[test]
fn test_return_type_narrowed_safe() {
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![],
            return_type: Box::new(TypeShape::Union(vec![
                TypeShape::Primitive(PrimitiveKind::String),
                TypeShape::Primitive(PrimitiveKind::Number),
            ])),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::String)),
        },
    );
    // old=string|number, new=string → new_ret <: old_ret → safe (narrowing)
    assert!(
            !findings.iter().any(|f| f.rule == "return-type-widened"),
            "old=string|number, new=string: return type narrowed should be safe (no return-type-widened), got: {:?}",
            findings.iter().map(|f| (f.rule, f.severity.clone())).collect::<Vec<_>>()
        );
}

// Test 21: Param reordered (SIG-05)
// Old: (a: string, b: number), New: (b: number, a: string)
#[test]
fn test_param_reordered() {
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![
                ParamShape {
                    name: "a".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                },
                ParamShape {
                    name: "b".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::Number),
                },
            ],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![
                ParamShape {
                    name: "b".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::Number),
                },
                ParamShape {
                    name: "a".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                },
            ],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "param-reordered" && f.severity == Severity::Major),
        "reordered params should produce param-reordered Major, got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, f.severity.clone()))
            .collect::<Vec<_>>()
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "param-reordered")
        .unwrap();
    assert!(
        f.message.contains("a") && f.message.contains("b"),
        "param-reordered message should list param names: {}",
        f.message
    );
}

// Test 22: Optional param added (SIG-04, OUT-05)
// Old: (x: string) => void, New: (x: string, y?: number) => void
#[test]
fn test_optional_param_added_minor() {
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![
                ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                },
                ParamShape {
                    name: "y".to_string(),
                    optional: true,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::Number),
                },
            ],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "param-added" && f.severity == Severity::Minor),
        "optional param added should produce param-added Minor, got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, f.severity.clone()))
            .collect::<Vec<_>>()
    );
    let f = findings.iter().find(|f| f.rule == "param-added").unwrap();
    assert!(
        f.message.contains("y"),
        "param-added message should mention param name 'y': {}",
        f.message
    );
}

// Test 23: Param removed (SIG-01)
// Old: (x: string, y: number) => void, New: (x: string) => void
#[test]
fn test_param_removed_major() {
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![
                ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                },
                ParamShape {
                    name: "y".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::Number),
                },
            ],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "param-removed" && f.severity == Severity::Major),
        "removed param should produce param-removed Major, got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, f.severity.clone()))
            .collect::<Vec<_>>()
    );
    let f = findings.iter().find(|f| f.rule == "param-removed").unwrap();
    assert!(
        f.message.contains("y"),
        "param-removed message should mention param name 'y': {}",
        f.message
    );
}

// Test 24: Opaque produces type-incompatible Major ( explicit test)
#[test]
fn opaque_type_changes_preserve_uncertainty() {
    // Note: test name kept for backward compat, but behavior is now opposite:
    //: Opaque produces type-incompatible Major

    // Opaque on old side
    let findings = diff(
        TypeShape::Opaque("x".to_string()),
        TypeShape::Primitive(PrimitiveKind::String),
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "analysis-incomplete" && f.severity == Severity::Warning),
        "Opaque old + Primitive new should produce incomplete analysis"
    );

    // Opaque on new side
    let findings = diff(
        TypeShape::Primitive(PrimitiveKind::String),
        TypeShape::Opaque("x".to_string()),
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "analysis-incomplete" && f.severity == Severity::Warning),
        "Primitive old + Opaque new should produce incomplete analysis"
    );

    // Both Opaque but different values
    let findings = diff(
        TypeShape::Opaque("x".to_string()),
        TypeShape::Opaque("y".to_string()),
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "analysis-incomplete" && f.severity == Severity::Warning),
        "different Opaques should produce incomplete analysis"
    );
}

// Test 25: Message includes old and new values
#[test]
fn test_message_includes_old_new_values() {
    let findings = diff(
        TypeShape::Primitive(PrimitiveKind::String),
        TypeShape::Primitive(PrimitiveKind::Number),
    );
    assert!(!findings.is_empty());
    let f = &findings[0];
    assert!(
        f.message.contains("string"),
        "message should contain old type 'string': {}",
        f.message
    );
    assert!(
        f.message.contains("number"),
        "message should contain new type 'number': {}",
        f.message
    );
}

// Optional param made required — fires param-made-required Major
#[test]
fn test_param_made_required_fires_major() {
    use crate::languages::typescript::compare::rules::RULE_PARAM_MADE_REQUIRED;
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: true, // was optional
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false, // now required
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    );
    assert!(
            findings
                .iter()
                .any(|f| f.rule == RULE_PARAM_MADE_REQUIRED && f.severity == Severity::Major),
            "optional->required param at position 0 should produce param-made-required Major, got: {:?}",
            findings
                .iter()
                .map(|f| (f.rule, f.severity.clone()))
                .collect::<Vec<_>>()
        );
    let f = findings
        .iter()
        .find(|f| f.rule == RULE_PARAM_MADE_REQUIRED)
        .unwrap();
    assert!(
        f.message.contains("x"),
        "param-made-required message should mention param name 'x': {}",
        f.message
    );
    assert!(
        f.message.contains("position 0"),
        "param-made-required message should mention position: {}",
        f.message
    );
}

// required param stays required — no finding
#[test]
fn test_required_stays_required_no_finding() {
    use crate::languages::typescript::compare::rules::RULE_PARAM_MADE_REQUIRED;
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    );
    assert!(
        !findings.iter().any(|f| f.rule == RULE_PARAM_MADE_REQUIRED),
        "required->required should produce no param-made-required, got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, f.severity.clone()))
            .collect::<Vec<_>>()
    );
}

// required param becomes optional — no finding (loosening is safe)
#[test]
fn test_required_to_optional_no_finding() {
    use crate::languages::typescript::compare::rules::RULE_PARAM_MADE_REQUIRED;
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false, // was required
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: true, // now optional
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    );
    assert!(
        !findings.iter().any(|f| f.rule == RULE_PARAM_MADE_REQUIRED),
        "required->optional should produce no param-made-required (loosening is safe), got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, f.severity.clone()))
            .collect::<Vec<_>>()
    );
}

// REF-06: optional property made required
#[test]
fn test_optional_to_required_property_emits_ref06() {
    use crate::languages::typescript::compare::rules::RULE_PROPERTY_MADE_REQUIRED;
    let mut old_fields = BTreeMap::new();
    old_fields.insert(
        "name".to_string(),
        FieldShape {
            write_type: None,
            optional: true,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        },
    );
    let mut new_fields = BTreeMap::new();
    new_fields.insert(
        "name".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        },
    );
    let findings = diff(TypeShape::Object(old_fields), TypeShape::Object(new_fields));
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_PROPERTY_MADE_REQUIRED),
        "should emit property-made-required when optional becomes required; got: {:?}",
        rule_ids(&findings)
    );
}

// ── IFT-02: Readonly modifier flip on type alias object field ─────────────

fn make_field(optional: bool, readonly: bool) -> FieldShape {
    FieldShape {
        write_type: None,
        optional,
        readonly,
        shape: TypeShape::Primitive(PrimitiveKind::String),
    }
}

#[test]
fn readonly_added_on_type_alias_field_fires_major() {
    let mut old_fields = BTreeMap::new();
    old_fields.insert("foo".to_string(), make_field(false, false));
    let mut new_fields = BTreeMap::new();
    new_fields.insert("foo".to_string(), make_field(false, true));

    let findings = diff(TypeShape::Object(old_fields), TypeShape::Object(new_fields));
    let ift_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "type-alias-readonly-changed")
        .collect();
    assert_eq!(
        ift_findings.len(),
        1,
        "readonly added should fire exactly once; got: {:?}",
        rule_ids(&findings)
    );
    assert_eq!(ift_findings[0].severity, Severity::Major);
    assert!(
        ift_findings[0].message.contains("added"),
        "message should say 'added': {}",
        ift_findings[0].message
    );
    assert!(
        ift_findings[0].message.contains("foo"),
        "message should mention field name: {}",
        ift_findings[0].message
    );
}

#[test]
fn readonly_removed_on_type_alias_field_fires_major() {
    let mut old_fields = BTreeMap::new();
    old_fields.insert("foo".to_string(), make_field(false, true));
    let mut new_fields = BTreeMap::new();
    new_fields.insert("foo".to_string(), make_field(false, false));

    let findings = diff(TypeShape::Object(old_fields), TypeShape::Object(new_fields));
    let ift_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "type-alias-readonly-changed")
        .collect();
    assert_eq!(
        ift_findings.len(),
        1,
        "readonly removed should fire exactly once; got: {:?}",
        rule_ids(&findings)
    );
    assert_eq!(ift_findings[0].severity, Severity::Major);
    assert!(
        ift_findings[0].message.contains("removed"),
        "message should say 'removed': {}",
        ift_findings[0].message
    );
}

#[test]
fn readonly_same_no_finding() {
    let mut old_fields = BTreeMap::new();
    old_fields.insert("foo".to_string(), make_field(false, true));
    let mut new_fields = BTreeMap::new();
    new_fields.insert("foo".to_string(), make_field(false, true));

    let findings = diff(TypeShape::Object(old_fields), TypeShape::Object(new_fields));
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "type-alias-readonly-changed")
            .count(),
        0,
        "same readonly state should NOT fire; got: {:?}",
        rule_ids(&findings)
    );
}

// ── SEM-03: any → T tightening ────────────────────────────────────────────

#[test]
fn any_to_string_fires_narrowing() {
    // Old: fn(x: any) -> void, New: fn(x: string) -> void
    // any → string is TIGHTENING — must fire param-type-narrowed Major
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::Any),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "param-type-narrowed" && f.severity == Severity::Major),
        "SEM-03: any→string must fire param-type-narrowed Major; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, f.severity.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn any_to_any_no_finding() {
    // Old: fn(x: any) -> void, New: fn(x: any) -> void — idempotent, no finding
    let findings = diff(
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::Any),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::Any),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    );
    assert!(
        findings
            .iter()
            .filter(|f| f.rule == "param-type-narrowed")
            .count()
            == 0,
        "SEM-03: any→any must NOT fire param-type-narrowed; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, f.severity.clone()))
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------
// BEH-06: `is_discriminated_union` heuristic unit tests
// ---------------------------------------------------------------------

fn field(shape: TypeShape) -> FieldShape {
    FieldShape {
        write_type: None,
        optional: false,
        readonly: false,
        shape,
    }
}

fn obj_kind(kind_literal: &str, extra_field: &str, extra_shape: TypeShape) -> TypeShape {
    let mut fields = BTreeMap::new();
    fields.insert(
        "kind".to_string(),
        field(TypeShape::Literal(LiteralKind::String(
            kind_literal.to_string(),
        ))),
    );
    fields.insert(extra_field.to_string(), field(extra_shape));
    TypeShape::Object(fields)
}

#[test]
fn is_disc_union_two_kinds_returns_kind() {
    let members = vec![
        obj_kind(
            "circle",
            "radius",
            TypeShape::Primitive(PrimitiveKind::Number),
        ),
        obj_kind(
            "square",
            "side",
            TypeShape::Primitive(PrimitiveKind::Number),
        ),
    ];
    assert_eq!(
        is_discriminated_union(&members),
        Some("kind".to_string()),
        "all-object union with shared `kind` literal discriminant should match"
    );
}

#[test]
fn is_disc_union_primitives_returns_none() {
    let members = vec![
        TypeShape::Primitive(PrimitiveKind::String),
        TypeShape::Primitive(PrimitiveKind::Number),
    ];
    assert_eq!(
        is_discriminated_union(&members),
        None,
        "plain-primitive union must not qualify as discriminated"
    );
}

#[test]
fn is_disc_union_missing_kind_in_one_returns_none() {
    let mut fields_a = BTreeMap::new();
    fields_a.insert(
        "kind".to_string(),
        field(TypeShape::Literal(LiteralKind::String("a".to_string()))),
    );
    let mut fields_b = BTreeMap::new();
    fields_b.insert(
        "other".to_string(),
        field(TypeShape::Primitive(PrimitiveKind::Number)),
    );
    let members = vec![TypeShape::Object(fields_a), TypeShape::Object(fields_b)];
    assert_eq!(
        is_discriminated_union(&members),
        None,
        "discriminant missing in one variant must disqualify"
    );
}

#[test]
fn added_discriminant_values_reports_new_variants() {
    let old = vec![
        obj_kind(
            "circle",
            "radius",
            TypeShape::Primitive(PrimitiveKind::Number),
        ),
        obj_kind(
            "square",
            "side",
            TypeShape::Primitive(PrimitiveKind::Number),
        ),
    ];
    let new = vec![
        obj_kind(
            "circle",
            "radius",
            TypeShape::Primitive(PrimitiveKind::Number),
        ),
        obj_kind(
            "square",
            "side",
            TypeShape::Primitive(PrimitiveKind::Number),
        ),
        obj_kind(
            "triangle",
            "base",
            TypeShape::Primitive(PrimitiveKind::Number),
        ),
    ];
    let added = added_discriminant_values("kind", &old, &new);
    assert_eq!(added.len(), 1, "exactly one variant added; got: {added:?}");
    // `format_type` renders string literals as `"literal"`
    assert_eq!(added[0], "\"triangle\"");
}

// ── DOC-01: paired Severity::Info tests ───────────────────────────

fn make_sym_with_satisfies(name: &str, has_narrowed: bool) -> js_model::ExportSymbol {
    use crate::languages::javascript::model::{ExportKind, ExportSymbol};
    use crate::languages::typescript::types::shape::{PrimitiveKind, TypeShape};
    let satisfies_type = Some(TypeShape::Satisfies {
        constraint: Box::new(TypeShape::Primitive(PrimitiveKind::Unknown)),
        narrowed: if has_narrowed {
            Some(Box::new(TypeShape::Primitive(PrimitiveKind::String)))
        } else {
            None
        },
    });
    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
        file: PathBuf::from("f.ts"),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: Vec::new(),
        type_guard_type: None,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        is_type_only: false,
        release_tag: None,
        satisfies_type,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    }
}

fn make_sym_plain(name: &str) -> js_model::ExportSymbol {
    use crate::languages::javascript::model::{ExportKind, ExportSymbol};
    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
        file: PathBuf::from("f.ts"),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: Vec::new(),
        type_guard_type: None,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        is_type_only: false,
        release_tag: None,
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    }
}

#[test]
fn paired_info_at_satisfies_site() {
    use crate::languages::typescript::compare::rules::{
        RULE_LIMITATION_SATISFIES_NARROWED, RULE_TYPE_NARROWING_WIDENED,
    };
    // old has satisfies with no narrowed (degraded) → new has no satisfies
    let old_sym = make_sym_with_satisfies("cfg", false);
    let new_sym = make_sym_plain("cfg");
    let findings = diff_satisfies_types(&old_sym, &new_sym, &PathBuf::from("f.ts"), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Minor && f.rule == RULE_TYPE_NARROWING_WIDENED),
        "expected Minor type-narrowing-widened; got: {:?}",
        findings
    );
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Info && f.rule == RULE_LIMITATION_SATISFIES_NARROWED),
        "expected Info limitation: satisfies-narrowed-type-not-verified; got: {:?}",
        findings
    );
    assert_eq!(
        findings.len(),
        2,
        "expected exactly 2 findings (Minor + paired Info); got: {:?}",
        findings
    );
}

#[test]
fn paired_info_at_typeof_import_site() {
    use crate::languages::javascript::model::{ExportKey, SymbolSpace};
    use crate::languages::typescript::compare::rules::{
        RULE_LIMITATION_LSP_REQUIRED, RULE_TYPE_REFERENCE_REMOVED,
    };
    use crate::languages::typescript::model::TypeofImportRef;

    let r = TypeofImportRef {
        specifier: "./target".to_string(),
        property: "Z".to_string(),
        property_line: 1,
        property_col: 10,
    };
    let mut old_sym = make_sym_plain("X");
    old_sym.typeof_import_refs = vec![r];
    let new_sym = make_sym_plain("X");

    let old_map = js_model::ExportMap::new();
    let new_map = js_model::ExportMap::new();
    let key = ExportKey {
        subpath: ".".to_string(),
        condition: None,
        space: SymbolSpace::Type,
        name: "X".to_string(),
    };

    let mut findings: Vec<Finding> = Vec::new();
    diff_typeof_import_refs(
        &old_sym,
        &new_sym,
        &old_map,
        &new_map,
        &key,
        &key,
        false, // lsp_was_available = false → degraded path
        &PathBuf::from("f.ts"),
        &mut findings,
    );

    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Minor && f.rule == RULE_TYPE_REFERENCE_REMOVED),
        "expected Minor type-reference-removed; got: {:?}",
        findings
    );
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Info && f.rule == RULE_LIMITATION_LSP_REQUIRED),
        "expected Info limitation: lsp-required; got: {:?}",
        findings
    );
    assert_eq!(
        findings.len(),
        2,
        "expected exactly 2 findings (Minor + paired Info); got: {:?}",
        findings
    );
}

#[test]
fn added_rest_parameters_preserve_calls_that_omit_them() {
    for (rest_type, breaking) in [
        ("number[]", false),
        ("[count?: number]", false),
        ("[count: number]", true),
    ] {
        let before = type_parser::parse_type_string("(name: string) => void");
        let after = type_parser::parse_type_string(&format!(
            "(name: string, ...extra: {rest_type}) => void"
        ));

        let findings = diff(before, after);

        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.severity == Severity::Major),
            breaking,
            "{rest_type}: {findings:?}"
        );
    }
}

#[test]
fn returned_writable_properties_preserve_accepted_assignments() {
    for (before, after, breaking) in [
        ("{ value: string | number }", "{ value: string }", true),
        (
            "{ readonly value: string | number }",
            "{ readonly value: string }",
            false,
        ),
        (
            "{ nested: { value: string | number } }",
            "{ nested: { value: string } }",
            true,
        ),
    ] {
        let before = type_parser::parse_type_string(before);
        let after = type_parser::parse_type_string(after);

        let findings = diff_values(&before, &after, "api", &dummy_file(), 1, 1);

        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.severity == Severity::Major),
            breaking,
            "{before:?} -> {after:?}: {findings:?}"
        );
    }
}

#[test]
fn receiver_requirements_compare_contravariantly() {
    for (before, after, breaking) in [
        ("() => void", "(this: { ready: boolean }) => void", true),
        ("(this: { ready: boolean }) => void", "() => void", false),
        (
            "(this: { ready: boolean }) => void",
            "(this: { ready: boolean; id: string }) => void",
            true,
        ),
    ] {
        let findings = diff(
            type_parser::parse_type_string(before),
            type_parser::parse_type_string(after),
        );

        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.severity == Severity::Major),
            breaking,
            "{findings:?}"
        );
    }
}

#[test]
fn predicates_preserve_true_and_false_branch_narrowing() {
    use crate::languages::typescript::types::shape::{PredicateTarget, TypePredicateShape};

    for (after, assertion, breaking) in [
        ("string | number", false, true),
        ("\"ready\"", false, true),
        ("string", false, false),
        ("\"ready\"", true, false),
    ] {
        let signature = |guarded: &str| {
            let mut shape = type_parser::parse_type_string("(input: unknown) => boolean");
            let TypeShape::Function { contract, .. } = &mut shape else {
                unreachable!()
            };
            contract.predicate = Some(TypePredicateShape {
                target: PredicateTarget::Parameter(0),
                asserts: assertion,
                shape: Some(Box::new(type_parser::parse_type_string(guarded))),
            });
            shape
        };

        let findings = diff(signature("string"), signature(after));

        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.severity == Severity::Major),
            breaking,
            "{after}: {findings:?}"
        );
    }
}

#[test]
fn returned_mutable_containers_preserve_write_domains() {
    use crate::languages::typescript::types::parser::parse_type_string;
    for container in [
        "(string | number)[]",
        "[string | number]",
        "Map<string, string | number>",
        "Set<string | number>",
        "Record<string, string | number>",
        "Promise<(string | number)[]>",
    ] {
        let before = format!("() => {container}");
        let after = before.replace("string | number", "string");
        let findings = diff(parse_type_string(&before), parse_type_string(&after));
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == RULE_FIELD_TYPE_CHANGED),
            "{container}: {findings:?}"
        );
    }
}

#[test]
fn returned_callables_preserve_optional_and_rest_argument_counts() {
    use crate::languages::typescript::types::parser::parse_type_string;
    for (before, after) in [
        ("(x: string, y?: number) => void", "(x: string) => void"),
        ("() => void", "(...args: [x: string]) => void"),
        ("(...args: string[]) => void", "(x?: string) => void"),
    ] {
        let findings = diff(
            parse_type_string(&format!("() => ({before})")),
            parse_type_string(&format!("() => ({after})")),
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.severity == Severity::Major),
            "{before} -> {after}: {findings:?}"
        );
    }
}

#[test]
fn comparison_depth_exhaustion_reports_uncertainty_instead_of_a_break() {
    use crate::languages::typescript::types::parser::parse_type_string;
    let mut old = "string | number".to_string();
    let mut new = "string".to_string();
    for _ in 0..22 {
        old = format!("{{readonly x: {old}}}");
        new = format!("{{readonly x: {new}}}");
    }
    let findings = diff(
        parse_type_string(&format!("() => {old}")),
        parse_type_string(&format!("() => {new}")),
    );
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].rule, "analysis-incomplete");
    assert_eq!(findings[0].severity, Severity::Warning);
}
