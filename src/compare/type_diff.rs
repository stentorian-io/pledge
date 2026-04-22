// Structural TypeShape diff engine — compares two type shapes and produces findings.
//
// Design decisions:
// - D-08: recursive shape diff
// - D-10: Opaque/unresolvable types assume breaking (changed from D-09 skip behavior)
// - D-11: normalization applied before comparison
// - D-02: all findings use fine-grained rule IDs — no generic "type-changed" rule
// - D-04: finding messages include both old and new type values
// - SIG-01..05: variance-aware function signature analysis

use std::path::Path;

use super::subtype::{
    format_type, is_subtype_of, RULE_FIELD_ADDED, RULE_FIELD_REMOVED, RULE_FIELD_TYPE_CHANGED,
    RULE_PARAM_ADDED, RULE_PARAM_MADE_REQUIRED, RULE_PARAM_REMOVED, RULE_PARAM_REORDERED,
    RULE_PARAM_TYPE_NARROWED, RULE_PROPERTY_MADE_REQUIRED, RULE_RETURN_TYPE_WIDENED,
    RULE_TYPE_ALIAS_READONLY_CHANGED, RULE_TYPE_INCOMPATIBLE,
};
use crate::types::shape::{ParamShape, TypeShape};
use crate::types::{Finding, Severity};

const MAX_DEPTH: usize = 20;

/// SEM-03: Returns true when the shape is `any` (TypeShape::Primitive(PrimitiveKind::Any)).
///
/// Used to detect `any → T` narrowing before calling is_subtype_of, which returns true
/// for (any, X) in both directions and would suppress the narrowing finding.
fn is_any_shape(s: &TypeShape) -> bool {
    matches!(
        s,
        TypeShape::Primitive(crate::types::shape::PrimitiveKind::Any)
    )
}

/// Compare two TypeShapes structurally and produce findings for differences.
///
/// `symbol_name`: the export symbol name (for finding messages)
/// `file`: file path for the finding location
/// `line`, `col`: source position for the finding
///
/// Per D-10: if either shape is Opaque, emits type-incompatible (assume breaking).
/// Per D-11: recursion depth capped at 20 levels.
pub fn diff_types(
    old: &TypeShape,
    new: &TypeShape,
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    // D-10: Opaque on either side at the top level — assume breaking
    if matches!(old, TypeShape::Opaque(_)) || matches!(new, TypeShape::Opaque(_)) {
        if old != new {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_TYPE_INCOMPATIBLE,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "type of `{symbol_name}` changed incompatibly (unresolvable type)"
                ),
            });
        }
        return findings;
    }

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
    // D-11: stop at max depth with a warning-level finding
    if depth >= MAX_DEPTH {
        findings.push(Finding {
            severity: Severity::Patch,
            rule: "type-comparison-truncated",
            file: file.to_path_buf(),
            line,
            col,
            message: format!("type comparison truncated at depth {MAX_DEPTH} for `{symbol_name}`"),
        });
        return;
    }

    // D-10: Opaque on either side — assume breaking
    if matches!(old, TypeShape::Opaque(_)) || matches!(new, TypeShape::Opaque(_)) {
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_TYPE_INCOMPATIBLE,
            file: file.to_path_buf(),
            line,
            col,
            message: format!("type of `{symbol_name}` changed incompatibly (unresolvable type)"),
        });
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
        // Opaque after normalization (shouldn't happen due to early return above, but be safe)
        (TypeShape::Opaque(_), _) | (_, TypeShape::Opaque(_)) => {
            let display = make_display_name(symbol_name, field_path);
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_TYPE_INCOMPATIBLE,
                file: file.to_path_buf(),
                line,
                col,
                message: format!("type of `{display}` changed incompatibly (unresolvable type)"),
            });
        }

        // Both Object — compare field sets
        (TypeShape::Object(old_fields), TypeShape::Object(new_fields)) => {
            // Fields in old but not new → field-removed (Major)
            for (field_name, old_field) in old_fields {
                if !new_fields.contains_key(field_name) {
                    let path = make_path(field_path, field_name);
                    findings.push(Finding {
                        severity: Severity::Major,
                        rule: RULE_FIELD_REMOVED,
                        file: file.to_path_buf(),
                        line,
                        col,
                        message: format!(
                            "field `{path}` was removed from type `{symbol_name}` (was {})",
                            format_type(&old_field.shape)
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
                        rule: RULE_FIELD_ADDED,
                        file: file.to_path_buf(),
                        line,
                        col,
                        message: format!(
                            "{optional_label}field `{path}` was added to type `{symbol_name}` ({})",
                            format_type(&new_field.shape)
                        ),
                    });
                }
            }

            // Fields in both — compare shapes
            for (field_name, old_field) in old_fields {
                if let Some(new_field) = new_fields.get(field_name) {
                    // REF-06: optional property made required
                    if old_field.optional && !new_field.optional {
                        let path = make_path(field_path, field_name);
                        findings.push(Finding {
                            severity: Severity::Major,
                            rule: RULE_PROPERTY_MADE_REQUIRED,
                            file: file.to_path_buf(),
                            line,
                            col,
                            message: format!(
                                "property `{}` was made required in `{}`  (was optional)",
                                path, symbol_name
                            ),
                        });
                    }

                    // IFT-02: readonly modifier flip on type alias object field (parity with CLS-08)
                    if old_field.readonly != new_field.readonly {
                        let path = make_path(field_path, field_name);
                        let direction = if !old_field.readonly && new_field.readonly {
                            "added"
                        } else {
                            "removed"
                        };
                        findings.push(Finding {
                            severity: Severity::Major,
                            rule: RULE_TYPE_ALIAS_READONLY_CHANGED,
                            file: file.to_path_buf(),
                            line,
                            col,
                            message: format!(
                                "field `{}` readonly modifier {} on type `{}`",
                                path, direction, symbol_name
                            ),
                        });
                    }

                    let child_path = make_path(field_path, field_name);
                    if old_field.shape != new_field.shape {
                        // Use subtype check to decide: try recursion first for more specific findings.
                        // If recursion produces nothing, emit field-type-changed with old/new values.
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
                        if findings.len() == prev_len {
                            // Recursion added nothing — emit field-type-changed
                            findings.push(Finding {
                                severity: Severity::Major,
                                rule: RULE_FIELD_TYPE_CHANGED,
                                file: file.to_path_buf(),
                                line,
                                col,
                                message: format!(
                                    "field `{child_path}` changed type from `{}` to `{}` in `{symbol_name}`",
                                    format_type(&old_field.shape),
                                    format_type(&new_field.shape)
                                ),
                            });
                        }
                    }
                }
            }
        }

        // Both Array — use subtype check on inner type
        (TypeShape::Array(old_inner), TypeShape::Array(new_inner)) => {
            let display = make_display_name(symbol_name, field_path);
            if !is_subtype_of(new_inner, old_inner, depth + 1)
                && !is_subtype_of(old_inner, new_inner, depth + 1)
            {
                // Types are incompatible in both directions
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_TYPE_INCOMPATIBLE,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "type of `{display}` changed from `{}` to `{}`",
                        format_type(&old_n),
                        format_type(&new_n)
                    ),
                });
            } else {
                // Try recursion to get more specific findings
                let prev_len = findings.len();
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
                if findings.len() == prev_len && old_inner != new_inner {
                    // Inner types differ but recursion didn't capture it — emit incompatible
                    findings.push(Finding {
                        severity: Severity::Major,
                        rule: RULE_TYPE_INCOMPATIBLE,
                        file: file.to_path_buf(),
                        line,
                        col,
                        message: format!(
                            "type of `{display}` changed from `{}` to `{}`",
                            format_type(&old_n),
                            format_type(&new_n)
                        ),
                    });
                }
            }
        }

        // Both Tuple — compare element counts and diff each position
        (TypeShape::Tuple(old_elems), TypeShape::Tuple(new_elems)) => {
            if old_elems.len() != new_elems.len() {
                let display = make_display_name(symbol_name, field_path);
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_TYPE_INCOMPATIBLE,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "type of `{display}` changed from `{}` to `{}`",
                        format_type(&old_n),
                        format_type(&new_n)
                    ),
                });
            } else {
                for (i, (old_elem, new_elem)) in old_elems.iter().zip(new_elems.iter()).enumerate()
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

        // Both Union — use subtype checker for variance-aware comparison
        (TypeShape::Union(_), TypeShape::Union(_)) => {
            if !is_subtype_of(&old_n, &new_n, depth + 1) {
                // Old union is not a subtype of new union — some members were removed/narrowed
                let display = make_display_name(symbol_name, field_path);
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_TYPE_INCOMPATIBLE,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "type of `{display}` narrowed from `{}` to `{}`",
                        format_type(&old_n),
                        format_type(&new_n)
                    ),
                });
            }
            // If old is a subtype of new, the union was widened (safe) — no finding
        }

        // Both Intersection — adding a member narrows the type (breaking)
        (TypeShape::Intersection(_), TypeShape::Intersection(_)) => {
            if !is_subtype_of(&new_n, &old_n, depth + 1) {
                // New intersection requires more constraints than old — narrowing
                let display = make_display_name(symbol_name, field_path);
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_TYPE_INCOMPATIBLE,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "type of `{display}` changed incompatibly from `{}` to `{}`",
                        format_type(&old_n),
                        format_type(&new_n)
                    ),
                });
            }
        }

        // Both Function — variance-aware signature analysis (SIG-01..05)
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
            diff_functions(
                old_params,
                old_ret,
                new_params,
                new_ret,
                symbol_name,
                field_path,
                file,
                line,
                col,
                depth,
                findings,
            );
        }

        // Both Generic — compare names and args (invariant position)
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
                let display = make_display_name(symbol_name, field_path);
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_TYPE_INCOMPATIBLE,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "type of `{display}` changed from `{}` to `{}`",
                        format_type(&old_n),
                        format_type(&new_n)
                    ),
                });
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
            let display = make_display_name(symbol_name, field_path);
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_TYPE_INCOMPATIBLE,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "type of `{display}` changed from `{}` to `{}`",
                    format_type(&old_n),
                    format_type(&new_n)
                ),
            });
        }

        // Both Literal — if we got here after equality check, they differ
        (TypeShape::Literal(_), TypeShape::Literal(_)) => {
            let display = make_display_name(symbol_name, field_path);
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_TYPE_INCOMPATIBLE,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "type of `{display}` changed from `{}` to `{}`",
                    format_type(&old_n),
                    format_type(&new_n)
                ),
            });
        }

        // Different variant entirely — emit type-incompatible (Major)
        _ => {
            let display = make_display_name(symbol_name, field_path);
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_TYPE_INCOMPATIBLE,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "type of `{display}` changed incompatibly from `{}` to `{}`",
                    format_type(&old_n),
                    format_type(&new_n)
                ),
            });
        }
    }
}

/// Variance-aware function signature diff (SIG-01..05).
///
/// Called from diff_recursive when both old and new are Function variants.
#[allow(clippy::too_many_arguments)]
fn diff_functions(
    old_params: &[ParamShape],
    old_ret: &TypeShape,
    new_params: &[ParamShape],
    new_ret: &TypeShape,
    symbol_name: &str,
    _field_path: &str,
    file: &Path,
    line: u32,
    col: u32,
    depth: usize,
    findings: &mut Vec<Finding>,
) {
    // SIG-05: Param reorder detection (check FIRST).
    // Find params that exist in both by name. If 2+ common params and their
    // relative order differs, emit param-reordered and return.
    {
        let old_names: Vec<&str> = old_params.iter().map(|p| p.name.as_str()).collect();
        let new_names: Vec<&str> = new_params.iter().map(|p| p.name.as_str()).collect();

        // Common params: names present in both lists
        let common_in_old: Vec<&str> = old_names
            .iter()
            .filter(|&&n| new_names.contains(&n))
            .copied()
            .collect();
        let common_in_new: Vec<&str> = new_names
            .iter()
            .filter(|&&n| old_names.contains(&n))
            .copied()
            .collect();

        if common_in_old.len() >= 2 && common_in_old != common_in_new {
            let fn_name = symbol_name;
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_PARAM_REORDERED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "parameter order changed in `{fn_name}`: was ({}), now ({})",
                    old_names.join(", "),
                    new_names.join(", ")
                ),
            });
            return;
        }
    }

    // SIG-01/SIG-04: Param count changes.
    // New params beyond old.len() are truly added params.
    if new_params.len() > old_params.len() {
        for (i, new_param) in new_params.iter().enumerate().skip(old_params.len()) {
            if !new_param.optional {
                // SIG-01: required param added — Major
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_PARAM_ADDED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "required parameter `{}` added to `{symbol_name}` at position {i}",
                        new_param.name
                    ),
                });
            } else {
                // SIG-04: optional param added — Minor
                findings.push(Finding {
                    severity: Severity::Minor,
                    rule: RULE_PARAM_ADDED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "optional parameter `{}` added to `{symbol_name}`",
                        new_param.name
                    ),
                });
            }
        }
    }

    // SIG-01: Removed params (old has more than new).
    if old_params.len() > new_params.len() {
        for old_param in old_params.iter().skip(new_params.len()) {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_PARAM_REMOVED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "parameter `{}` removed from `{symbol_name}`",
                    old_param.name
                ),
            });
        }
    }

    // GAP-16-01: Optional parameter becoming required at same position.
    // ParamShape has `optional: bool` (shape.rs:60). When old_param.optional=true
    // and new_param.optional=false at the same position, this is a breaking change:
    // callers that omitted the argument will now get a type error.
    for (i, (old_param, new_param)) in old_params.iter().zip(new_params.iter()).enumerate() {
        if old_param.optional && !new_param.optional {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_PARAM_MADE_REQUIRED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "parameter `{}` changed from optional to required in `{}` (position {})",
                    new_param.name, symbol_name, i
                ),
            });
        }
    }

    // SIG-02: Param type narrowing (for params that exist at same position in both).
    // Params are contravariant: new param type must accept everything old param type did.
    // Check: is new_param a supertype of old_param? i.e., is_subtype_of(old_param, new_param)?
    // If NOT, the new param type is narrower — breaking.
    for (i, (old_param, new_param)) in old_params.iter().zip(new_params.iter()).enumerate() {
        if old_param.shape != new_param.shape {
            // SEM-03: Treat any → T (where T is not any) as tightening regardless of subtype
            // direction. Reason: is_subtype_of returns true for (any, X) in BOTH directions
            // (any is both top and bottom in TypeScript's escape-hatch semantics), which
            // incorrectly suppresses the narrowing finding when old param is `any`.
            let is_any_to_concrete = is_any_shape(&old_param.shape) && !is_any_shape(&new_param.shape);
            if is_any_to_concrete {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_PARAM_TYPE_NARROWED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "parameter `{}` narrowed from `any` to `{}` in `{symbol_name}` (position {i})",
                        new_param.name,
                        format_type(&new_param.shape)
                    ),
                });
                continue;
            }
            // Contravariant check: new param must accept what old param accepted.
            // new_param is "safe" if old_param <: new_param (old is subtype of new)
            // meaning new accepts at least everything old accepted.
            if !is_subtype_of(&old_param.shape, &new_param.shape, depth + 1) {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_PARAM_TYPE_NARROWED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "parameter `{}` type narrowed from `{}` to `{}` in `{symbol_name}` (position {i})",
                        new_param.name,
                        format_type(&old_param.shape),
                        format_type(&new_param.shape)
                    ),
                });
            }
            // If old_param <: new_param, the new param accepts a wider type — safe, no finding.
        }
    }

    // SIG-03: Return type widening.
    // Return types are covariant: the safe direction is new_ret <: old_ret (new is narrower).
    // Widening occurs when old_ret <: new_ret but new_ret !<: old_ret (new strictly wider).
    // Example: string -> string | number is widening (breaking for callers that assumed string).
    // Incompatible change (string -> number): neither subtypes the other → emit type-incompatible
    // instead of return-type-widened to be accurate.
    if old_ret != new_ret {
        let old_subtypes_new = is_subtype_of(old_ret, new_ret, depth + 1);
        let new_subtypes_old = is_subtype_of(new_ret, old_ret, depth + 1);
        if old_subtypes_new && !new_subtypes_old {
            // New return type is strictly wider — breaking (callers see more possible values)
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_RETURN_TYPE_WIDENED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "return type widened from `{}` to `{}` in `{symbol_name}`",
                    format_type(old_ret),
                    format_type(new_ret)
                ),
            });
        } else if !old_subtypes_new && !new_subtypes_old {
            // Completely incompatible return types — emit type-incompatible
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_TYPE_INCOMPATIBLE,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "return type of `{symbol_name}` changed incompatibly from `{}` to `{}`",
                    format_type(old_ret),
                    format_type(new_ret)
                ),
            });
        }
        // else: new_subtypes_old (new is narrower or equal) — safe, no finding
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

/// Build a display name for finding messages that includes the field path when present.
/// e.g. symbol_name="MyType", field_path="config.timeout" → "MyType.config.timeout"
/// e.g. symbol_name="MyType", field_path="" → "MyType"
fn make_display_name(symbol_name: &str, field_path: &str) -> String {
    if field_path.is_empty() {
        symbol_name.to_string()
    } else {
        format!("{symbol_name}.{field_path}")
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
        assert!(
            findings.is_empty(),
            "identical shapes should produce no findings"
        );
    }

    // Test 2: Different primitives produce type-incompatible (D-02: no more "type-changed")
    #[test]
    fn test_different_primitives_type_changed() {
        let findings = diff(
            TypeShape::Primitive(PrimitiveKind::String),
            TypeShape::Primitive(PrimitiveKind::Number),
        );
        assert_eq!(rule_ids(&findings), vec!["type-incompatible"]);
        assert_eq!(findings[0].severity, Severity::Major);
        // D-04: message includes old and new type values
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
                optional: false,
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            },
        );
        old_fields.insert(
            "y".to_string(),
            FieldShape {
                optional: false,
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            },
        );
        let mut new_fields = BTreeMap::new();
        new_fields.insert(
            "x".to_string(),
            FieldShape {
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
                optional: false,
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            },
        );
        let mut new_fields = BTreeMap::new();
        new_fields.insert(
            "x".to_string(),
            FieldShape {
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
                optional: false,
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            },
        );
        let mut new_fields = BTreeMap::new();
        new_fields.insert(
            "x".to_string(),
            FieldShape {
                optional: false,
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            },
        );
        new_fields.insert(
            "y".to_string(),
            FieldShape {
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
                optional: false,
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            },
        );
        let mut new_fields = BTreeMap::new();
        new_fields.insert(
            "x".to_string(),
            FieldShape {
                optional: false,
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            },
        );
        new_fields.insert(
            "y".to_string(),
            FieldShape {
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
                optional: false,
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            },
        );
        let mut inner_new = BTreeMap::new();
        inner_new.insert(
            "timeout".to_string(),
            FieldShape {
                optional: false,
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            },
        );

        let mut old_fields = BTreeMap::new();
        old_fields.insert(
            "config".to_string(),
            FieldShape {
                optional: false,
                readonly: false,
                shape: TypeShape::Object(inner_old),
            },
        );
        let mut new_fields = BTreeMap::new();
        new_fields.insert(
            "config".to_string(),
            FieldShape {
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
        // D-04: message should include old and new type values
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

    // Test 11: Opaque on either side — CRITICAL: D-10 behavior change
    // Phase 3: Opaque now produces type-incompatible Major (not empty findings)
    #[test]
    fn test_opaque_produces_type_incompatible() {
        // Opaque on old side
        let findings = diff(
            TypeShape::Opaque("x".to_string()),
            TypeShape::Primitive(PrimitiveKind::String),
        );
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "type-incompatible" && f.severity == Severity::Major),
            "Opaque old + Primitive new should produce type-incompatible Major"
        );

        // Opaque on new side
        let findings = diff(
            TypeShape::Primitive(PrimitiveKind::String),
            TypeShape::Opaque("x".to_string()),
        );
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "type-incompatible" && f.severity == Severity::Major),
            "Primitive old + Opaque new should produce type-incompatible Major"
        );

        // Both Opaque but different — still incompatible
        let findings = diff(
            TypeShape::Opaque("x".to_string()),
            TypeShape::Opaque("y".to_string()),
        );
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "type-incompatible" && f.severity == Severity::Major),
            "different Opaques should produce type-incompatible Major"
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
        // Build a deeply nested object: {a: {a: {a: ... (21 levels)}}}
        fn nested_object(depth: usize) -> TypeShape {
            if depth == 0 {
                TypeShape::Primitive(PrimitiveKind::String)
            } else {
                let mut fields = BTreeMap::new();
                fields.insert(
                    "a".to_string(),
                    FieldShape {
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
                .any(|f| f.rule == "type-comparison-truncated"),
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
                params: vec![ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Number)),
            },
            TypeShape::Function {
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
                params: vec![ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
            },
            TypeShape::Function {
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
                optional: false,
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            },
        );
        fields.insert(
            "y".to_string(),
            FieldShape {
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
        // D-04: message should include old and new type values
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
                params: vec![ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
            },
            TypeShape::Function {
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
                params: vec![],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::String)),
            },
            TypeShape::Function {
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
                params: vec![],
                return_type: Box::new(TypeShape::Union(vec![
                    TypeShape::Primitive(PrimitiveKind::String),
                    TypeShape::Primitive(PrimitiveKind::Number),
                ])),
            },
            TypeShape::Function {
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
                params: vec![ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
            },
            TypeShape::Function {
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

    // Test 24: Opaque produces type-incompatible Major (D-10 explicit test)
    #[test]
    fn test_opaque_either_side_no_findings() {
        // Note: test name kept for backward compat, but behavior is now opposite:
        // Phase 3 D-10: Opaque produces type-incompatible Major

        // Opaque on old side
        let findings = diff(
            TypeShape::Opaque("x".to_string()),
            TypeShape::Primitive(PrimitiveKind::String),
        );
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "type-incompatible" && f.severity == Severity::Major),
            "Opaque old + Primitive new should produce type-incompatible Major"
        );

        // Opaque on new side
        let findings = diff(
            TypeShape::Primitive(PrimitiveKind::String),
            TypeShape::Opaque("x".to_string()),
        );
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "type-incompatible" && f.severity == Severity::Major),
            "Primitive old + Opaque new should produce type-incompatible Major"
        );

        // Both Opaque but different values
        let findings = diff(
            TypeShape::Opaque("x".to_string()),
            TypeShape::Opaque("y".to_string()),
        );
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "type-incompatible" && f.severity == Severity::Major),
            "different Opaques should produce type-incompatible Major"
        );
    }

    // Test 25: Message includes old and new values (D-04)
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

    // GAP-16-01: Optional param made required — fires param-made-required Major
    #[test]
    fn test_param_made_required_fires_major() {
        use crate::compare::subtype::RULE_PARAM_MADE_REQUIRED;
        let findings = diff(
            TypeShape::Function {
                params: vec![ParamShape {
                    name: "x".to_string(),
                    optional: true, // was optional
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
            },
            TypeShape::Function {
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

    // GAP-16-01: required param stays required — no finding
    #[test]
    fn test_required_stays_required_no_finding() {
        use crate::compare::subtype::RULE_PARAM_MADE_REQUIRED;
        let findings = diff(
            TypeShape::Function {
                params: vec![ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
            },
            TypeShape::Function {
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

    // GAP-16-01: required param becomes optional — no finding (loosening is safe)
    #[test]
    fn test_required_to_optional_no_finding() {
        use crate::compare::subtype::RULE_PARAM_MADE_REQUIRED;
        let findings = diff(
            TypeShape::Function {
                params: vec![ParamShape {
                    name: "x".to_string(),
                    optional: false, // was required
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
            },
            TypeShape::Function {
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
        use crate::compare::subtype::RULE_PROPERTY_MADE_REQUIRED;
        let mut old_fields = BTreeMap::new();
        old_fields.insert(
            "name".to_string(),
            FieldShape {
                optional: true,
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            },
        );
        let mut new_fields = BTreeMap::new();
        new_fields.insert(
            "name".to_string(),
            FieldShape {
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
            optional,
            readonly,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        }
    }

    #[test]
    fn ift_02_readonly_added_on_type_alias_field_fires_major() {
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
    fn ift_02_readonly_removed_on_type_alias_field_fires_major() {
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
    fn ift_02_readonly_same_no_finding() {
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
    fn sem_03_any_to_string_fires_narrowing() {
        // Old: fn(x: any) -> void, New: fn(x: string) -> void
        // any → string is TIGHTENING — must fire param-type-narrowed Major
        let findings = diff(
            TypeShape::Function {
                params: vec![ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::Any),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
            },
            TypeShape::Function {
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
    fn sem_03_any_to_any_no_finding() {
        // Old: fn(x: any) -> void, New: fn(x: any) -> void — idempotent, no finding
        let findings = diff(
            TypeShape::Function {
                params: vec![ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::Any),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
            },
            TypeShape::Function {
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
}
