//! Overload matching compatibility checks.

use pledge::core::findings::Finding;
use pledge::languages::javascript::model::{DeclarationKind, ExportKind, ExportSymbol};
use pledge::languages::typescript::compare::refinements as ref_diff;
use pledge::languages::typescript::compare::rules::{
    RULE_OVERLOAD_REMOVED, RULE_REST_PARAM_NARROWED,
};
use pledge::languages::typescript::types::shape::{ParamShape, PrimitiveKind, TypeShape};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("test.ts")
}

fn make_fn_sym_with_overloads(name: &str, overloads: Vec<TypeShape>) -> ExportSymbol {
    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
        file: test_file(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads,
        decl_kinds: vec![DeclarationKind::Function],
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

/// BUG 5: Two overloads with same arity but different param types are distinct.
/// Old has: `foo(x: string)` and `foo(x: number)`.
/// New has: only `foo(x: string)`.
/// The `foo(x: number)` overload was removed — should fire `overload-removed`.
#[test]
fn test_overload_same_arity_different_types() {
    let string_overload = TypeShape::Function {
        contract: Default::default(),
        params: vec![ParamShape {
            name: "x".to_string(),
            optional: false,
            rest: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        }],
        return_type: Box::new(TypeShape::Primitive(PrimitiveKind::String)),
    };

    let number_overload = TypeShape::Function {
        contract: Default::default(),
        params: vec![ParamShape {
            name: "x".to_string(),
            optional: false,
            rest: false,
            shape: TypeShape::Primitive(PrimitiveKind::Number),
        }],
        return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Number)),
    };

    // Old has both overloads (arity 1 each, different types)
    let old_sym = make_fn_sym_with_overloads("foo", vec![string_overload.clone(), number_overload]);

    // New has only the string overload
    let new_sym = make_fn_sym_with_overloads("foo", vec![string_overload]);

    let findings =
        ref_diff::compare_ref_changes(&old_sym, &new_sym, "foo", &test_file(), 1, 1, &[], &[]);

    assert!(
        findings.iter().any(|f| f.rule == RULE_OVERLOAD_REMOVED),
        "Expected overload-removed for removed number overload; got: {:?}",
        rule_ids(&findings)
    );
}

/// BUG 8: Old rest param `Primitive(String)`, new rest param `Primitive(Any)`.
/// This is a widening — safe for callers. Should NOT fire `rest-param-narrowed`.
#[test]
fn test_rest_param_widened_safe() {
    let old_sym = make_fn_sym_with_overloads(
        "bar",
        vec![TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "args".to_string(),
                optional: false,
                rest: true,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        }],
    );

    let new_sym = make_fn_sym_with_overloads(
        "bar",
        vec![TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "args".to_string(),
                optional: false,
                rest: true,
                shape: TypeShape::Primitive(PrimitiveKind::Any),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        }],
    );

    let findings =
        ref_diff::compare_ref_changes(&old_sym, &new_sym, "bar", &test_file(), 1, 1, &[], &[]);

    assert!(
        !findings.iter().any(|f| f.rule == RULE_REST_PARAM_NARROWED),
        "rest-param-narrowed should NOT fire for widening (string→any); got: {:?}",
        rule_ids(&findings)
    );
}

/// BUG 8: Old rest param `Primitive(Number)` union `Primitive(String)`, new rest param
/// `Primitive(String)` only. This is a narrowing — breaks callers passing numbers.
/// Should fire `rest-param-narrowed`.
///
/// Note: `any → string` is NOT considered narrowing in the implementation because
/// TypeScript's `any` is treated as both top and bottom type (is_subtype_of(Any, String)
/// returns true). Use concrete types to test narrowing.
#[test]
fn test_rest_param_narrowed_flagged() {
    // Old: rest param accepts `string | number`
    let old_sym = make_fn_sym_with_overloads(
        "bar",
        vec![TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "args".to_string(),
                optional: false,
                rest: true,
                shape: TypeShape::Union(vec![
                    TypeShape::Primitive(PrimitiveKind::String),
                    TypeShape::Primitive(PrimitiveKind::Number),
                ]),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        }],
    );

    // New: rest param accepts only `string`
    let new_sym = make_fn_sym_with_overloads(
        "bar",
        vec![TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "args".to_string(),
                optional: false,
                rest: true,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        }],
    );

    let findings =
        ref_diff::compare_ref_changes(&old_sym, &new_sym, "bar", &test_file(), 1, 1, &[], &[]);

    assert!(
        findings.iter().any(|f| f.rule == RULE_REST_PARAM_NARROWED),
        "Expected rest-param-narrowed for narrowing (string|number→string); got: {:?}",
        rule_ids(&findings)
    );
}

fn rule_ids(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule).collect()
}
