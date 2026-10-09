//! Compare JS/TS export surfaces and consolidate related findings.

pub mod behavior;
pub mod classes;
pub mod namespace_moves;
pub mod renames;
pub mod rules;
pub mod structure;

use crate::core::findings::{Finding, Severity};
use crate::languages::javascript::model as js_model;
use crate::languages::javascript::model::{ExportMap, ExportSymbol};
use crate::languages::typescript::compare::rules::{
    RULE_AUGMENT_GLOBAL_CHANGED, RULE_AUGMENT_MODULE_CHANGED, RULE_EXPORT_DEPRECATED,
};
use crate::languages::typescript::compare::{directives as directive_diff, types as type_diff};
use crate::languages::typescript::model::ReleaseTag;
use crate::languages::typescript::syntax::comments;
use crate::languages::typescript::types::shape as type_shapes;
use std::collections::HashSet;
use std::path::Path;

/// SEM-08: Normalize rollup-plugin-dts / api-extractor-style internal
/// disambiguation suffixes (`T$1`, `T$2`, `T$10`) back to the base name `T`,
/// but ONLY when the base name `T` is in the provided exported-names set.
///
/// If `T$1` is mentioned in a type signature but the base `T` is not exported,
/// leave it as-is because it represents a genuinely distinct internal type
/// even when its name uses the same `$` convention.
///
/// Fast path: if the input contains no `$` character, returns a clone immediately.
fn normalize_rollup_suffix(input: &str, exported_names: &HashSet<String>) -> String {
    // Quick check: if no "$" in the input, skip all work.
    if !input.contains('$') {
        return input.to_string();
    }

    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        // Accumulate an identifier candidate (alphabetic or underscore start)
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();

            // Check for $<digits> suffix immediately following the identifier
            if i < chars.len() && chars[i] == '$' {
                let dollar_pos = i;
                i += 1; // consume '$'
                let digits_start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i > digits_start {
                    //: after `$<digits>`, check for an identifier-
                    // continuation char. If present, the `$N` sits MID-identifier
                    // (e.g., `price$1plus2`) and must NOT be split — we'd otherwise
                    // concatenate two parse tokens (`price` + `plus2` -> `priceplus2`)
                    // or, worse, emit `ident` verbatim when it's exported.
                    if i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                        // Treat the entire span as a single identifier. Emit the
                        // prefix + `$digits` + the continuation chars verbatim.
                        out.push_str(&ident);
                        out.extend(chars[dollar_pos..i].iter());
                        let rest_start = i;
                        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                            i += 1;
                        }
                        out.extend(chars[rest_start..i].iter());
                        continue;
                    }

                    // Pure suffix form `Ident$N` — safe to normalize.
                    if exported_names.contains(&ident) {
                        // Base is exported: emit just `ident` (strip $digits)
                        out.push_str(&ident);
                    } else {
                        // Base NOT exported: preserve full `ident$digits` form
                        out.push_str(&ident);
                        out.extend(chars[dollar_pos..i].iter());
                    }
                    continue;
                } else {
                    // `$` not followed by digits — emit identifier and `$` literally
                    out.push_str(&ident);
                    out.push('$');
                    continue;
                }
            } else {
                // No `$` suffix — emit identifier as-is
                out.push_str(&ident);
            }
        } else {
            out.push(c);
            i += 1;
        }
    }

    out
}

/// Compare two export maps and produce a list of findings.
///
/// - Exports present in `old` but absent in `new` → `export-removed` (Major)
/// - Exports present in `new` but absent in `old` → `export-added` (Minor)
///
/// big-bang: unified signature always takes `pkg_root`. IDN-01
/// rename detection scans new-side source files for alias trailers; IDN-02
/// namespace-move consolidation doesn't read files but accepts `pkg_root`
/// uniformly for symmetry. All callers pass the package root they're comparing
/// against (typically the "current"/new-side package root).
///
/// The public `compare_exports` wraps `compare_exports_core` (base diff without
/// sort/post-pass) with the IDN-01 + IDN-02 post-passes and a single
/// total-order sort at the tail (no double-sort).
///
/// Findings are sorted by (severity, file, line, col, rule) at the tail.
///
/// `lsp_was_available` (INF-04, ): when `true`, the diff pipeline
/// reads `ExportMap.typeof_targets` and emits Major
/// `type-reference-removed` findings for verified target removals. When
/// `false` (degraded mode), the same code path emits a Minor finding with
/// a `(limitation: lsp-required)` annotation. The flag is threaded
/// explicitly through the signature rather than read from a thread-local —
/// trivially testable, no hidden state.
pub fn compare_exports(
    old: &ExportMap,
    new: &ExportMap,
    pkg_root: &Path,
    lsp_was_available: bool,
) -> Vec<Finding> {
    // Base diff: ExportKey iteration, augment routing, type-aware compare.
    let mut findings = compare_exports_core(old, new, lsp_was_available);
    // IDN-01: transparent rename coalescing (alias-trailer-only, ).
    findings = renames::coalesce_renames(old, new, findings, pkg_root);
    // IDN-02: namespace-move cascade consolidation (exact-subtree, depth 1).
    findings = namespace_moves::consolidate_namespace_moves(old, new, findings);
    // Apply a single total-order sort at
    // the tail. Post-passes see unsorted findings; this is the only sort call.
    findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.col.cmp(&b.col))
            .then_with(|| a.rule.cmp(b.rule))
    });
    findings
}

/// Base export diff — ExportKey iteration, augment-prefix routing, type-aware
/// comparison. No sort, no post-passes. Called by `compare_exports` which
/// wraps this with IDN-01 + IDN-02 post-passes and a single total-order sort.
///
/// `lsp_was_available` (INF-04, ): threaded to
/// `diff_typeof_import_refs` to choose between Major-verified and
/// Minor-limitation-annotated emission.
fn compare_exports_core(old: &ExportMap, new: &ExportMap, lsp_was_available: bool) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();

    // META-04 / META-06: Filter non-public release tags before comparison.
    // Symbols tagged @internal, @alpha, or @beta are non-public.
    // Only @public and untagged symbols remain comparable.
    //
    //: no clones — see is_public predicate
    // below. Previous implementation cloned both ExportMaps and cleared the directive
    // sidecars on the copies; current code borrows directly.
    let is_public = |sym: &ExportSymbol| {
        !matches!(
            sym.release_tag,
            Some(ReleaseTag::Internal) | Some(ReleaseTag::Alpha) | Some(ReleaseTag::Beta)
        )
    };

    // Removed exports (DET-01, DET-02)
    for (key, sym) in old {
        if !is_public(sym) {
            continue;
        }
        let name = key.name.as_str();
        if !new.get(key).is_some_and(is_public) {
            // META-03: route augmentation member keys to specific rules
            if name.starts_with("augment:global:") {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_AUGMENT_GLOBAL_CHANGED,
                    file: sym.file.clone(),
                    line: sym.line,
                    col: sym.col,
                    message: format!("global augmentation member removed: {}", name),
                });
                continue;
            }
            if name.starts_with("augment:") && name.contains(":*") {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_AUGMENT_MODULE_CHANGED,
                    file: sym.file.clone(),
                    line: sym.line,
                    col: sym.col,
                    message: format!("module augmentation member removed: {}", name),
                });
                continue;
            }
            // qualify message with subpath/condition when non-canonical
            // to disambiguate multi-subpath / multi-condition exports.
            let is_root_no_cond = key.subpath == "." && key.condition.is_none();
            let qualifier = if is_root_no_cond {
                String::new()
            } else {
                format!(
                    " from {}{}",
                    key.subpath,
                    key.condition
                        .as_deref()
                        .map(|c| format!(" ({})", c))
                        .unwrap_or_default()
                )
            };
            let message = if name == "default" {
                if is_root_no_cond {
                    "default export was removed".to_string()
                } else {
                    format!("default export was removed{}", qualifier)
                }
            } else if key.space == js_model::SymbolSpace::Symbol && name.starts_with("@@") {
                let symbol_part = &name[2..];
                // ECF-13: well-known-Symbol removal branch. Present
                // the synthetic `@@<name>` internal key as the human-readable
                // `[Symbol.<name>]` form in the `export-removed` message.
                format!(
                    "symbol export `[Symbol.{}]` was removed{} - consumers relying on this key will break",
                    symbol_part, qualifier
                )
            } else {
                format!("exported symbol `{}` was removed{}", name, qualifier)
            };
            findings.push(Finding {
                severity: Severity::Major,
                rule: "export-removed",
                file: sym.file.clone(),
                line: sym.line,
                col: sym.col,
                message,
            });
        }
    }

    // New exports (DET-03, DET-04, OUT-05): compatible additions reported as Minor
    for (key, sym) in new {
        if !is_public(sym) {
            continue;
        }
        let name = key.name.as_str();
        if !old.get(key).is_some_and(is_public) {
            // META-03: augmentation member additions also use specific rules
            if name.starts_with("augment:global:") {
                findings.push(Finding {
                    severity: Severity::Minor,
                    rule: RULE_AUGMENT_GLOBAL_CHANGED,
                    file: sym.file.clone(),
                    line: sym.line,
                    col: sym.col,
                    message: format!("global augmentation member added: {}", name),
                });
                continue;
            }
            if name.starts_with("augment:") && name.contains(":*") {
                findings.push(Finding {
                    severity: Severity::Minor,
                    rule: RULE_AUGMENT_MODULE_CHANGED,
                    file: sym.file.clone(),
                    line: sym.line,
                    col: sym.col,
                    message: format!("module augmentation member added: {}", name),
                });
                continue;
            }
            // qualify message with subpath/condition when non-canonical
            let is_root_no_cond = key.subpath == "." && key.condition.is_none();
            let qualifier = if is_root_no_cond {
                String::new()
            } else {
                format!(
                    " at {}{}",
                    key.subpath,
                    key.condition
                        .as_deref()
                        .map(|c| format!(" ({})", c))
                        .unwrap_or_default()
                )
            };
            findings.push(Finding {
                severity: Severity::Minor,
                rule: "export-added",
                file: sym.file.clone(),
                line: sym.line,
                col: sym.col,
                message: format!("exported symbol `{}` was added{}", name, qualifier),
            });
        }
    }

    // META-04: Surface @deprecated symbols that exist in both old and new as Warning.
    for (key, sym) in new {
        if !is_public(sym) {
            continue;
        }
        let name = key.name.as_str();
        if matches!(sym.release_tag, Some(ReleaseTag::Deprecated))
            && old.get(key).is_some_and(is_public)
        {
            findings.push(Finding {
                severity: Severity::Warning,
                rule: RULE_EXPORT_DEPRECATED,
                file: sym.file.clone(),
                line: sym.line,
                col: sym.col,
                message: format!("export `{}` is marked @deprecated", name),
            });
        }
    }

    // Type-aware comparison: only when both symbols have TypeShape
    // Use first overload (primary signature) for type diff
    for (key, old_sym) in old {
        if !is_public(old_sym) {
            continue;
        }
        let name = key.name.as_str();
        if let Some(new_sym) = new.get(key).filter(|s| is_public(s)) {
            if let (Some(old_shape), Some(new_shape)) =
                (old_sym.overloads.first(), new_sym.overloads.first())
            {
                let compare_type = if old_sym
                    .decl_kinds
                    .contains(&js_model::DeclarationKind::Variable)
                    && !matches!(
                        old_shape.without_type_parameters(),
                        type_shapes::TypeShape::Function { .. }
                    ) {
                    type_diff::diff_values
                } else {
                    type_diff::diff_types
                };
                let type_findings = compare_type(
                    old_shape,
                    new_shape,
                    name,
                    &new_sym.file,
                    new_sym.line,
                    new_sym.col,
                );
                findings.extend(type_findings);
            }
            if !old_sym
                .decl_kinds
                .contains(&js_model::DeclarationKind::Function)
            {
                findings.extend(type_diff::diff_function_param_noinfer_removed(
                    &old_sym.overloads,
                    &new_sym.overloads,
                    name,
                    &new_sym.file,
                    new_sym.line,
                    new_sym.col,
                ));
            }
            // TYPE-01: `satisfies T` removal emits Minor
            // `type-narrowing-widened`. Fires regardless of TypeShape overload
            // presence — the satisfies_type field is the direct source of truth.
            findings.extend(type_diff::diff_satisfies_types(
                old_sym,
                new_sym,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
            // INF-04: `typeof import('./y').Z` cross-file type
            // reference removal. Reads the typeof_targets sidecar; the diff
            // function returns nothing when old_sym has no typeof refs.
            // The unfiltered `old`/`new` maps carry the directives + typeof_targets
            // sidecars used by the diff functions below.
            type_diff::diff_typeof_import_refs(
                old_sym,
                new_sym,
                old,
                new,
                key,
                key,
                lsp_was_available,
                &new_sym.file,
                &mut findings,
            );
        }
    }

    // INF-05: triple-slash directive diffs per source file.
    // Iterate the union of keys from old.directives and new.directives and
    // feed each per-file directive list pair through compare_directives.
    // Directives are a file-level concept, not per-symbol, so we read from
    // the unfiltered maps (the @internal/@alpha/@beta filter that preceded
    // this block strips SYMBOLS; directives are unrelated to release tags).
    {
        let all_files: std::collections::BTreeSet<&std::path::PathBuf> =
            old.directives.keys().chain(new.directives.keys()).collect();
        let empty_vec: Vec<comments::DirectiveRecord> = Vec::new();
        for file in all_files {
            let old_dirs = old.directives.get(file).unwrap_or(&empty_vec).as_slice();
            let new_dirs = new.directives.get(file).unwrap_or(&empty_vec).as_slice();
            findings.extend(directive_diff::compare_directives(old_dirs, new_dirs, file));
        }
    }

    // DOC-01: paired Info for Proxy/Reflect-dispatched defaults.
    // The Major export-removed findings (above) already report the typed
    // surface that disappeared; this paired Info documents WHY (the new
    // surface is dynamically dispatched and not enumerable statically).
    let mut dispatched = std::collections::HashSet::new();
    for (key, sym) in old.iter().chain(new.iter()) {
        if !is_public(sym) {
            continue;
        }
        if sym.is_proxy_dispatched && dispatched.insert(key) {
            findings.push(Finding {
                severity: Severity::Warning,
                rule: "analysis-incomplete",
                file: sym.file.clone(),
                line: sym.line,
                col: sym.col,
                message: format!(
                    "runtime dispatch of export `{}` requires handler or consumer evidence",
                    sym.name
                ),
            });
            findings.push(Finding {
                severity: Severity::Info,
                rule: rules::RULE_LIMITATION_PROXY_DISPATCHED_EXPORTS,
                file: sym.file.clone(),
                line: sym.line,
                col: sym.col,
                message: format!(
                    "export `{}` in `{}` uses runtime dispatch; the handler's observable surface is not established",
                    sym.name, sym.file.display()
                ),
            });
        }
    }

    // NO sort here — the public
    // `compare_exports` wrapper performs the single total-order sort after
    // IDN-01 + IDN-02 post-passes. Sorting here would mean double-sort.
    findings
}

/// Produce a synthetic list of findings for a first-publish package (PKG-03).
///
/// All current exports are treated as `export-added` minor findings.
pub fn first_publish_findings(exports: &ExportMap) -> Vec<Finding> {
    let mut findings: Vec<Finding> = exports
        .iter()
        .map(|(key, sym)| {
            // qualify with subpath/condition when non-canonical
            let is_root_no_cond = key.subpath == "." && key.condition.is_none();
            let qualifier = if is_root_no_cond {
                String::new()
            } else {
                format!(
                    " at {}{}",
                    key.subpath,
                    key.condition
                        .as_deref()
                        .map(|c| format!(" ({})", c))
                        .unwrap_or_default()
                )
            };
            Finding {
                severity: Severity::Minor,
                rule: "export-added",
                file: sym.file.clone(),
                line: sym.line,
                col: sym.col,
                message: format!("exported symbol `{}` was added{}", key.name, qualifier),
            }
        })
        .collect();

    // total-order sort on (severity, file, line, col, rule).
    findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.col.cmp(&b.col))
            .then_with(|| a.rule.cmp(b.rule))
    });

    findings
}

#[cfg(test)]
mod tests {
    use crate::languages::javascript::compare::*;
    use crate::languages::javascript::model as js_model;

    // ─── find_decl_in_node dotted name (namespace member) ─────────

    // ─── CJS inline structural diff ──────────────────────────────

    // ─── compare_exports ExportKey-iteration tests ─────────────────

    fn make_basic_sym(name: &str, file: std::path::PathBuf, line: u32) -> js_model::ExportSymbol {
        use crate::languages::javascript::model::{ExportKind, ExportSymbol};
        ExportSymbol {
            name: name.to_string(),
            kind: ExportKind::Named,
            file,
            line,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: Vec::new(),
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

    #[test]
    fn test_arch01_different_subpaths_produce_separate_findings() {
        // ARCH-01: same name at different subpaths must fan out to
        // separate export-removed + export-added findings (no silent match).
        use crate::languages::javascript::model::{ExportKey, ExportMap, SymbolSpace};
        use std::path::PathBuf;

        let mut old: ExportMap = ExportMap::new();
        let mut new: ExportMap = ExportMap::new();
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "foo"),
            make_basic_sym("foo", PathBuf::from("a.ts"), 1),
        );
        new.insert(
            ExportKey::new("./sub", None, SymbolSpace::Value, "foo"),
            make_basic_sym("foo", PathBuf::from("b.ts"), 1),
        );
        let findings = compare_exports(&old, &new, std::path::Path::new("."), true);
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.rule == "export-removed")
                .count(),
            1,
            "ARCH-01: exactly one export-removed for `foo` at `.`; got: {:?}",
            findings
        );
        assert_eq!(
            findings.iter().filter(|f| f.rule == "export-added").count(),
            1,
            "ARCH-01: exactly one export-added for `foo` at `./sub`; got: {:?}",
            findings
        );
        // Removed must reference the root `.` subpath in the message (canonical — no qualifier)
        let removed = findings
            .iter()
            .find(|f| f.rule == "export-removed")
            .unwrap();
        assert!(
            !removed.message.contains("./sub"),
            "ARCH-01: removed message for root subpath should not qualify; got: `{}`",
            removed.message
        );
        // Added must reference./sub in the qualifier
        let added = findings.iter().find(|f| f.rule == "export-added").unwrap();
        assert!(
            added.message.contains("./sub"),
            "ARCH-01: added message must qualify with `./sub`; got: `{}`",
            added.message
        );
    }

    #[test]
    fn test_arch03_type_and_value_space_diff_independently() {
        // ARCH-03: `export type Foo` and `export const Foo` coexist in
        // different SymbolSpace buckets and diff independently.
        use crate::languages::javascript::model::{ExportKey, ExportMap, SymbolSpace};
        use std::path::PathBuf;

        let mut old: ExportMap = ExportMap::new();
        let mut new: ExportMap = ExportMap::new();
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Type, "Foo"),
            make_basic_sym("Foo", PathBuf::from("a.ts"), 1),
        );
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "Foo"),
            make_basic_sym("Foo", PathBuf::from("a.ts"), 1),
        );
        // New: only value-space Foo
        new.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "Foo"),
            make_basic_sym("Foo", PathBuf::from("a.ts"), 1),
        );
        let findings = compare_exports(&old, &new, std::path::Path::new("."), true);
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.rule == "export-removed")
                .count(),
            1,
            "ARCH-03: exactly one export-removed for type-space Foo; got: {:?}",
            findings
        );
        assert_eq!(
            findings.iter().filter(|f| f.rule == "export-added").count(),
            0,
            "ARCH-03: value-space Foo present in both sides must NOT fire export-added; got: {:?}",
            findings
        );
    }

    #[test]
    fn test_arch04_multi_file_same_name_diffed_independently() {
        // ARCH-04: `User` from `./a.ts` at subpath `.` and `User` from
        // `./b.ts` at subpath `./admin` are distinct keys; removing one does not
        // collapse onto the other.
        use crate::languages::javascript::model::{ExportKey, ExportMap, SymbolSpace};
        use std::path::PathBuf;

        let mut old: ExportMap = ExportMap::new();
        let mut new: ExportMap = ExportMap::new();
        let user_root_key = ExportKey::new(".", None, SymbolSpace::Value, "User");
        let user_admin_key = ExportKey::new("./admin", None, SymbolSpace::Value, "User");
        old.insert(
            user_root_key.clone(),
            make_basic_sym("User", PathBuf::from("src/a.ts"), 1),
        );
        old.insert(
            user_admin_key.clone(),
            make_basic_sym("User", PathBuf::from("src/b.ts"), 1),
        );
        new.insert(
            user_root_key,
            make_basic_sym("User", PathBuf::from("src/a.ts"), 1),
        );
        // ./admin User removed.
        let findings = compare_exports(&old, &new, std::path::Path::new("."), true);
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.rule == "export-removed")
                .count(),
            1,
            "ARCH-04: exactly one export-removed for ./admin User; got: {:?}",
            findings
        );
        let removed = findings
            .iter()
            .find(|f| f.rule == "export-removed")
            .unwrap();
        assert_eq!(
            removed.file,
            PathBuf::from("src/b.ts"),
            "ARCH-04: removed finding must point to src/b.ts (admin subpath source); got: {:?}",
            removed
        );
    }

    #[test]
    fn test_warning8_compare_exports_deterministic_total_order() {
        // final sort must be total-order on
        // (severity, file, line, col, rule) so HashMap iteration non-determinism
        // never surfaces as Finding reorder.
        use crate::languages::javascript::model::{ExportKey, ExportMap, SymbolSpace};
        use std::path::PathBuf;

        let mut old: ExportMap = ExportMap::new();
        let new: ExportMap = ExportMap::new();
        // 4 distinct ExportKeys, all removed → 4 Major findings.
        for n in ["alpha", "beta", "gamma", "delta"] {
            old.insert(
                ExportKey::new(".", None, SymbolSpace::Value, n),
                make_basic_sym(n, PathBuf::from(format!("{}.ts", n)), 1),
            );
        }

        let run_a = compare_exports(&old, &new, std::path::Path::new("."), true);
        let run_b = compare_exports(&old, &new, std::path::Path::new("."), true);

        assert_eq!(
            run_a.len(),
            run_b.len(),
            "Warning 8: two runs must produce equal-length Finding vecs"
        );
        for (a, b) in run_a.iter().zip(run_b.iter()) {
            assert_eq!(a.rule, b.rule, "Warning 8: rule order must be stable");
            assert_eq!(a.severity, b.severity);
            assert_eq!(a.file, b.file);
            assert_eq!(a.line, b.line);
            assert_eq!(a.col, b.col);
            assert_eq!(a.message, b.message);
        }

        // Stronger: findings of the same severity must be sorted by
        // (file, line, col, rule) — this validates the total-order comparator.
        for window in run_a.windows(2) {
            if window[0].severity == window[1].severity {
                let a_tuple = (
                    &window[0].file,
                    window[0].line,
                    window[0].col,
                    window[0].rule,
                );
                let b_tuple = (
                    &window[1].file,
                    window[1].line,
                    window[1].col,
                    window[1].rule,
                );
                assert!(
                    a_tuple <= b_tuple,
                    "Warning 8: same-severity findings must be in total order; {:?} !<= {:?}",
                    a_tuple,
                    b_tuple
                );
            }
        }
    }
}

#[cfg(test)]
mod sem_08_tests {
    use crate::languages::javascript::compare::normalize_rollup_suffix;
    use std::collections::HashSet;

    fn set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn base_exported_suffix_normalized() {
        let exp = set(&["Foo"]);
        assert_eq!(normalize_rollup_suffix("Foo$1 | null", &exp), "Foo | null");
    }

    #[test]
    fn base_not_exported_suffix_preserved() {
        let exp = set(&["Baz"]);
        assert_eq!(
            normalize_rollup_suffix("Bar$1 | null", &exp),
            "Bar$1 | null"
        );
    }

    #[test]
    fn multi_digit_suffix() {
        let exp = set(&["Foo"]);
        assert_eq!(normalize_rollup_suffix("Foo$10", &exp), "Foo");
    }

    #[test]
    fn mixed_names() {
        let exp = set(&["Foo"]); // Foo exported, Bar not
        assert_eq!(
            normalize_rollup_suffix("Array<Foo$1> | Record<string, Bar$2>", &exp),
            "Array<Foo> | Record<string, Bar$2>"
        );
    }

    #[test]
    fn no_suffix_passthrough() {
        let exp = set(&["Foo"]);
        assert_eq!(normalize_rollup_suffix("Foo | Bar", &exp), "Foo | Bar");
    }

    #[test]
    fn no_dollar_fast_path() {
        let exp = set(&["Foo"]);
        assert_eq!(
            normalize_rollup_suffix("string | number", &exp),
            "string | number"
        );
    }

    #[test]
    fn dollar_without_digits_passthrough() {
        let exp = set(&["Foo"]);
        assert_eq!(normalize_rollup_suffix("Foo$ | Bar", &exp), "Foo$ | Bar");
    }

    //: `$N` mid-identifier must NOT split the token.
    // The stripper previously treated `Foo$1Bar` as `Foo` + consumed `$1` +
    // new identifier `Bar`, concatenating two parse tokens into `FooBar`.
    #[test]
    fn wr_01_dollar_n_mid_identifier_preserved() {
        let exp = set(&["Foo"]); // Foo is exported
                                 // `Foo$1Bar` is a single JS-valid identifier — must NOT become `FooBar`.
        assert_eq!(
            normalize_rollup_suffix("Foo$1Bar | null", &exp),
            "Foo$1Bar | null"
        );
    }

    #[test]
    fn wr_01_dollar_n_mid_identifier_preserved_unexported_base() {
        let exp = set(&["Qux"]); // `price` not exported
        assert_eq!(normalize_rollup_suffix("price$2plus", &exp), "price$2plus");
    }

    #[test]
    fn wr_01_dollar_n_mid_identifier_underscore_continuation() {
        let exp = set(&["Foo"]); // exported base
                                 // Underscore as continuation char after `$N` — also mid-identifier, do not split.
        assert_eq!(normalize_rollup_suffix("Foo$1_tail", &exp), "Foo$1_tail");
    }
}
