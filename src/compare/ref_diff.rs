//! Signature & type refinement rules (REF-01..05, REF-07, REF-08).
//!
//! Called from compare_structural_diff() in compare/mod.rs for each
//! exported symbol present in both old and new versions.

use std::path::Path;

use crate::discover::tree_query::IndexSignature;
use crate::types::shape::TypeShape;
use crate::types::{DeclarationKind, ExportKind, ExportSymbol, Finding, Severity};

use super::subtype::{
    is_subtype_of, RULE_DECLARATION_KIND_CHANGED, RULE_DEFAULT_EXPORT_CHANGED,
    RULE_INDEX_SIGNATURE_CHANGED, RULE_NAMESPACE_TO_OTHER, RULE_OVERLOAD_REMOVED,
    RULE_REST_PARAM_NARROWED, RULE_TYPE_GUARD_WIDENED,
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
    // Per D-08: match overloads by parameter arity first, then check parameter type compatibility.
    // For each old overload, check if any new overload has the same arity AND compatible param types.
    // If no match, emit overload-removed.
    if old_sym.overloads.len() > 1 {
        // Only check overloads when old had multiple signatures
        for old_overload in &old_sym.overloads {
            let old_arity = count_required_arity(old_overload);
            // First check: any new overload with same arity?
            let arity_matches: Vec<_> = new_sym
                .overloads
                .iter()
                .filter(|new_ol| count_required_arity(new_ol) == old_arity)
                .collect();

            if arity_matches.is_empty() {
                // No arity match at all -> overload removed
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
            } else {
                // Arity matches exist — check if any has compatible param types
                let has_type_match = arity_matches
                    .iter()
                    .any(|new_ol| overload_params_compatible(old_overload, new_ol));
                if !has_type_match {
                    findings.push(Finding {
                        severity: Severity::Major,
                        rule: RULE_OVERLOAD_REMOVED,
                        file: file.to_path_buf(),
                        line,
                        col,
                        message: format!(
                            "overload signature with arity {} had incompatible parameter type changes in `{}`",
                            old_arity, name
                        ),
                    });
                }
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

    // REF-04: declaration-kind-changed
    // Any declaration kind change is potentially breaking (not just interface->type alias).
    // Specific sub-cases:
    // - interface -> type alias: breaks declaration merging
    // - class -> function: breaks `new` and `extends`
    // - enum -> other: breaks member access pattern
    // - function -> variable: breaks call signatures if shape differs
    // REF-05 (namespace-to-other) is handled separately below.
    if let (Some(old_kind), Some(new_kind)) = (&old_sym.decl_kind, &new_sym.decl_kind) {
        if old_kind != new_kind
            && *old_kind != DeclarationKind::Unknown
            && *new_kind != DeclarationKind::Unknown
            && *old_kind != DeclarationKind::Namespace
        {
            let detail = if *old_kind == DeclarationKind::Interface
                && *new_kind == DeclarationKind::TypeAlias
            {
                " -- breaks declaration merging"
            } else {
                ""
            };
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_DECLARATION_KIND_CHANGED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "`{}` changed from {} to {}{}",
                    name, old_kind, new_kind, detail
                ),
            });
        }
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
        (Some(old_shape), Some(new_shape)) => {
            // Widening rest param type is safe (e.g., string[] -> any[]).
            // Only flag when new type is not a supertype of old type (i.e., it narrowed).
            if old_shape != new_shape && !is_subtype_of(old_shape, new_shape, 0) {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_REST_PARAM_NARROWED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!("rest parameter type was narrowed in `{}`", name),
                });
            }
        }
        _ => {} // no rest in old, or same type — not breaking
    }
}

/// Check if two overloads have compatible parameter types.
///
/// Returns true if each old required param type has a subtype relationship with the
/// corresponding new required param type (either direction — we accept any overlap as
/// "compatible" to avoid false-positive overload-removed findings).
fn overload_params_compatible(old: &TypeShape, new: &TypeShape) -> bool {
    match (old, new) {
        (
            TypeShape::Function {
                params: old_params, ..
            },
            TypeShape::Function {
                params: new_params, ..
            },
        ) => {
            // Compare non-rest required params pairwise
            let old_required: Vec<_> = old_params
                .iter()
                .filter(|p| !p.optional && !p.rest)
                .collect();
            let new_required: Vec<_> = new_params
                .iter()
                .filter(|p| !p.optional && !p.rest)
                .collect();
            if old_required.len() != new_required.len() {
                return false;
            }
            old_required
                .iter()
                .zip(new_required.iter())
                .all(|(o, n)| {
                    // Exact match or subtype relationship in either direction
                    o.shape == n.shape
                        || is_subtype_of(&o.shape, &n.shape, 0)
                        || is_subtype_of(&n.shape, &o.shape, 0)
                })
        }
        _ => false,
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
            is_async: false,
            is_generator: false,
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

    #[test]
    fn test_ref01_overload_same_arity_different_types() {
        // Old has two arity-1 overloads: fn(string) and fn(number).
        // New only has fn(string). The fn(number) overload should be detected as removed.
        let mut old = make_sym("foo", ExportKind::Named, Some(DeclarationKind::Function));
        old.overloads = vec![
            TypeShape::Function {
                params: vec![ParamShape {
                    name: "x".into(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::String),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
            },
            TypeShape::Function {
                params: vec![ParamShape {
                    name: "x".into(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Primitive(PrimitiveKind::Number),
                }],
                return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
            },
        ];
        let mut new = make_sym("foo", ExportKind::Named, Some(DeclarationKind::Function));
        new.overloads = vec![TypeShape::Function {
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        }];
        // fn(number) overload is gone — must fire overload-removed
        let findings =
            compare_ref_changes(&old, &new, "foo", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
        assert!(
            findings.iter().any(|f| f.rule == RULE_OVERLOAD_REMOVED),
            "Expected overload-removed for fn(number) removal, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_ref04_class_to_function() {
        let old = make_sym("Foo", ExportKind::Named, Some(DeclarationKind::Class));
        let new = make_sym("Foo", ExportKind::Named, Some(DeclarationKind::Function));
        let findings =
            compare_ref_changes(&old, &new, "Foo", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
        assert!(
            findings.iter().any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
            "Expected declaration-kind-changed for class->function, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_ref04_enum_to_interface() {
        let old = make_sym("MyEnum", ExportKind::Named, Some(DeclarationKind::Enum));
        let new = make_sym("MyEnum", ExportKind::Named, Some(DeclarationKind::Interface));
        let findings =
            compare_ref_changes(&old, &new, "MyEnum", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
        assert!(
            findings.iter().any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
            "Expected declaration-kind-changed for enum->interface, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_ref04_function_to_function_no_finding() {
        // Same kind -> no declaration-kind-changed finding
        let old = make_sym("fn", ExportKind::Named, Some(DeclarationKind::Function));
        let new = make_sym("fn", ExportKind::Named, Some(DeclarationKind::Function));
        let findings =
            compare_ref_changes(&old, &new, "fn", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
        assert!(
            !findings.iter().any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
            "Expected no declaration-kind-changed for function->function, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_ref02_rest_param_widened_no_finding() {
        // Old has rest param Opaque("string[]"), new has rest param Primitive(Any).
        // Widening to any[] is safe — should NOT fire rest-param-narrowed.
        let mut old = make_sym("bar", ExportKind::Named, Some(DeclarationKind::Function));
        old.overloads = vec![TypeShape::Function {
            params: vec![ParamShape {
                name: "args".into(),
                optional: false,
                rest: true,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        }];
        let mut new = make_sym("bar", ExportKind::Named, Some(DeclarationKind::Function));
        new.overloads = vec![TypeShape::Function {
            params: vec![ParamShape {
                name: "args".into(),
                optional: false,
                rest: true,
                shape: TypeShape::Primitive(PrimitiveKind::Any),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        }];
        let findings =
            compare_ref_changes(&old, &new, "bar", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
        assert!(
            !findings.iter().any(|f| f.rule == RULE_REST_PARAM_NARROWED),
            "Expected no rest-param-narrowed when widening to any, got: {:?}",
            findings
        );
    }
}
