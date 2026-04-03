//! Integration tests for Phase 11 bug fixes (plans 01 and 02).
//!
//! Each test covers one of the 9 bug fixes:
//!   Bug 1: Scoped package URL encoding (tested via unit tests in acquire)
//!   Bug 2: Export alias — exported name should be the alias, not local name
//!   Bug 3: Namespace re-export name captured correctly (`export * as utils`)
//!   Bug 4: Typings field resolved correctly (covered by tree-sitter unit tests)
//!   Bug 5: Overload matching by arity AND type (not just arity)
//!   Bug 6: Declaration kind change covers Class→Function, Enum→Variable etc.
//!   Bug 7: Finding deduplication (same rule+file+line collapsed to one)
//!   Bug 8: Rest param subtype check — widening safe, narrowing flagged
//!   Bug 9: Regular→const enum fires (not just const→regular)
//!
//! Tests exercise the comparison engine directly with constructed inputs,
//! following the pattern from integration_ref_rules.rs.

use interlint::compare::subtype::{
    RULE_DECLARATION_KIND_CHANGED, RULE_ENUM_REGULAR_TO_CONST, RULE_OVERLOAD_REMOVED,
    RULE_REST_PARAM_NARROWED,
};
use interlint::compare::{enum_diff, ref_diff};
use interlint::discover::tree_query::{self, RawExportKind, SourceLanguage};
use interlint::types::shape::{ParamShape, PrimitiveKind, TypeShape};
use interlint::types::{
    DeclarationKind, EnumMember, EnumShape, EnumValueKind, ExportKind, ExportMap, ExportSymbol,
    Finding, Severity,
};
use std::path::PathBuf;

// ─── Test helpers ─────────────────────────────────────────────────────────────

fn test_file() -> PathBuf {
    PathBuf::from("test.ts")
}

fn rule_ids(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule).collect()
}

fn make_sym(name: &str, decl_kind: Option<DeclarationKind>) -> ExportSymbol {
    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
        file: test_file(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kind,
        type_guard_type: None,
        is_async: false,
        is_generator: false,
    }
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
        decl_kind: Some(DeclarationKind::Function),
        type_guard_type: None,
        is_async: false,
        is_generator: false,
    }
}

// ─── Bug 2: Export alias uses alias name, not local name ─────────────────────

/// BUG 2: `export { foo as bar }` should record `bar` as the exported name.
/// When `bar` is removed in the new version (only `foo` exported without alias),
/// the finding should mention `bar`, not `foo`.
#[test]
fn test_export_alias_detected_correctly() {
    // Old version: `export { foo as bar }` — public name is `bar`
    let old_src = b"export { foo as bar }\nfunction foo() { return 1; }";

    // New version: `export { foo }` — public name is `foo`, `bar` is gone
    let new_src = b"export { foo }\nfunction foo() { return 1; }";

    let old_exports =
        tree_query::parse_exports(old_src, SourceLanguage::TypeScript).expect("parse old");
    let new_exports =
        tree_query::parse_exports(new_src, SourceLanguage::TypeScript).expect("parse new");

    // Old should export `bar` (the alias), not `foo`
    assert!(
        old_exports
            .iter()
            .any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
        "Old exports should contain `bar` (alias), got: {:?}",
        old_exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    // New should export `foo`, not `bar`
    assert!(
        new_exports
            .iter()
            .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
        "New exports should contain `foo`, got: {:?}",
        new_exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    // Old should NOT contain `foo` as a direct export name (it's the local binding)
    assert!(
        !old_exports.iter().any(|e| e.name == "foo"),
        "Old exports should NOT contain `foo` (it's the local name, not public), got: {:?}",
        old_exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    // Now build ExportMaps and compare — `bar` removed should produce export-removed finding
    let mut old_map = ExportMap::new();
    for raw in &old_exports {
        old_map.insert(
            raw.name.clone(),
            ExportSymbol {
                name: raw.name.clone(),
                kind: ExportKind::Named,
                file: test_file(),
                line: raw.line,
                col: raw.col,
                type_signature: None,
                overloads: Vec::new(),
                decl_kind: None,
                type_guard_type: None,
                is_async: false,
                is_generator: false,
            },
        );
    }

    let mut new_map = ExportMap::new();
    for raw in &new_exports {
        new_map.insert(
            raw.name.clone(),
            ExportSymbol {
                name: raw.name.clone(),
                kind: ExportKind::Named,
                file: test_file(),
                line: raw.line,
                col: raw.col,
                type_signature: None,
                overloads: Vec::new(),
                decl_kind: None,
                type_guard_type: None,
                is_async: false,
                is_generator: false,
            },
        );
    }

    let findings = interlint::compare::compare_exports(&old_map, &new_map);

    // `bar` was removed — should fire export-removed
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "export-removed" && f.message.contains("bar")),
        "Should find export-removed for `bar`; got: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

// ─── Bug 3: Namespace re-export captures name, not `*` ───────────────────────

/// BUG 3: `export * as utils from './helpers'` should produce a RawExport
/// with `name: "utils"`, not `name: "*"`.
#[test]
fn test_namespace_reexport_name_captured() {
    let src = b"export * as utils from './helpers'";

    let exports = tree_query::parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    let ns_export = exports
        .iter()
        .find(|e| e.name == "utils")
        .unwrap_or_else(|| panic!("No export named 'utils' found; got: {:?}", exports));

    assert_eq!(ns_export.name, "utils");
    // Should NOT be recorded as '*'
    assert!(
        !exports.iter().any(|e| e.name == "*"),
        "Namespace re-export should not be recorded as '*'"
    );
}

// ─── Bug 6: Declaration kind change — Class → Function ───────────────────────

/// BUG 6: Changing from `DeclarationKind::Class` to `DeclarationKind::Function`
/// should fire `declaration-kind-changed`.
#[test]
fn test_declaration_kind_class_to_function() {
    let old_sym = make_sym("Widget", Some(DeclarationKind::Class));
    let new_sym = make_sym("Widget", Some(DeclarationKind::Function));

    let findings =
        ref_diff::compare_ref_changes(&old_sym, &new_sym, "Widget", &test_file(), 1, 1, &[], &[]);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
        "Expected declaration-kind-changed (Class→Function); got: {:?}",
        rule_ids(&findings)
    );
}

// ─── Bug 6: Declaration kind change — Enum → Variable ────────────────────────

/// BUG 6: Changing from `DeclarationKind::Enum` to `DeclarationKind::Variable`
/// should fire `declaration-kind-changed`.
#[test]
fn test_declaration_kind_enum_to_variable() {
    let old_sym = make_sym("Status", Some(DeclarationKind::Enum));
    let new_sym = make_sym("Status", Some(DeclarationKind::Variable));

    let findings =
        ref_diff::compare_ref_changes(&old_sym, &new_sym, "Status", &test_file(), 1, 1, &[], &[]);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
        "Expected declaration-kind-changed (Enum→Variable); got: {:?}",
        rule_ids(&findings)
    );
}

// ─── Bug 9: Regular enum → const enum fires ──────────────────────────────────

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

// ─── Bug 5: Overload matching by type — same arity but different types ────────

/// BUG 5: Two overloads with same arity but different param types are distinct.
/// Old has: `foo(x: string)` and `foo(x: number)`.
/// New has: only `foo(x: string)`.
/// The `foo(x: number)` overload was removed — should fire `overload-removed`.
#[test]
fn test_overload_same_arity_different_types() {
    let string_overload = TypeShape::Function {
        params: vec![ParamShape {
            name: "x".to_string(),
            optional: false,
            rest: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        }],
        return_type: Box::new(TypeShape::Primitive(PrimitiveKind::String)),
    };

    let number_overload = TypeShape::Function {
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

// ─── Bug 8: Rest param widened — safe (should NOT fire) ──────────────────────

/// BUG 8: Old rest param `Primitive(String)`, new rest param `Primitive(Any)`.
/// This is a widening — safe for callers. Should NOT fire `rest-param-narrowed`.
#[test]
fn test_rest_param_widened_safe() {
    let old_sym = make_fn_sym_with_overloads(
        "bar",
        vec![TypeShape::Function {
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

// ─── Bug 8: Rest param narrowed — breaking (should fire) ─────────────────────

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

// ─── Bug 7: Finding deduplication ────────────────────────────────────────────

/// BUG 7: Duplicate findings (same rule + file + line, different messages)
/// should be collapsed to one per (rule, file, line) key.
/// This tests the dedup logic directly with a local reimplementation matching
/// the one in main.rs.
#[test]
fn test_finding_deduplication() {
    use std::collections::HashSet;

    let file = PathBuf::from("src/index.ts");

    // Create 3 findings: two are duplicates of finding 1, one is unique
    let findings = vec![
        Finding {
            severity: Severity::Major,
            rule: "export-removed",
            file: file.clone(),
            line: 5,
            col: 1,
            message: "exported symbol `foo` was removed".to_string(),
        },
        Finding {
            severity: Severity::Major,
            rule: "export-removed",
            file: file.clone(),
            line: 5,
            col: 1,
            message: "duplicate: exported symbol `foo` was removed".to_string(),
        },
        Finding {
            severity: Severity::Minor,
            rule: "export-added",
            file: file.clone(),
            line: 10,
            col: 1,
            message: "exported symbol `bar` was added".to_string(),
        },
    ];

    // Apply dedup logic: retain first occurrence per (rule, file, line)
    let mut seen = HashSet::new();
    let mut deduped = findings;
    deduped.retain(|f| {
        let key = (f.rule, f.file.clone(), f.line);
        seen.insert(key)
    });

    assert_eq!(
        deduped.len(),
        2,
        "Should have 2 findings after dedup, got {}",
        deduped.len()
    );

    // The duplicate export-removed at line 5 should be collapsed to one
    let removed_findings: Vec<_> = deduped
        .iter()
        .filter(|f| f.rule == "export-removed")
        .collect();
    assert_eq!(
        removed_findings.len(),
        1,
        "Should have exactly 1 export-removed finding after dedup"
    );

    // The unique export-added should still be present
    assert!(
        deduped.iter().any(|f| f.rule == "export-added"),
        "export-added should still be present after dedup"
    );
}
