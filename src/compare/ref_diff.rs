//! Signature & type refinement rules (REF-01..05, REF-07, REF-08).
//!
//! Called from compare_structural_diff() in compare/mod.rs for each
//! exported symbol present in both old and new versions.

use std::path::Path;

use crate::discover::tree_query::IndexSignature;
use crate::types::shape::TypeShape;
use crate::types::{DeclarationKind, ExportKind, ExportSymbol, Finding, Severity};

use super::subtype::{
    is_subtype_of, RULE_ASSERTION_WIDENED, RULE_DECLARATION_KIND_CHANGED,
    RULE_DEFAULT_EXPORT_CHANGED, RULE_INDEX_SIGNATURE_CHANGED, RULE_NAMESPACE_TO_OTHER,
    RULE_OVERLOAD_REMOVED, RULE_REST_PARAM_NARROWED, RULE_THIS_PARAM_CHANGED,
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

    // SEM-05: Track whether REF-08 (default-export-changed) fires for this symbol.
    // When REF-08 fires, suppress REF-04 (declaration-kind-changed) for the same symbol
    // to avoid double-reporting the same conceptual change at two different rule levels.
    let mut ref_08_fired = false;

    // REF-01: overload-removed
    // SEM-07: Shape-aware overload matching with consumed tracking.
    // Each new overload can match at most one old overload. Unmatched old overloads
    // fire overload-removed Major. This prevents a single new overload from
    // "absorbing" multiple old overloads of the same arity (the prior FN).
    if old_sym.overloads.len() > 1 {
        // Only check overloads when old had multiple signatures
        let mut consumed = vec![false; new_sym.overloads.len()];
        for old_overload in &old_sym.overloads {
            let old_arity = count_required_arity(old_overload);
            let mut matched = false;

            for (i, new_overload) in new_sym.overloads.iter().enumerate() {
                if consumed[i] {
                    continue; // already matched to another old overload
                }
                if count_required_arity(new_overload) != old_arity {
                    continue; // arity bucket mismatch
                }
                // Shape equality: exact match or bidirectional subtype (structural equivalence)
                let equal_shape = old_overload == new_overload
                    || (is_subtype_of(old_overload, new_overload, 0)
                        && is_subtype_of(new_overload, old_overload, 0))
                    || overload_params_compatible(old_overload, new_overload);
                if equal_shape {
                    consumed[i] = true;
                    matched = true;
                    break;
                }
            }

            if !matched {
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

    // REF-04: declaration-kind-changed
    // Any declaration kind change is potentially breaking (not just interface->type alias).
    // Specific sub-cases:
    // - interface -> type alias: breaks declaration merging
    // - class -> function: breaks `new` and `extends`
    // - enum -> other: breaks member access pattern
    // - function -> variable: breaks call signatures if shape differs
    // REF-05 (namespace-to-other) is handled separately below.
    //
    // Phase 18 D-14: decl_kinds is now Vec<_>. We compare the "primary" kind
    // (first element) for REF-04. The ref-05 check below uses set-membership
    // semantics so hybrids like [Class, Namespace] don't spuriously fire.
    if let (Some(old_kind), Some(new_kind)) =
        (old_sym.decl_kinds.first(), new_sym.decl_kinds.first())
    {
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
    // Phase 18 D-14 / D-16: fire when old carries Namespace in decl_kinds AND new
    // carries at least one decl_kind AND none of new's decl_kinds is Namespace.
    // This preserves REF-05 for pure namespace -> other transitions while
    // allowing hybrid class+namespace / function+namespace to suppress REF-05
    // when the namespace facet is preserved in the new symbol.
    if old_sym.decl_kinds.contains(&DeclarationKind::Namespace)
        && !new_sym.decl_kinds.is_empty()
        && !new_sym.decl_kinds.contains(&DeclarationKind::Namespace)
    {
        // Use the primary (first) new kind for the message.
        if let Some(new_kind) = new_sym.decl_kinds.first() {
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
    //
    // Phase 18 D-14: use primary (first) decl_kind from the Vec.
    if old_sym.kind == ExportKind::Default {
        if let (Some(old_kind), Some(new_kind)) =
            (old_sym.decl_kinds.first(), new_sym.decl_kinds.first())
        {
            if old_kind != new_kind {
                ref_08_fired = true; // SEM-05: record that REF-08 fired for this symbol
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

    // SEM-05: When REF-08 fired for this symbol, suppress REF-04 (declaration-kind-changed)
    // to avoid double-reporting the same conceptual change at two rule levels.
    if ref_08_fired {
        findings.retain(|f| f.rule != RULE_DECLARATION_KIND_CHANGED);
    }

    findings
}

/// Count the number of required (non-optional) parameters in a TypeShape::Function.
fn count_required_arity(shape: &TypeShape) -> usize {
    match shape {
        TypeShape::Function { params, .. } => {
            params.iter().filter(|p| !p.optional && !p.rest).count()
        }
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
            old_required.iter().zip(new_required.iter()).all(|(o, n)| {
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

/// Compare `this` parameter types between old and new function versions (TLF-03).
///
/// A `this` parameter constrains the call receiver. Changing or removing it is
/// breaking for callers who rely on the old receiver type contract.
pub fn compare_this_param(
    old_this: Option<&str>,
    new_this: Option<&str>,
    name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    match (old_this, new_this) {
        (Some(old_t), Some(new_t)) if old_t != new_t => {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_THIS_PARAM_CHANGED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "`this` parameter type changed from `{}` to `{}` in `{}`",
                    old_t, new_t, name
                ),
            });
        }
        (Some(old_t), None) => {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_THIS_PARAM_CHANGED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "`this` parameter (was `{}`) was removed from `{}`",
                    old_t, name
                ),
            });
        }
        _ => {} // unchanged, widened (None -> Some), or both None
    }

    findings
}

/// Compare assertion targets between old and new function versions (TLF-04).
///
/// An `asserts x is T` return narrows the caller's type. Removing or widening
/// the assertion target is a breaking change for consumers relying on the narrowing.
pub fn compare_assertion(
    old_target: Option<&str>,
    new_target: Option<&str>,
    name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    match (old_target, new_target) {
        (Some(_), None) => {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_ASSERTION_WIDENED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "assertion return type was removed from `{}` — type narrowing no longer guaranteed",
                    name
                ),
            });
        }
        (Some(old_t), Some(new_t)) if old_t != new_t => {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_ASSERTION_WIDENED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "assertion target changed from `{}` to `{}` in `{}`",
                    old_t, new_t, name
                ),
            });
        }
        _ => {} // unchanged or assertion added (not breaking)
    }

    findings
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
            decl_kinds: decl_kind.map(|k| vec![k]).unwrap_or_default(),
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            satisfies_type: None,
            compound_members: Vec::new(),
        }
    }

    /// Phase 18: helper for constructing symbols with multi-kind decl_kinds
    /// (declaration-merging hybrids like [Class, Namespace]).
    fn make_sym_multi(
        name: &str,
        kind: ExportKind,
        decl_kinds: Vec<DeclarationKind>,
    ) -> ExportSymbol {
        ExportSymbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from("test.ts"),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds,
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            satisfies_type: None,
            compound_members: Vec::new(),
        }
    }

    #[test]
    fn test_ref04_interface_to_type_alias() {
        let old = make_sym(
            "Config",
            ExportKind::Named,
            Some(DeclarationKind::Interface),
        );
        let new = make_sym(
            "Config",
            ExportKind::Named,
            Some(DeclarationKind::TypeAlias),
        );
        let findings = compare_ref_changes(
            &old,
            &new,
            "Config",
            &PathBuf::from("test.ts"),
            1,
            1,
            &[],
            &[],
        );
        assert!(findings
            .iter()
            .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED));
    }

    #[test]
    fn test_ref05_namespace_to_variable() {
        let old = make_sym("MyNS", ExportKind::Named, Some(DeclarationKind::Namespace));
        let new = make_sym("MyNS", ExportKind::Named, Some(DeclarationKind::Variable));
        let findings = compare_ref_changes(
            &old,
            &new,
            "MyNS",
            &PathBuf::from("test.ts"),
            1,
            1,
            &[],
            &[],
        );
        assert!(findings.iter().any(|f| f.rule == RULE_NAMESPACE_TO_OTHER));
    }

    // ── Phase 18 Warning 4: REF-05 hybrid + empty-decl_kinds guard ──────────

    #[test]
    fn test_ref05_hybrid_class_namespace_does_not_fire() {
        // Phase 18 D-14/D-16: class+namespace hybrid in new must NOT fire
        // RULE_NAMESPACE_TO_OTHER — the namespace facet is preserved.
        let old = make_sym_multi("X", ExportKind::Named, vec![DeclarationKind::Namespace]);
        let new = make_sym_multi(
            "X",
            ExportKind::Named,
            vec![DeclarationKind::Class, DeclarationKind::Namespace],
        );
        let findings = compare_ref_changes(&old, &new, "X", &PathBuf::from("x.ts"), 1, 1, &[], &[]);
        assert!(
            !findings.iter().any(|f| f.rule == RULE_NAMESPACE_TO_OTHER),
            "hybrid class+namespace must not fire RULE_NAMESPACE_TO_OTHER; got: {:?}",
            findings.iter().map(|f| f.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_ref05_pure_namespace_to_class_fires() {
        // Old: pure namespace. New: pure class (no namespace kind). Must fire REF-05.
        let old = make_sym_multi("X", ExportKind::Named, vec![DeclarationKind::Namespace]);
        let new = make_sym_multi("X", ExportKind::Named, vec![DeclarationKind::Class]);
        let findings = compare_ref_changes(&old, &new, "X", &PathBuf::from("x.ts"), 1, 1, &[], &[]);
        assert!(
            findings.iter().any(|f| f.rule == RULE_NAMESPACE_TO_OTHER),
            "pure namespace→class must fire REF-05; got: {:?}",
            findings.iter().map(|f| f.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_ref05_empty_new_decl_kinds_does_not_fire() {
        // Warning 4: empty new.decl_kinds (e.g., re-export with unknown kind) is
        // the pre-classification gap — pre-Phase-18 this was decl_kind: None and
        // REF-05 did NOT fire. Must preserve that semantics.
        let old = make_sym_multi("X", ExportKind::Named, vec![DeclarationKind::Namespace]);
        let new = make_sym_multi("X", ExportKind::Named, Vec::new());
        let findings = compare_ref_changes(&old, &new, "X", &PathBuf::from("x.ts"), 1, 1, &[], &[]);
        assert!(
            !findings.iter().any(|f| f.rule == RULE_NAMESPACE_TO_OTHER),
            "empty new.decl_kinds must NOT fire REF-05 (Warning 4 pre-classification-gap semantics); got: {:?}",
            findings.iter().map(|f| f.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_ref03_type_guard_widened() {
        let mut old = make_sym("isStr", ExportKind::Named, Some(DeclarationKind::Function));
        old.type_guard_type = Some("string".to_string());
        let new = make_sym("isStr", ExportKind::Named, Some(DeclarationKind::Function));
        let findings = compare_ref_changes(
            &old,
            &new,
            "isStr",
            &PathBuf::from("test.ts"),
            1,
            1,
            &[],
            &[],
        );
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
        assert!(findings
            .iter()
            .any(|f| f.rule == RULE_INDEX_SIGNATURE_CHANGED));
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
        assert!(findings
            .iter()
            .any(|f| f.rule == RULE_INDEX_SIGNATURE_CHANGED));
        let f = findings
            .iter()
            .find(|f| f.rule == RULE_INDEX_SIGNATURE_CHANGED)
            .unwrap();
        assert!(f.message.contains("number") && f.message.contains("string"));
    }

    #[test]
    fn test_ref08_default_export_kind_changed() {
        let old = make_sym(
            "default",
            ExportKind::Default,
            Some(DeclarationKind::Function),
        );
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
        assert!(findings
            .iter()
            .any(|f| f.rule == RULE_DEFAULT_EXPORT_CHANGED));
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
            findings
                .iter()
                .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
            "Expected declaration-kind-changed for class->function, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_ref04_enum_to_interface() {
        let old = make_sym("MyEnum", ExportKind::Named, Some(DeclarationKind::Enum));
        let new = make_sym(
            "MyEnum",
            ExportKind::Named,
            Some(DeclarationKind::Interface),
        );
        let findings = compare_ref_changes(
            &old,
            &new,
            "MyEnum",
            &PathBuf::from("test.ts"),
            1,
            1,
            &[],
            &[],
        );
        assert!(
            findings
                .iter()
                .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
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
            !findings
                .iter()
                .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
            "Expected no declaration-kind-changed for function->function, got: {:?}",
            findings
        );
    }

    // ── TLF-03: this-param type changed ──────────────────────────────────────

    #[test]
    fn tlf_03_this_param_changed_fires_major() {
        let findings = compare_this_param(
            Some("HTMLElement"),
            Some("Element"),
            "onClick",
            &PathBuf::from("test.ts"),
            1,
            1,
        );
        assert_eq!(
            findings.len(),
            1,
            "changed this-param should fire; got: {:?}",
            findings
        );
        assert_eq!(
            findings[0].rule,
            crate::compare::subtype::RULE_THIS_PARAM_CHANGED
        );
        assert_eq!(findings[0].severity, Severity::Major);
    }

    #[test]
    fn tlf_03_this_param_removed_fires_major() {
        let findings = compare_this_param(
            Some("HTMLElement"),
            None,
            "onClick",
            &PathBuf::from("test.ts"),
            1,
            1,
        );
        assert_eq!(
            findings.len(),
            1,
            "removed this-param should fire; got: {:?}",
            findings
        );
        assert_eq!(
            findings[0].rule,
            crate::compare::subtype::RULE_THIS_PARAM_CHANGED
        );
    }

    #[test]
    fn tlf_03_this_param_unchanged_no_finding() {
        let findings = compare_this_param(
            Some("HTMLElement"),
            Some("HTMLElement"),
            "onClick",
            &PathBuf::from("test.ts"),
            1,
            1,
        );
        assert!(
            findings.is_empty(),
            "identical this-param should produce no findings"
        );
    }

    // ── TLF-04: assertion target widened ─────────────────────────────────────

    #[test]
    fn tlf_04_assertion_removed_fires_major() {
        let findings = compare_assertion(
            Some("string"),
            None,
            "assertIsString",
            &PathBuf::from("test.ts"),
            1,
            1,
        );
        assert_eq!(
            findings.len(),
            1,
            "removed assertion should fire; got: {:?}",
            findings
        );
        assert_eq!(
            findings[0].rule,
            crate::compare::subtype::RULE_ASSERTION_WIDENED
        );
        assert_eq!(findings[0].severity, Severity::Major);
    }

    #[test]
    fn tlf_04_assertion_unchanged_no_finding() {
        let findings = compare_assertion(
            Some("string"),
            Some("string"),
            "assertIsString",
            &PathBuf::from("test.ts"),
            1,
            1,
        );
        assert!(
            findings.is_empty(),
            "identical assertion should produce no findings"
        );
    }

    #[test]
    fn tlf_04_no_assertion_no_finding() {
        let findings = compare_assertion(None, None, "fn", &PathBuf::from("test.ts"), 1, 1);
        assert!(
            findings.is_empty(),
            "no assertion on either side should produce no findings"
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

    // ── SEM-05: REF-04/REF-08 dedup ──────────────────────────────────────────

    #[test]
    fn sem_05_ref_04_suppressed_when_ref_08_fires() {
        // Both REF-04 (decl kind flips) AND REF-08 (default export kind change) would fire.
        // SEM-05: only REF-08 should be emitted for the same symbol.
        let mut old = make_sym("default", ExportKind::Default, Some(DeclarationKind::Class));
        old.overloads = vec![];
        let mut new = make_sym(
            "default",
            ExportKind::Default,
            Some(DeclarationKind::Function),
        );
        new.overloads = vec![];
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
        assert!(
            findings
                .iter()
                .any(|f| f.rule == RULE_DEFAULT_EXPORT_CHANGED),
            "SEM-05: REF-08 (default-export-changed) must fire; got: {:?}",
            findings
        );
        assert!(
            !findings.iter().any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
            "SEM-05: REF-04 (declaration-kind-changed) must be suppressed when REF-08 fires for same symbol; got: {:?}",
            findings
        );
    }

    #[test]
    fn sem_05_ref_04_fires_alone_when_ref_08_does_not() {
        // Only decl kind flip, NOT a default export — REF-04 alone fires normally.
        let old = make_sym(
            "Config",
            ExportKind::Named,
            Some(DeclarationKind::Interface),
        );
        let new = make_sym(
            "Config",
            ExportKind::Named,
            Some(DeclarationKind::TypeAlias),
        );
        let findings = compare_ref_changes(
            &old,
            &new,
            "Config",
            &PathBuf::from("test.ts"),
            1,
            1,
            &[],
            &[],
        );
        assert!(
            findings
                .iter()
                .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
            "SEM-05: REF-04 must fire when REF-08 does not; got: {:?}",
            findings
        );
    }

    // ── SEM-07: Shape-aware overload matching with consumed tracking ──────────

    #[test]
    fn sem_07_unmatched_overload_fires_removed() {
        // Old: [fn(string), fn(number)]; New: [fn(string)]
        // fn(number) overload has no match in new → overload-removed
        let mut old = make_sym("fn", ExportKind::Named, Some(DeclarationKind::Function));
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
        let mut new = make_sym("fn", ExportKind::Named, Some(DeclarationKind::Function));
        new.overloads = vec![TypeShape::Function {
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        }];
        let findings =
            compare_ref_changes(&old, &new, "fn", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
        assert!(
            findings.iter().any(|f| f.rule == RULE_OVERLOAD_REMOVED),
            "SEM-07: fn(number) overload removal must fire overload-removed; got: {:?}",
            findings
        );
        // Should fire exactly once (one removed overload)
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.rule == RULE_OVERLOAD_REMOVED)
                .count(),
            1,
            "SEM-07: exactly one overload-removed expected; got: {:?}",
            findings
        );
    }

    #[test]
    fn sem_07_matched_shapes_no_removal() {
        // Both sides: [fn(string)] — same shape, no removal
        let mut old = make_sym("fn", ExportKind::Named, Some(DeclarationKind::Function));
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
        let mut new = make_sym("fn", ExportKind::Named, Some(DeclarationKind::Function));
        new.overloads = vec![
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
        let findings =
            compare_ref_changes(&old, &new, "fn", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
        assert!(
            !findings.iter().any(|f| f.rule == RULE_OVERLOAD_REMOVED),
            "SEM-07: matching overloads on both sides should not fire overload-removed; got: {:?}",
            findings
        );
    }
}
