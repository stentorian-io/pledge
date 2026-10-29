//! Signature & type refinement rules (REF-01..05, REF-07, REF-08).
//!
//! Called by JavaScript structural comparison for each
//! exported symbol present in both old and new versions.

use crate::core::findings::{Finding, Severity};
use crate::languages::javascript::compare::rules::RULE_DEFAULT_EXPORT_CHANGED;
use crate::languages::javascript::model::{DeclarationKind, ExportKind, ExportSymbol};
use crate::languages::typescript::compare::rules::{
    RULE_ASSERTION_WIDENED, RULE_DECLARATION_KIND_CHANGED, RULE_INDEX_SIGNATURE_CHANGED,
    RULE_INDEX_SIGNATURE_KEY_PATTERN_CHANGED, RULE_NAMESPACE_TO_OTHER, RULE_OVERLOAD_REMOVED,
    RULE_REST_PARAM_NARROWED, RULE_THIS_PARAM_CHANGED, RULE_TYPE_GUARD_WIDENED,
};
use crate::languages::typescript::syntax::interfaces::IndexSignature;
use crate::languages::typescript::types::parser::{
    compare_template_literal_patterns, parse_template_literal_type, parse_type_string,
    PatternDirection,
};
use crate::languages::typescript::types::shape::TypeShape;
use crate::languages::typescript::types::subtype::{accepts_receiver, is_subtype_of};
use std::collections::HashSet;
use std::path::Path;

/// Compare two ExportSymbols for REF rules.
///
/// Checks: overload removal (REF-01), rest param narrowing (REF-02),
/// type guard widening (REF-03), declaration kind change (REF-04),
/// namespace conversion (REF-05), index signature change (REF-07),
/// default export kind change (REF-08).
///
/// REF-06 (optional->required property) is handled by `compare::types`.
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
                // A replacement must accept every old call and preserve its result.
                let compatible = old_overload == new_overload
                    || (is_subtype_of(new_overload, old_overload, 0)
                        && super::types::diff_types(
                            old_overload,
                            new_overload,
                            name,
                            file,
                            line,
                            col,
                        )
                        .iter()
                        .all(|finding| finding.severity != Severity::Major));
                if compatible {
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
    // old had type guard, new does not -> emit type-guard-widened
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
    //: decl_kinds is now Vec<_>. We compare the "primary" kind
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
    //  /: fire when old carries Namespace in decl_kinds AND new
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

    // REF-09: template-literal index-signature key pattern diff.
    // Runs BEFORE the REF-07 string-equality path so its matched-key sets
    // can suppress the REF-07 "removed + added" pair that would otherwise
    // fire for the same pattern transition.
    //
    // Dedup guard: for interface symbols, `compare_interface_index_signatures`
    // already fires REF-09 via the same shared helper. Running it here too
    // would double-fire. We still populate `ref09_matched_old` for interfaces
    // (so the REF-07 string-equality path below can still skip template-
    // literal keys that the interface-level pass will match), but discard
    // the findings to avoid duplication.
    let is_interface_sym = old_sym
        .decl_kinds
        .first()
        .map(|k| k == &DeclarationKind::Interface)
        .unwrap_or(false)
        || new_sym
            .decl_kinds
            .first()
            .map(|k| k == &DeclarationKind::Interface)
            .unwrap_or(false);
    let (ref09_matched_old, ref09_matched_new, ref09_findings) =
        diff_template_literal_key_pairs(old_index_sigs, new_index_sigs, name, file, line, col);
    if !is_interface_sym {
        findings.extend(ref09_findings);
    }

    // REF-07: index-signature-changed
    // Compare index signatures by key type. For each old sig, check new.
    for old_sig in old_index_sigs {
        // REF-09 suppression: if REF-09 already fired for this key, skip REF-07.
        if ref09_matched_old.contains(old_sig.key_type_text.as_str()) {
            continue;
        }
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
    // Silence unused-var warning when ref09_matched_new isn't consumed in this
    // function body (only ref09_matched_old is used by the REF-07 loop above).
    let _ = &ref09_matched_new;

    // REF-08: default-export-changed (kind change only)
    // Per /: two-tier analysis. Tier 1: kind changed -> always major.
    // Tier 2: same kind -> delegate to existing diff_types() (handled in compare_exports).
    //
    //: use primary (first) decl_kind from the Vec.
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

/// REF-09: detect narrowing/widening transitions between two
/// template-literal index signatures. Called from both `compare_ref_changes`
/// (type-alias level) and `compare_interface_index_signatures` (interface
/// level). Returns the set of old/new key-type texts that were matched
/// (so callers can suppress REF-07 for them) and the findings emitted.
///
/// Algorithm per key_type_text that starts with a backtick:
///  1. Find the best matching new sig whose key_type_text also starts with
///     a backtick (bijective pairing by first-come-first-served).
///  2. Parse both via `parse_template_literal_type`.
///  3. Compare via `compare_template_literal_patterns`.
///  4. Emit `index-signature-key-pattern-changed` — Major for Narrowing,
///     Minor for Widening, conservative Major for Unrelated, none for
///     Equal.
#[allow(clippy::too_many_arguments)]
pub(crate) fn diff_template_literal_key_pairs<'a>(
    old_sigs: &'a [IndexSignature],
    new_sigs: &'a [IndexSignature],
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> (HashSet<&'a str>, HashSet<&'a str>, Vec<Finding>) {
    let mut matched_old: HashSet<&str> = HashSet::new();
    let mut matched_new: HashSet<&str> = HashSet::new();
    let mut findings: Vec<Finding> = Vec::new();

    for old_sig in old_sigs {
        if !old_sig.key_type_text.trim_start().starts_with('`') {
            continue;
        }
        let old_parts = match parse_template_literal_type(&old_sig.key_type_text) {
            Some(p) => p,
            None => continue,
        };
        // Bijective pairing — first new sig whose key is a template-literal
        // AND is not yet matched. Allows multiple template-literal keys on
        // the same symbol to be paired up, though this is uncommon.
        let new_match = new_sigs.iter().find(|ns| {
            ns.key_type_text.trim_start().starts_with('`')
                && !matched_new.contains(ns.key_type_text.as_str())
                && parse_template_literal_type(&ns.key_type_text).is_some()
        });
        let new_sig = match new_match {
            Some(ns) => ns,
            None => continue,
        };
        let new_parts = match parse_template_literal_type(&new_sig.key_type_text) {
            Some(p) => p,
            None => continue,
        };
        matched_old.insert(old_sig.key_type_text.as_str());
        matched_new.insert(new_sig.key_type_text.as_str());
        let direction = compare_template_literal_patterns(&old_parts, &new_parts);
        match direction {
            PatternDirection::Equal => {} // no finding
            PatternDirection::Narrowing => {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_INDEX_SIGNATURE_KEY_PATTERN_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "template-literal index signature key pattern narrowed on `{}` (was `{}`, now `{}`)",
                        symbol_name, old_sig.key_type_text, new_sig.key_type_text
                    ),
                });
            }
            PatternDirection::Widening => {
                findings.push(Finding {
                    severity: Severity::Minor,
                    rule: RULE_INDEX_SIGNATURE_KEY_PATTERN_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "template-literal index signature key pattern widened on `{}` (was `{}`, now `{}`)",
                        symbol_name, old_sig.key_type_text, new_sig.key_type_text
                    ),
                });
            }
            PatternDirection::Unrelated => {
                // Conservative Major fallback.
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_INDEX_SIGNATURE_KEY_PATTERN_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "template-literal index signature key pattern changed on `{}` (was `{}`, now `{}`)",
                        symbol_name, old_sig.key_type_text, new_sig.key_type_text
                    ),
                });
            }
        }
    }
    (matched_old, matched_new, findings)
}

/// Count the number of required (non-optional) parameters in a TypeShape::Function.
fn count_required_arity(shape: &TypeShape) -> usize {
    match shape.without_type_parameters() {
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
        // A widened rest parameter accepts every previously valid argument.
        (Some(old_shape), Some(new_shape))
            if old_shape != new_shape && !is_subtype_of(old_shape, new_shape, 0) =>
        {
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
    match shape.without_type_parameters() {
        TypeShape::Function { params, .. } => params.iter().find(|p| p.rest).map(|p| &p.shape),
        _ => None,
    }
}

/// Compare `this` parameter types between old and new function versions (TLF-03).
///
/// A `this` parameter constrains the call receiver. Adding a requirement or
/// changing an unresolved requirement can break callers; removing it is safe.
pub fn compare_this_param(
    old_this: Option<&str>,
    new_this: Option<&str>,
    name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let old_shape = old_this.map(parse_type_string);
    let new_shape = new_this.map(parse_type_string);
    if accepts_receiver(new_shape.as_ref(), old_shape.as_ref(), 0) {
        return Vec::new();
    }
    vec![Finding {
        severity: Severity::Major,
        rule: RULE_THIS_PARAM_CHANGED,
        file: file.to_path_buf(),
        line,
        col,
        message: format!("receiver requirements narrowed in `{name}`"),
    }]
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
mod tests;
