// Generic type parameter diff engine.
//
// Implements GEN-01 through GEN-06: detects all 6 generic-related breaking changes.
// Applies to ALL exported constructs functions, interfaces, type aliases, classes.
//
// Design decisions:
// -: text-fallback constraint comparison is conservative (assume breaking if texts differ)
// - GEN-04: uses is_subtype_of() for shape-based constraint tightening analysis
// - GEN-03: only compares relative order of common params (ignores added/removed)

use crate::core::findings::{Finding, Severity};
use crate::languages::typescript::compare::rules::{
    RULE_CONST_TYPE_PARAM_CHANGED, RULE_GENERIC_CONSTRAINT_TIGHTENED, RULE_GENERIC_DEFAULT_CHANGED,
    RULE_GENERIC_INFERENCE_CONSTRAINT_REMOVED, RULE_GENERIC_PARAM_ADDED,
    RULE_GENERIC_PARAM_ADDED_WITH_DEFAULT, RULE_GENERIC_PARAM_REMOVED,
    RULE_GENERIC_PARAM_REORDERED, RULE_GENERIC_VARIANCE_CHANGED,
};
use crate::languages::typescript::model as ts_model;
use crate::languages::typescript::model::TypeParam;
use crate::languages::typescript::types::subtype::{format_type, is_subtype_of};
use crate::languages::typescript::types::{parser as type_parser, shape as type_shapes};
use std::path::Path;

pub(crate) fn compare_parameter_shapes(
    before: &[type_shapes::TypeParameterShape],
    after: &[type_shapes::TypeParameterShape],
    name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let parameters = |shapes: &[type_shapes::TypeParameterShape]| {
        shapes
            .iter()
            .enumerate()
            .map(|(index, parameter)| TypeParam {
                name: format!("T{index}"),
                constraint_text: parameter.constraint.as_ref().map(format_type),
                constraint_shape: parameter.constraint.clone(),
                default_text: parameter.default.as_ref().map(format_type),
                is_const: parameter.is_const,
                variance: parameter.variance.clone(),
            })
            .collect::<Vec<_>>()
    };
    let mut findings = compare_type_params(
        &parameters(before),
        &parameters(after),
        name,
        file,
        line,
        col,
    );
    for (index, (before, after)) in before.iter().zip(after).enumerate() {
        if let (Some(before), Some(after)) = (&before.default, &after.default) {
            if before != after && format_type(before) == format_type(after) {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_GENERIC_DEFAULT_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "default type for parameter T{index} changed structurally in `{name}`"
                    ),
                });
            }
        }
    }
    findings
}

/// Compare two TypeParam slices and produce findings for generic-related breaking changes.
///
/// Detects:
/// - GEN-01: required type parameter added (no default) → Major
/// - GEN-02: type parameter removed → Major
/// - GEN-03: type parameters reordered → Major
/// - GEN-04: generic constraint tightened → Major
/// - GEN-05: default type parameter changed → Major
/// - GEN-06: type parameter with default added → Minor
///
/// `symbol_name`: the export symbol name (for finding messages)
/// `file`: file path for the finding location
/// `line`, `col`: source position for the finding
pub fn compare_type_params(
    old_params: &[TypeParam],
    new_params: &[TypeParam],
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    // Early return: both slices empty
    if old_params.is_empty() && new_params.is_empty() {
        return Vec::new();
    }

    let mut findings: Vec<Finding> = Vec::new();

    // Build name-indexed lookups
    let old_by_name: std::collections::HashMap<&str, &TypeParam> =
        old_params.iter().map(|p| (p.name.as_str(), p)).collect();
    let new_by_name: std::collections::HashMap<&str, &TypeParam> =
        new_params.iter().map(|p| (p.name.as_str(), p)).collect();

    // GEN-02: param removed — old param not in new
    for old_param in old_params.iter().skip(new_params.len()) {
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_GENERIC_PARAM_REMOVED,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "type parameter `{}` was removed from `{}`",
                old_param.name, symbol_name
            ),
        });
    }

    // GEN-01 and GEN-06: param added — new param not in old
    for new_param in new_params.iter().skip(old_params.len()) {
        if let Some(ref default_text) = new_param.default_text {
            // GEN-06: added with default → Minor
            findings.push(Finding {
                severity: Severity::Minor,
                rule: RULE_GENERIC_PARAM_ADDED_WITH_DEFAULT,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "type parameter `{}` was added to `{}` with default `{}`",
                    new_param.name, symbol_name, default_text
                ),
            });
        } else {
            // GEN-01: added without default → Major (required)
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_GENERIC_PARAM_ADDED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "required type parameter `{}` was added to `{}` without default",
                    new_param.name, symbol_name
                ),
            });
        }
    }

    // GEN-03: reordered — compare relative order of common params
    // Build the ordered list of common param names in old order vs new order
    let old_common_order: Vec<&str> = old_params
        .iter()
        .filter(|p| new_by_name.contains_key(p.name.as_str()))
        .map(|p| p.name.as_str())
        .collect();
    let new_common_order: Vec<&str> = new_params
        .iter()
        .filter(|p| old_by_name.contains_key(p.name.as_str()))
        .map(|p| p.name.as_str())
        .collect();

    if old_common_order != new_common_order && old_common_order.len() > 1 {
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_GENERIC_PARAM_REORDERED,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "type parameters were reordered in `{}` (was [{}], now [{}])",
                symbol_name,
                old_common_order.join(", "),
                new_common_order.join(", "),
            ),
        });
    }

    // GEN-04 and GEN-05: per-param comparison for params present in both
    // Explicit type arguments are positional. A binder rename neither adds nor
    // removes an argument and must still compare constraints at the same slot.
    for (old_param, new_param) in old_params.iter().zip(new_params) {
        // GEN-04: constraint tightened
        check_constraint_tightened(
            old_param,
            new_param,
            symbol_name,
            file,
            line,
            col,
            &mut findings,
        );

        // GEN-05: default changed
        check_default_changed(
            old_param,
            new_param,
            symbol_name,
            file,
            line,
            col,
            &mut findings,
        );

        // GEN-07: NoInfer<T> wrapper removed from this constraint.
        // Runs AFTER check_constraint_tightened so the GEN-04 finding (if
        // any) for the same param is visible — when GEN-07 fires, we
        // suppress that GEN-04 finding because the wrapper-removal is a
        // distinct, lower-severity event (Minor inference change vs Major
        // constraint tightening). Without suppression both would fire on
        // the exact same syntactic transition (`NoInfer<X>` -> `X`).
        let gen07_fired = check_noinfer_wrapper_removed(
            old_param,
            new_param,
            symbol_name,
            file,
            line,
            col,
            &mut findings,
        );
        if gen07_fired {
            // Suppress any GEN-04 finding emitted by check_constraint_tightened
            // for this same param above — it would double-report the
            // wrapper-removal as a constraint tightening.
            let target_param_name = old_param.name.as_str();
            findings.retain(|f| {
                !(f.rule == RULE_GENERIC_CONSTRAINT_TIGHTENED
                    && f.message.contains(&format!("on `{}`", target_param_name))
                    && f.message.contains("NoInfer<"))
            });
        }

        // const type parameter modifier changed
        if old_param.is_const != new_param.is_const {
            let action = if old_param.is_const {
                "removed from"
            } else {
                "added to"
            };
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_CONST_TYPE_PARAM_CHANGED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "`const` modifier {} type parameter `{}` of `{}` — inference behavior changed",
                    action, old_param.name, symbol_name
                ),
            });
        }

        // TLF-05: variance modifier changed (in/out/inout)
        // Changing variance changes the assignability contract — Major.
        if old_param.variance != new_param.variance {
            let variance_str = |v: &Option<ts_model::Variance>| match v {
                Some(ts_model::Variance::In) => "in",
                Some(ts_model::Variance::Out) => "out",
                Some(ts_model::Variance::InOut) => "in out",
                None => "none",
            };
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_GENERIC_VARIANCE_CHANGED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "variance of type parameter `{}` changed from `{}` to `{}` in `{}`",
                    old_param.name,
                    variance_str(&old_param.variance),
                    variance_str(&new_param.variance),
                    symbol_name,
                ),
            });
        }
    }

    findings
}

/// GEN-04: Check if a generic constraint was tightened.
///
/// Uses is_subtype_of() for shape-based analysis when available.
/// Falls back to text comparison (assume breaking if texts differ).
fn check_constraint_tightened(
    old_param: &TypeParam,
    new_param: &TypeParam,
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
    findings: &mut Vec<Finding>,
) {
    match (&old_param.constraint_shape, &new_param.constraint_shape) {
        (Some(old_shape), Some(new_shape)) => {
            // Shape-based analysis: determine if the constraint was tightened.
            //
            // A constraint is tightened if:
            //  - old allowed more types than new (new is more restrictive)
            //  - i.e. old_shape is NOT a subtype of new_shape,
            // meaning old accepted types that new doesn't
            //
            // NOTE: TypeScript's `any` is both top and bottom type in the subtype checker,
            // so is_subtype_of(any, string) = true AND is_subtype_of(string, any) = true.
            // For constraint purposes, `any` means unconstrained. If old was `any` and new
            // is a specific type, the constraint was tightened regardless of subtype semantics.
            //
            // Check: are the shapes the same? If not, is new strictly more restrictive?
            // A constraint is NOT tightened only if: old_shape <: new_shape (old is subset of new,
            // meaning new accepts at least everything old accepts — new is same or broader).
            //
            // To handle `any` correctly: if old is `any` and new is not `any`, it's tightening.
            use crate::languages::typescript::types::shape::PrimitiveKind;
            let old_is_any = matches!(
                old_shape,
                type_shapes::TypeShape::Primitive(PrimitiveKind::Any)
            );
            let new_is_any = matches!(
                new_shape,
                type_shapes::TypeShape::Primitive(PrimitiveKind::Any)
            );

            let tightened = if old_is_any && !new_is_any {
                // was unconstrained (any), now constrained — tightened
                true
            } else if new_is_any {
                // new is any → loosened or same, never tightened
                false
            } else {
                // Both are non-any: tightened if old is NOT subtype of new
                // (i.e., old accepted types that new doesn't accept)
                !is_subtype_of(old_shape, new_shape, 0)
            };

            if tightened {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_GENERIC_CONSTRAINT_TIGHTENED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "generic constraint on `{}` was tightened in `{}` (was `{}`, now `{}`)",
                        old_param.name,
                        symbol_name,
                        format_type(old_shape),
                        format_type(new_shape),
                    ),
                });
            }
        }
        (None, Some(new_shape)) => {
            // No old constraint → any type allowed. Adding a constraint = tightening.
            let new_constraint_formatted = format_type(new_shape);
            let new_constraint_text = new_param
                .constraint_text
                .as_deref()
                .unwrap_or(&new_constraint_formatted);
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_GENERIC_CONSTRAINT_TIGHTENED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "generic constraint on `{}` was tightened in `{}` (was unconstrained, now `{}`)",
                    old_param.name, symbol_name, new_constraint_text,
                ),
            });
        }
        (Some(_), None) => {
            // Old had constraint, new has no constraint → loosened, not breaking. Skip.
        }
        (None, None) => {
            // Both have no shape — fall back to text comparison
            match (&old_param.constraint_text, &new_param.constraint_text) {
                (Some(old_text), Some(new_text)) => {
                    if old_text != new_text {
                        // conservative — texts differ, assume breaking
                        findings.push(Finding {
                            severity: Severity::Major,
                            rule: RULE_GENERIC_CONSTRAINT_TIGHTENED,
                            file: file.to_path_buf(),
                            line,
                            col,
                            message: format!(
                                "generic constraint on `{}` was tightened in `{}` (was `{}`, now `{}`)",
                                old_param.name, symbol_name, old_text, new_text,
                            ),
                        });
                    }
                    // Same text → no change
                }
                (None, Some(new_text)) => {
                    // Adding a constraint = tightening
                    findings.push(Finding {
                        severity: Severity::Major,
                        rule: RULE_GENERIC_CONSTRAINT_TIGHTENED,
                        file: file.to_path_buf(),
                        line,
                        col,
                        message: format!(
                            "generic constraint on `{}` was tightened in `{}` (was unconstrained, now `{}`)",
                            old_param.name, symbol_name, new_text,
                        ),
                    });
                }
                (Some(_), None) => {
                    // Removing constraint → loosened, not breaking. Skip.
                }
                (None, None) => {
                    // No constraints at all → no change
                }
            }
        }
    }
}

/// GEN-05: Check if a default type parameter value changed.
fn check_default_changed(
    old_param: &TypeParam,
    new_param: &TypeParam,
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
    findings: &mut Vec<Finding>,
) {
    match (&old_param.default_text, &new_param.default_text) {
        (Some(old_default), Some(new_default)) => {
            if old_default != new_default {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_GENERIC_DEFAULT_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "default type for parameter `{}` changed from `{}` to `{}` in `{}`",
                        old_param.name, old_default, new_default, symbol_name,
                    ),
                });
            }
        }
        (Some(old_default), None) => {
            // Old had a default, new does not → breaking (callers may rely on default)
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_GENERIC_DEFAULT_CHANGED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "default type for parameter `{}` was removed in `{}` (was `{}`)",
                    old_param.name, symbol_name, old_default,
                ),
            });
        }
        (None, Some(_)) => {
            // Adding a default to an existing param → compatible (not breaking)
            // Callers who previously passed the type arg still work; those who didn't now can omit it.
        }
        (None, None) => {
            // No defaults → no change
        }
    }
}

/// GEN-07: Detect `NoInfer<T>` wrapper removal on a
/// type-parameter constraint between old and new.
///
/// Two detection paths handle LSP-enriched and degraded modes:
///
/// 1. **Shape path (LSP-enriched):** when both sides have a populated
///    `constraint_shape`, look for `TypeShape::NoInferWrap(inner)` on the
///    old side and the structurally-equal unwrapped `inner` on the new
///    side. This is the canonical path that fires when LSP hover
///    enrichment has filled in the parsed constraint shape.
///
/// 2. **Text fallback (degraded mode):** when the parsed shapes aren't
///    populated, route the constraint *text* through `parse_type_string`
///    and apply the same NoInferWrap-vs-unwrapped pattern. This keeps
///    GEN-07 covering the type-param-constraint position even in
///    LSP-less runs where `constraint_shape` stays None.
///
/// Returns `true` when a Minor `generic-inference-constraint-removed`
/// finding was emitted; the caller uses this signal to suppress any
/// GEN-04 `generic-constraint-tightened` finding emitted for the same
/// param (the wrapper-removal would otherwise be double-reported as a
/// constraint tightening, since `NoInfer<X>` -> `X` looks like
/// "tightened" to the text-fallback comparator).
#[allow(clippy::too_many_arguments)]
fn check_noinfer_wrapper_removed(
    old_param: &TypeParam,
    new_param: &TypeParam,
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
    findings: &mut Vec<Finding>,
) -> bool {
    use crate::languages::typescript::types::shape::TypeShape;

    // 1. Shape path — preferred, fires when LSP enrichment filled the
    // parsed constraint shape on both sides.
    if let (Some(TypeShape::NoInferWrap(old_inner)), Some(new_shape)) = (
        old_param.constraint_shape.as_ref(),
        new_param.constraint_shape.as_ref(),
    ) {
        if !matches!(new_shape, TypeShape::NoInferWrap(_)) && **old_inner == *new_shape {
            findings.push(Finding {
                severity: Severity::Minor,
                rule: RULE_GENERIC_INFERENCE_CONSTRAINT_REMOVED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "`NoInfer<T>` wrapper removed from constraint on type parameter `{}` in `{}` — consumer inference behavior may change",
                    old_param.name, symbol_name
                ),
            });
            return true;
        }
    }

    // 2. Text fallback — degraded mode (no LSP). Parse both texts through
    //  `parse_type_string` and apply the same shape-level pattern. The
    // parser's GEN-07 special-case (parse_generic) produces
    //  `TypeShape::NoInferWrap` for `NoInfer<T>` literally, so the same
    // matching logic works on the parsed result.
    if let (Some(old_text), Some(new_text)) = (
        old_param.constraint_text.as_deref(),
        new_param.constraint_text.as_deref(),
    ) {
        let old_shape = type_parser::parse_type_string(old_text);
        let new_shape = type_parser::parse_type_string(new_text);
        if let TypeShape::NoInferWrap(old_inner) = &old_shape {
            if !matches!(new_shape, TypeShape::NoInferWrap(_)) && **old_inner == new_shape {
                findings.push(Finding {
                    severity: Severity::Minor,
                    rule: RULE_GENERIC_INFERENCE_CONSTRAINT_REMOVED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "`NoInfer<T>` wrapper removed from constraint on type parameter `{}` in `{}` — consumer inference behavior may change",
                        old_param.name, symbol_name
                    ),
                });
                return true;
            }
        }
    }

    false
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
