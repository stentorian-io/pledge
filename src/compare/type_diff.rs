// Structural TypeShape diff engine — compares two type shapes and produces findings.
//
// Design decisions:
// - D-08: recursive shape diff
// - D-09: skip when either side is Opaque (incomparable)
// - D-10: max recursion depth of 20 levels
// - D-11: normalization applied before comparison
// - D-19: rule IDs: type-changed, field-removed, field-type-changed, field-added

use std::path::Path;

use crate::types::{Finding, Severity};
use crate::types::shape::TypeShape;

const MAX_DEPTH: usize = 20;

/// Compare two TypeShapes structurally and produce findings for differences.
///
/// `symbol_name`: the export symbol name (for finding messages)
/// `file`: file path for the finding location
/// `line`, `col`: source position for the finding
///
/// Per D-09: if either shape is Opaque, returns empty (incomparable).
/// Per D-10: recursion depth capped at 20 levels.
pub fn diff_types(
    old: &TypeShape,
    new: &TypeShape,
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    diff_recursive(old, new, symbol_name, "", file, line, col, 0, &mut findings);
    findings
}

#[allow(clippy::too_many_arguments)]
fn diff_recursive(
    old: &TypeShape,
    new: &TypeShape,
    symbol_name: &str,
    field_path: &str,
    file: &Path,
    line: u32,
    col: u32,
    depth: usize,
    findings: &mut Vec<Finding>,
) {
    // D-10: stop at max depth with a warning-level finding
    if depth >= MAX_DEPTH {
        findings.push(Finding {
            severity: Severity::Patch,
            rule: "type-comparison-truncated",
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "type comparison truncated at depth {MAX_DEPTH} for `{symbol_name}`"
            ),
        });
        return;
    }

    // D-09: skip if either side is Opaque
    if matches!(old, TypeShape::Opaque(_)) || matches!(new, TypeShape::Opaque(_)) {
        return;
    }

    // Same type — no findings
    if old == new {
        return;
    }

    // Normalize for comparison
    let old_n = old.clone().normalize();
    let new_n = new.clone().normalize();

    // After normalization, check equality again
    if old_n == new_n {
        return;
    }

    match (&old_n, &new_n) {
        // Both Opaque (after normalization) — skip
        (TypeShape::Opaque(_), _) | (_, TypeShape::Opaque(_)) => {}

        // Both Object — compare field sets
        (TypeShape::Object(old_fields), TypeShape::Object(new_fields)) => {
            // Fields in old but not new → field-removed (Major)
            for field_name in old_fields.keys() {
                if !new_fields.contains_key(field_name) {
                    let path = make_path(field_path, field_name);
                    findings.push(Finding {
                        severity: Severity::Major,
                        rule: "field-removed",
                        file: file.to_path_buf(),
                        line,
                        col,
                        message: format!(
                            "field `{path}` was removed from type `{symbol_name}`"
                        ),
                    });
                }
            }

            // Fields in new but not old → field-added
            for (field_name, new_field) in new_fields {
                if !old_fields.contains_key(field_name) {
                    let path = make_path(field_path, field_name);
                    let (severity, optional_label) = if new_field.optional {
                        (Severity::Minor, "optional ")
                    } else {
                        (Severity::Major, "required ")
                    };
                    findings.push(Finding {
                        severity,
                        rule: "field-added",
                        file: file.to_path_buf(),
                        line,
                        col,
                        message: format!(
                            "{optional_label}field `{path}` was added to type `{symbol_name}`"
                        ),
                    });
                }
            }

            // Fields in both — recursively diff shapes
            for (field_name, old_field) in old_fields {
                if let Some(new_field) = new_fields.get(field_name) {
                    let child_path = make_path(field_path, field_name);
                    // If the shapes differ, recurse to produce specific findings
                    if old_field.shape != new_field.shape {
                        // Recurse to produce inner findings. If recursion produces no
                        // findings (e.g. both Opaque), emit a field-type-changed finding.
                        let prev_len = findings.len();
                        diff_recursive(
                            &old_field.shape,
                            &new_field.shape,
                            symbol_name,
                            &child_path,
                            file,
                            line,
                            col,
                            depth + 1,
                            findings,
                        );
                        // If recursion added nothing (incomparable types), emit field-type-changed
                        if findings.len() == prev_len {
                            findings.push(Finding {
                                severity: Severity::Major,
                                rule: "field-type-changed",
                                file: file.to_path_buf(),
                                line,
                                col,
                                message: format!(
                                    "field `{child_path}` changed type in `{symbol_name}`"
                                ),
                            });
                        }
                    }
                }
            }
        }

        // Both Array — diff inner types
        (TypeShape::Array(old_inner), TypeShape::Array(new_inner)) => {
            diff_recursive(
                old_inner,
                new_inner,
                symbol_name,
                field_path,
                file,
                line,
                col,
                depth + 1,
                findings,
            );
            // If no inner findings but types differ, emit type-changed
            // (handled by the outer type-changed case if variants differ,
            // but if both are Array with different element types we need to emit)
        }

        // Both Tuple — compare element counts and diff each position
        (TypeShape::Tuple(old_elems), TypeShape::Tuple(new_elems)) => {
            if old_elems.len() != new_elems.len() {
                findings.push(make_type_changed(symbol_name, field_path, file, line, col));
            } else {
                for (i, (old_elem, new_elem)) in
                    old_elems.iter().zip(new_elems.iter()).enumerate()
                {
                    let idx_path = if field_path.is_empty() {
                        format!("[{i}]")
                    } else {
                        format!("{field_path}[{i}]")
                    };
                    diff_recursive(
                        old_elem,
                        new_elem,
                        symbol_name,
                        &idx_path,
                        file,
                        line,
                        col,
                        depth + 1,
                        findings,
                    );
                }
            }
        }

        // Both Union — check for widening vs narrowing
        (TypeShape::Union(old_members), TypeShape::Union(new_members)) => {
            // Use Debug-repr set membership to check if all old members are in new
            let old_reprs: std::collections::HashSet<String> =
                old_members.iter().map(|m| format!("{m:?}")).collect();
            let new_reprs: std::collections::HashSet<String> =
                new_members.iter().map(|m| format!("{m:?}")).collect();

            let narrowed: Vec<_> = old_reprs.difference(&new_reprs).collect();
            if !narrowed.is_empty() {
                // Union narrowed — members removed — Major
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: "type-changed",
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: make_type_changed_msg(symbol_name, field_path),
                });
            }
            // Widening (new has more members than old) is safe — no finding or Minor
            // Per spec: "minor or no finding" — we emit nothing for pure widening
        }

        // Both Intersection — adding a member narrows the type (Major)
        (TypeShape::Intersection(old_members), TypeShape::Intersection(new_members)) => {
            let old_reprs: std::collections::HashSet<String> =
                old_members.iter().map(|m| format!("{m:?}")).collect();
            let new_reprs: std::collections::HashSet<String> =
                new_members.iter().map(|m| format!("{m:?}")).collect();

            // New members added to intersection → narrows → Major
            let added: Vec<_> = new_reprs.difference(&old_reprs).collect();
            // Old members removed from intersection → widens → safe
            if !added.is_empty() {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: "type-changed",
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: make_type_changed_msg(symbol_name, field_path),
                });
            }
        }

        // Both Function — compare params and return type
        (
            TypeShape::Function {
                params: old_params,
                return_type: old_ret,
            },
            TypeShape::Function {
                params: new_params,
                return_type: new_ret,
            },
        ) => {
            // Count required params in each
            let old_required = old_params.iter().filter(|p| !p.optional).count();
            let new_required = new_params.iter().filter(|p| !p.optional).count();

            if new_required > old_required {
                // Added required params — Major
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: "type-changed",
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "function `{symbol_name}` has more required parameters"
                    ),
                });
            } else if new_params.len() < old_params.len() {
                // Removed params — Major
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: "type-changed",
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "function `{symbol_name}` has fewer parameters"
                    ),
                });
            } else {
                // Same param count — diff each param type
                for (i, (old_param, new_param)) in
                    old_params.iter().zip(new_params.iter()).enumerate()
                {
                    let param_path = if field_path.is_empty() {
                        format!("param[{i}]")
                    } else {
                        format!("{field_path}.param[{i}]")
                    };
                    diff_recursive(
                        &old_param.shape,
                        &new_param.shape,
                        symbol_name,
                        &param_path,
                        file,
                        line,
                        col,
                        depth + 1,
                        findings,
                    );
                }
            }

            // Return type diff
            let ret_path = if field_path.is_empty() {
                "return".to_string()
            } else {
                format!("{field_path}.return")
            };
            diff_recursive(
                old_ret,
                new_ret,
                symbol_name,
                &ret_path,
                file,
                line,
                col,
                depth + 1,
                findings,
            );
        }

        // Both Generic — compare names and args
        (
            TypeShape::Generic {
                name: old_name,
                args: old_args,
            },
            TypeShape::Generic {
                name: new_name,
                args: new_args,
            },
        ) => {
            if old_name != new_name || old_args.len() != new_args.len() {
                findings.push(make_type_changed(symbol_name, field_path, file, line, col));
            } else {
                for (i, (old_arg, new_arg)) in old_args.iter().zip(new_args.iter()).enumerate() {
                    let arg_path = if field_path.is_empty() {
                        format!("arg[{i}]")
                    } else {
                        format!("{field_path}.arg[{i}]")
                    };
                    diff_recursive(
                        old_arg,
                        new_arg,
                        symbol_name,
                        &arg_path,
                        file,
                        line,
                        col,
                        depth + 1,
                        findings,
                    );
                }
            }
        }

        // Both Primitive — if we got here after equality check, they differ
        (TypeShape::Primitive(_), TypeShape::Primitive(_)) => {
            findings.push(make_type_changed(symbol_name, field_path, file, line, col));
        }

        // Both Literal — if we got here after equality check, they differ
        (TypeShape::Literal(_), TypeShape::Literal(_)) => {
            findings.push(make_type_changed(symbol_name, field_path, file, line, col));
        }

        // Different variant entirely — emit type-changed (Major)
        _ => {
            // Handle Array case where we already recursed but emit if no findings
            findings.push(make_type_changed(symbol_name, field_path, file, line, col));
        }
    }
}

/// Build a dot-separated path for nested field access.
fn make_path(prefix: &str, field: &str) -> String {
    if prefix.is_empty() {
        field.to_string()
    } else {
        format!("{prefix}.{field}")
    }
}

fn make_type_changed_msg(symbol_name: &str, field_path: &str) -> String {
    if field_path.is_empty() {
        format!("type of `{symbol_name}` changed")
    } else {
        format!("type of `{symbol_name}.{field_path}` changed")
    }
}

fn make_type_changed(
    symbol_name: &str,
    field_path: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Finding {
    Finding {
        severity: Severity::Major,
        rule: "type-changed",
        file: file.to_path_buf(),
        line,
        col,
        message: make_type_changed_msg(symbol_name, field_path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::shape::{FieldShape, LiteralKind, ParamShape, PrimitiveKind};
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
        assert!(findings.is_empty(), "identical shapes should produce no findings");
    }

    // Test 2: Different primitives produce type-changed
    #[test]
    fn test_different_primitives_type_changed() {
        let findings = diff(
            TypeShape::Primitive(PrimitiveKind::String),
            TypeShape::Primitive(PrimitiveKind::Number),
        );
        assert_eq!(rule_ids(&findings), vec!["type-changed"]);
        assert_eq!(findings[0].severity, Severity::Major);
    }

    // Test 3: Object field removed
    #[test]
    fn test_field_removed() {
        let mut old_fields = BTreeMap::new();
        old_fields.insert(
            "x".to_string(),
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::Number) },
        );
        old_fields.insert(
            "y".to_string(),
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::String) },
        );
        let mut new_fields = BTreeMap::new();
        new_fields.insert(
            "x".to_string(),
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::Number) },
        );

        let findings = diff(TypeShape::Object(old_fields), TypeShape::Object(new_fields));
        assert!(rule_ids(&findings).contains(&"field-removed"));
        let removed = findings.iter().find(|f| f.rule == "field-removed").unwrap();
        assert!(removed.message.contains("`y`"), "message should mention field 'y'");
        assert_eq!(removed.severity, Severity::Major);
    }

    // Test 4: Object field type changed
    #[test]
    fn test_field_type_changed() {
        let mut old_fields = BTreeMap::new();
        old_fields.insert(
            "x".to_string(),
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::Number) },
        );
        let mut new_fields = BTreeMap::new();
        new_fields.insert(
            "x".to_string(),
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::String) },
        );

        let findings = diff(TypeShape::Object(old_fields), TypeShape::Object(new_fields));
        // Should have type-changed (from primitive diff) for the field
        assert!(!findings.is_empty(), "should have findings for changed field type");
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
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::Number) },
        );
        let mut new_fields = BTreeMap::new();
        new_fields.insert(
            "x".to_string(),
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::Number) },
        );
        new_fields.insert(
            "y".to_string(),
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::String) },
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
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::Number) },
        );
        let mut new_fields = BTreeMap::new();
        new_fields.insert(
            "x".to_string(),
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::Number) },
        );
        new_fields.insert(
            "y".to_string(),
            FieldShape { optional: true, shape: TypeShape::Primitive(PrimitiveKind::String) },
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
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::Number) },
        );
        let mut inner_new = BTreeMap::new();
        inner_new.insert(
            "timeout".to_string(),
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::String) },
        );

        let mut old_fields = BTreeMap::new();
        old_fields.insert(
            "config".to_string(),
            FieldShape { optional: false, shape: TypeShape::Object(inner_old) },
        );
        let mut new_fields = BTreeMap::new();
        new_fields.insert(
            "config".to_string(),
            FieldShape { optional: false, shape: TypeShape::Object(inner_new) },
        );

        let findings = diff(TypeShape::Object(old_fields), TypeShape::Object(new_fields));
        assert!(!findings.is_empty());
        // Should mention the nested path "config.timeout"
        assert!(
            findings.iter().any(|f| f.message.contains("config.timeout")),
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
        assert!(findings.iter().any(|f| f.rule == "type-changed"));
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

    // Test 10: Union narrowed — Major
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
        assert!(findings.iter().any(|f| f.rule == "type-changed"));
    }

    // Test 11: Opaque on either side — no findings (incomparable)
    #[test]
    fn test_opaque_either_side_no_findings() {
        // Opaque on old side
        let findings = diff(
            TypeShape::Opaque("x".to_string()),
            TypeShape::Primitive(PrimitiveKind::String),
        );
        assert!(findings.is_empty(), "Opaque old + Primitive new should be empty");

        // Opaque on new side
        let findings = diff(
            TypeShape::Primitive(PrimitiveKind::String),
            TypeShape::Opaque("x".to_string()),
        );
        assert!(findings.is_empty(), "Primitive old + Opaque new should be empty");

        // Both Opaque
        let findings = diff(
            TypeShape::Opaque("x".to_string()),
            TypeShape::Opaque("y".to_string()),
        );
        assert!(findings.is_empty(), "both Opaque should be empty");
    }

    // Test 12: Max recursion depth 20 — stops at depth 20 without panicking
    #[test]
    fn test_max_recursion_depth() {
        // Build a deeply nested object: {a: {a: {a: ... (21 levels)}}}
        fn nested_object(depth: usize) -> TypeShape {
            if depth == 0 {
                TypeShape::Primitive(PrimitiveKind::String)
            } else {
                let mut fields = BTreeMap::new();
                fields.insert(
                    "a".to_string(),
                    FieldShape { optional: false, shape: nested_object(depth - 1) },
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
                    FieldShape { optional: false, shape: nested_object_num(depth - 1) },
                );
                TypeShape::Object(fields)
            }
        }

        // Should not panic, should produce at most MAX_DEPTH-bounded findings
        let findings = diff(nested_object(22), nested_object_num(22));
        assert!(!findings.is_empty(), "should find some change in deeply nested object");
        // The truncation finding should be present
        assert!(
            findings.iter().any(|f| f.rule == "type-comparison-truncated"),
            "should have truncation finding at depth 20"
        );
    }

    // Test 13: Function return type changed
    #[test]
    fn test_function_return_type_changed() {
        let findings = diff(
            TypeShape::Function {
                params: vec![ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Number)),
            },
            TypeShape::Function {
                params: vec![ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::String)),
            },
        );
        assert!(!findings.is_empty(), "return type change should produce findings");
        // Should mention return type
        assert!(
            findings.iter().any(|f| f.message.contains("return") || f.rule == "type-changed"),
            "should produce type-changed finding for return type"
        );
    }

    // Test 14: Function param added (required) — Major
    #[test]
    fn test_function_required_param_added() {
        let findings = diff(
            TypeShape::Function {
                params: vec![ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
            },
            TypeShape::Function {
                params: vec![
                    ParamShape {
                        name: "x".to_string(),
                        optional: false,
                        shape: TypeShape::Primitive(PrimitiveKind::String),
                    },
                    ParamShape {
                        name: "y".to_string(),
                        optional: false,
                        shape: TypeShape::Primitive(PrimitiveKind::Number),
                    },
                ],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
            },
        );
        assert!(!findings.is_empty(), "adding required param should produce findings");
        assert!(findings.iter().any(|f| f.severity == Severity::Major));
    }

    // Extra: literal type changed
    #[test]
    fn test_literal_type_changed() {
        let findings = diff(
            TypeShape::Literal(LiteralKind::String("foo".to_string())),
            TypeShape::Literal(LiteralKind::String("bar".to_string())),
        );
        assert!(!findings.is_empty());
        assert!(findings.iter().any(|f| f.rule == "type-changed"));
    }

    // Extra: identical complex object — no findings
    #[test]
    fn test_identical_objects_no_findings() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "x".to_string(),
            FieldShape { optional: false, shape: TypeShape::Primitive(PrimitiveKind::Number) },
        );
        fields.insert(
            "y".to_string(),
            FieldShape { optional: true, shape: TypeShape::Primitive(PrimitiveKind::String) },
        );
        let findings = diff(
            TypeShape::Object(fields.clone()),
            TypeShape::Object(fields),
        );
        assert!(findings.is_empty(), "identical objects should produce no findings");
    }
}
