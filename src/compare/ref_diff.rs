//! Signature & type refinement rules (REF-01..05, REF-07, REF-08).
//!
//! Called from compare_structural_diff() in compare/mod.rs for each
//! exported symbol present in both old and new versions.

use std::path::Path;

use crate::discover::tree_query::IndexSignature;
use crate::types::shape::TypeShape;
use crate::types::{DeclarationKind, ExportKind, ExportSymbol, Finding, Severity};

use super::subtype::{
    RULE_DECLARATION_KIND_CHANGED, RULE_DEFAULT_EXPORT_CHANGED, RULE_INDEX_SIGNATURE_CHANGED,
    RULE_NAMESPACE_TO_OTHER, RULE_OVERLOAD_REMOVED, RULE_REST_PARAM_NARROWED,
    RULE_TYPE_GUARD_WIDENED,
};

/// Compare two ExportSymbols for Phase 8 REF rules.
///
/// Checks: overload removal (REF-01), rest param narrowing (REF-02),
/// type guard widening (REF-03), declaration kind change (REF-04),
/// namespace conversion (REF-05), index signature change (REF-07),
/// default export kind change (REF-08).
///
/// REF-06 (optional->required property) is handled in type_diff.rs.
#[allow(clippy::too_many_arguments)]
pub fn compare_ref_changes(
    old_sym: &ExportSymbol,
    new_sym: &ExportSymbol,
    name: &str,
    file: &Path,
    line: u32,
    col: u32,
    old_index_sigs: &[IndexSignature],
    new_index_sigs: &[IndexSignature],
) -> Vec<Finding> {
    let mut findings = Vec::new();

    // REF-01: overload-removed
    // Per D-08: match overloads by parameter arity first.
    // For each old overload, check if any new overload has the same arity (count of required params).
    // If no match, emit overload-removed.
    if old_sym.overloads.len() > 1 {
        // Only check overloads when old had multiple signatures
        for old_overload in &old_sym.overloads {
            let old_arity = count_required_arity(old_overload);
            let has_match = new_sym
                .overloads
                .iter()
                .any(|new_ol| count_required_arity(new_ol) == old_arity);
            if !has_match {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_OVERLOAD_REMOVED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "overload signature with arity {} was removed from `{}`",
                        old_arity, name
                    ),
                });
            }
        }
    }

    // REF-02: rest-param-narrowed
    // Check if old had a rest param and new does not, or rest param type changed.
    check_rest_param_narrowed(
        &old_sym.overloads,
        &new_sym.overloads,
        name,
        file,
        line,
        col,
        &mut findings,
    );

    // REF-03: type-guard-widened
    // Per CONTEXT: old had type guard, new does not -> emit type-guard-widened
    if old_sym.type_guard_type.is_some() && new_sym.type_guard_type.is_none() {
        let guard_type = old_sym.type_guard_type.as_deref().unwrap_or("unknown");
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_TYPE_GUARD_WIDENED,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "type guard return `x is {}` was widened to `boolean` in `{}`",
                guard_type, name
            ),
        });
    }

    // REF-04: declaration-kind-changed (interface -> type alias)
    // Per D-05: compare old.decl_kind vs new.decl_kind
    if let (Some(DeclarationKind::Interface), Some(DeclarationKind::TypeAlias)) =
        (&old_sym.decl_kind, &new_sym.decl_kind)
    {
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_DECLARATION_KIND_CHANGED,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "interface `{}` converted to type alias -- breaks declaration merging",
                name
            ),
        });
    }

    // REF-05: namespace-to-other
    // Per D-05: old is Namespace, new is anything else
    if let Some(DeclarationKind::Namespace) = &old_sym.decl_kind {
        if let Some(new_kind) = &new_sym.decl_kind {
            if *new_kind != DeclarationKind::Namespace {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_NAMESPACE_TO_OTHER,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "namespace `{}` converted to {} -- breaks namespace augmentation",
                        name, new_kind
                    ),
                });
            }
        }
    }

    // REF-07: index-signature-changed
    // Compare index signatures by key type. For each old sig, check new.
    for old_sig in old_index_sigs {
        let matching_new = new_index_sigs
            .iter()
            .find(|ns| ns.key_type_text == old_sig.key_type_text);
        match matching_new {
            None => {
                // Index signature removed
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_INDEX_SIGNATURE_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "index signature `[key: {}]: {}` was removed from `{}`",
                        old_sig.key_type_text, old_sig.value_type_text, name
                    ),
                });
            }
            Some(new_sig) if new_sig.value_type_text != old_sig.value_type_text => {
                // Value type changed
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_INDEX_SIGNATURE_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "index signature value type changed from `{}` to `{}` in `{}`",
                        old_sig.value_type_text, new_sig.value_type_text, name
                    ),
                });
            }
            _ => {} // unchanged
        }
    }

    // REF-08: default-export-changed (kind change only)
    // Per D-09/D-10: two-tier analysis. Tier 1: kind changed -> always major.
    // Tier 2: same kind -> delegate to existing diff_types() (handled in compare_exports).
    if old_sym.kind == ExportKind::Default {
        if let (Some(old_kind), Some(new_kind)) = (&old_sym.decl_kind, &new_sym.decl_kind) {
            if old_kind != new_kind {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_DEFAULT_EXPORT_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "default export kind changed from `{}` to `{}`",
                        old_kind, new_kind
                    ),
                });
            }
        }
    }

    findings
}

/// Count the number of required (non-optional) parameters in a TypeShape::Function.
fn count_required_arity(shape: &TypeShape) -> usize {
    match shape {
        TypeShape::Function { params, .. } => params.iter().filter(|p| !p.optional && !p.rest).count(),
        _ => 0,
    }
}

/// Check for rest parameter removal or narrowing (REF-02).
///
/// Fires when:
/// 1. Old function had a rest param, new does not -> rest removed
/// 2. Old function had a rest param, new has a rest param with different type -> rest narrowed
fn check_rest_param_narrowed(
    old_overloads: &[TypeShape],
    new_overloads: &[TypeShape],
    name: &str,
    file: &Path,
    line: u32,
    col: u32,
    findings: &mut Vec<Finding>,
) {
    // Use first overload (primary signature) for rest param check
    let old_rest = old_overloads.first().and_then(|s| find_rest_param(s));
    let new_rest = new_overloads.first().and_then(|s| find_rest_param(s));

    match (old_rest, new_rest) {
        (Some(_), None) => {
            // Rest param removed
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_REST_PARAM_NARROWED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!("rest parameter was removed from `{}`", name),
            });
        }
        (Some(old_shape), Some(new_shape)) if old_shape != new_shape => {
            // Rest param type changed
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_REST_PARAM_NARROWED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!("rest parameter type was narrowed in `{}`", name),
            });
        }
        _ => {} // no rest in old, or same type — not breaking
    }
}

/// Find the rest parameter's type shape in a function signature.
fn find_rest_param(shape: &TypeShape) -> Option<&TypeShape> {
    match shape {
        TypeShape::Function { params, .. } => params.iter().find(|p| p.rest).map(|p| &p.shape),
        _ => None,
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::shape::{ParamShape, PrimitiveKind};
    use std::path::PathBuf;

    fn make_sym(name: &str, kind: ExportKind, decl_kind: Option<DeclarationKind>) -> ExportSymbol {
        ExportSymbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from("test.ts"),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kind,
            type_guard_type: None,
        }
    }

    #[test]
    fn test_ref04_interface_to_type_alias() {
        let old = make_sym("Config", ExportKind::Named, Some(DeclarationKind::Interface));
        let new = make_sym("Config", ExportKind::Named, Some(DeclarationKind::TypeAlias));
        let findings =
            compare_ref_changes(&old, &new, "Config", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
        assert!(findings.iter().any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED));
    }

    #[test]
    fn test_ref05_namespace_to_variable() {
        let old = make_sym("MyNS", ExportKind::Named, Some(DeclarationKind::Namespace));
        let new = make_sym("MyNS", ExportKind::Named, Some(DeclarationKind::Variable));
        let findings =
            compare_ref_changes(&old, &new, "MyNS", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
        assert!(findings.iter().any(|f| f.rule == RULE_NAMESPACE_TO_OTHER));
    }

    #[test]
    fn test_ref03_type_guard_widened() {
        let mut old = make_sym("isStr", ExportKind::Named, Some(DeclarationKind::Function));
        old.type_guard_type = Some("string".to_string());
        let new = make_sym("isStr", ExportKind::Named, Some(DeclarationKind::Function));
        let findings =
            compare_ref_changes(&old, &new, "isStr", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
        assert!(findings.iter().any(|f| f.rule == RULE_TYPE_GUARD_WIDENED));
    }

    #[test]
    fn test_ref07_index_sig_removed() {
        let old = make_sym("Dict", ExportKind::Named, Some(DeclarationKind::Interface));
        let new = make_sym("Dict", ExportKind::Named, Some(DeclarationKind::Interface));
        let old_sigs = vec![IndexSignature {
            key_type_text: "string".into(),
            value_type_text: "number".into(),
        }];
        let findings = compare_ref_changes(
            &old,
            &new,
            "Dict",
            &PathBuf::from("test.ts"),
            1,
            1,
            &old_sigs,
            &[],
        );
        assert!(findings.iter().any(|f| f.rule == RULE_INDEX_SIGNATURE_CHANGED));
    }

    #[test]
    fn test_ref07_index_sig_value_changed() {
        let old = make_sym("Dict", ExportKind::Named, Some(DeclarationKind::Interface));
        let new = make_sym("Dict", ExportKind::Named, Some(DeclarationKind::Interface));
        let old_sigs = vec![IndexSignature {
            key_type_text: "string".into(),
            value_type_text: "number".into(),
        }];
        let new_sigs = vec![IndexSignature {
            key_type_text: "string".into(),
            value_type_text: "string".into(),
        }];
        let findings = compare_ref_changes(
            &old,
            &new,
            "Dict",
            &PathBuf::from("test.ts"),
            1,
            1,
            &old_sigs,
            &new_sigs,
        );
        assert!(findings.iter().any(|f| f.rule == RULE_INDEX_SIGNATURE_CHANGED));
        let f = findings
            .iter()
            .find(|f| f.rule == RULE_INDEX_SIGNATURE_CHANGED)
            .unwrap();
        assert!(f.message.contains("number") && f.message.contains("string"));
    }

    #[test]
    fn test_ref08_default_export_kind_changed() {
        let old = make_sym("default", ExportKind::Default, Some(DeclarationKind::Function));
        let new = make_sym("default", ExportKind::Default, Some(DeclarationKind::Class));
        let findings = compare_ref_changes(
            &old,
            &new,
            "default",
            &PathBuf::from("test.ts"),
            1,
            1,
            &[],
            &[],
        );
        assert!(findings.iter().any(|f| f.rule == RULE_DEFAULT_EXPORT_CHANGED));
    }

    #[test]
    fn test_ref01_overload_removed() {
        let mut old = make_sym("foo", ExportKind::Named, Some(DeclarationKind::Function));
        old.overloads = vec![
            TypeShape::Function {
                params: vec![ParamShape {
                    name: "x".into(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Opaque("string".into()),
                }],
                return_type: Box::new(TypeShape::Opaque("string".into())),
            },
            TypeShape::Function {
                params: vec![
                    ParamShape {
                        name: "x".into(),
                        optional: false,
                        rest: false,
                        shape: TypeShape::Opaque("number".into()),
                    },
                    ParamShape {
                        name: "y".into(),
                        optional: false,
                        rest: false,
                        shape: TypeShape::Opaque("number".into()),
                    },
                ],
                return_type: Box::new(TypeShape::Opaque("number".into())),
            },
        ];
        let mut new = make_sym("foo", ExportKind::Named, Some(DeclarationKind::Function));
        new.overloads = vec![TypeShape::Function {
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Opaque("string".into()),
            }],
            return_type: Box::new(TypeShape::Opaque("string".into())),
        }];
        // Old had arity 1 and arity 2; new only has arity 1 -> arity 2 overload removed
        let findings =
            compare_ref_changes(&old, &new, "foo", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
        assert!(findings.iter().any(|f| f.rule == RULE_OVERLOAD_REMOVED));
    }

    #[test]
    fn test_ref02_rest_param_removed() {
        let mut old = make_sym("bar", ExportKind::Named, Some(DeclarationKind::Function));
        old.overloads = vec![TypeShape::Function {
            params: vec![ParamShape {
                name: "args".into(),
                optional: false,
                rest: true,
                shape: TypeShape::Opaque("string[]".into()),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        }];
        let mut new = make_sym("bar", ExportKind::Named, Some(DeclarationKind::Function));
        new.overloads = vec![TypeShape::Function {
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Opaque("string".into()),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        }];
        let findings =
            compare_ref_changes(&old, &new, "bar", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
        assert!(findings.iter().any(|f| f.rule == RULE_REST_PARAM_NARROWED));
    }
}
