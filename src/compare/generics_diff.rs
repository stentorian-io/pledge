// Generic type parameter diff engine.
//
// Implements GEN-01 through GEN-06: detects all 6 generic-related breaking changes.
// Applies to ALL exported constructs per D-06: functions, interfaces, type aliases, classes.
//
// Design decisions:
// - D-10: text-fallback constraint comparison is conservative (assume breaking if texts differ)
// - GEN-04: uses is_subtype_of() for shape-based constraint tightening analysis
// - GEN-03: only compares relative order of common params (ignores added/removed)

use std::path::Path;

use crate::types::{Finding, Severity, TypeParam};
use super::subtype::{
    format_type, is_subtype_of,
    RULE_CONST_TYPE_PARAM_CHANGED,
    RULE_GENERIC_CONSTRAINT_TIGHTENED, RULE_GENERIC_DEFAULT_CHANGED,
    RULE_GENERIC_PARAM_ADDED, RULE_GENERIC_PARAM_ADDED_WITH_DEFAULT,
    RULE_GENERIC_PARAM_REMOVED, RULE_GENERIC_PARAM_REORDERED,
    RULE_GENERIC_VARIANCE_CHANGED,
};

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
    for old_param in old_params {
        if !new_by_name.contains_key(old_param.name.as_str()) {
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
    }

    // GEN-01 and GEN-06: param added — new param not in old
    for new_param in new_params {
        if !old_by_name.contains_key(new_param.name.as_str()) {
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
    for old_param in old_params {
        let new_param = match new_by_name.get(old_param.name.as_str()) {
            Some(p) => p,
            None => continue, // already handled as removed
        };

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

        // GAP-15-07: const type parameter modifier changed
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
            let variance_str = |v: &Option<crate::types::Variance>| match v {
                Some(crate::types::Variance::In) => "in",
                Some(crate::types::Variance::Out) => "out",
                Some(crate::types::Variance::InOut) => "in out",
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
/// Falls back to text comparison per D-10 (assume breaking if texts differ).
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
            //   - old allowed more types than new (new is more restrictive)
            //   - i.e. old_shape is NOT a subtype of new_shape,
            //     meaning old accepted types that new doesn't
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
            use crate::types::shape::PrimitiveKind;
            let old_is_any = matches!(old_shape, crate::types::shape::TypeShape::Primitive(PrimitiveKind::Any));
            let new_is_any = matches!(new_shape, crate::types::shape::TypeShape::Primitive(PrimitiveKind::Any));

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
                        // D-10: conservative — texts differ, assume breaking
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

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::shape::{PrimitiveKind, TypeShape};
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
        assert!(findings.is_empty(), "empty params should produce no findings");
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

        assert_eq!(findings.len(), 1, "should find 1 finding for added required param");
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
        assert!(f.message.contains("U"), "message should mention removed param");
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
        assert_eq!(tightened.len(), 1, "different constraint texts should produce finding");
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
        assert!(tightened.is_empty(), "same constraint texts should produce no finding");
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
        assert_eq!(default_findings.len(), 1, "changed default should produce finding");
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
        assert_eq!(added_with_default.len(), 1, "should detect added param with default");
        assert_eq!(added_with_default[0].severity, Severity::Minor);
        assert!(added_with_default[0].message.contains("U"));
        assert!(added_with_default[0].message.contains("string"));
        assert!(added_with_default[0].message.contains("MyFn"));
    }

    // ── GAP-15-07: const type parameter modifier ──────────────────────────────

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
            findings.iter().any(|f| f.rule == "const-type-param-changed"),
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
            findings.iter().any(|f| f.rule == "const-type-param-changed"),
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
            !findings.iter().any(|f| f.rule == "const-type-param-changed"),
            "should not fire for unchanged const; got: {:?}",
            findings
        );
    }

    // ── TLF-05: generic variance changed ─────────────────────────────────────

    #[test]
    fn tlf_05_variance_changed_fires_major() {
        use crate::types::Variance;
        let mut old_p = param("T");
        old_p.variance = Some(Variance::In);
        let mut new_p = param("T");
        new_p.variance = Some(Variance::Out);
        let findings = diff(&[old_p], &[new_p]);
        assert!(
            findings.iter().any(|f| f.rule == crate::compare::subtype::RULE_GENERIC_VARIANCE_CHANGED),
            "variance change should fire generic-variance-changed, got: {:?}",
            findings
        );
        let f = findings.iter().find(|f| f.rule == crate::compare::subtype::RULE_GENERIC_VARIANCE_CHANGED).unwrap();
        assert_eq!(f.severity, Severity::Major, "generic-variance-changed should be Major");
    }

    #[test]
    fn tlf_05_variance_unchanged_no_finding() {
        use crate::types::Variance;
        let mut old_p = param("T");
        old_p.variance = Some(Variance::In);
        let mut new_p = param("T");
        new_p.variance = Some(Variance::In);
        let findings = diff(&[old_p], &[new_p]);
        assert!(
            !findings.iter().any(|f| f.rule == crate::compare::subtype::RULE_GENERIC_VARIANCE_CHANGED),
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
}
