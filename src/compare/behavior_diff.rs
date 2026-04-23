// Behavior diff: detect async/sync and generator/non-generator transitions.
//
// BEH-01: async-to-sync (async function became sync)
//         sync-to-async (sync function became async)
// BEH-02: generator-removed (generator function became non-generator)
//
// Most findings use Severity::Warning (Tier 2), except generator-removed
// which is Severity::Major (Tier 1) per SEM-04 — removing a generator breaks
// consumers' for...of iteration deterministically at call time.

use std::path::Path;

use crate::types::{ExportSymbol, Finding, Severity};

use super::subtype::{RULE_ASYNC_TO_SYNC, RULE_GENERATOR_REMOVED, RULE_SYNC_TO_ASYNC};

/// Compare behavioral modifiers (async, generator) between old and new exports.
///
/// Returns Warning findings for:
/// - async -> sync transition (BEH-01, D-07: consumers awaiting get non-Promise)
/// - sync -> async transition (BEH-01, D-07: consumers not awaiting get Promise)
/// - generator -> non-generator transition (BEH-02: consumers iterating get non-iterable)
///
/// Does NOT flag generator addition (non-generator -> generator) as that is
/// a compatible change (return value gains iterator protocol).
pub fn compare_behavior(
    old_sym: &ExportSymbol,
    new_sym: &ExportSymbol,
    name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    // BEH-01: async -> sync
    if old_sym.is_async && !new_sym.is_async {
        findings.push(Finding {
            severity: Severity::Warning,
            rule: RULE_ASYNC_TO_SYNC,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "function `{}` changed from async to sync -- consumers awaiting this function will get a non-Promise value",
                name
            ),
        });
    }

    // BEH-01: sync -> async
    if !old_sym.is_async && new_sym.is_async {
        findings.push(Finding {
            severity: Severity::Warning,
            rule: RULE_SYNC_TO_ASYNC,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "function `{}` changed from sync to async -- consumers not awaiting this function will get a Promise instead of the value",
                name
            ),
        });
    }

    // BEH-02: generator -> non-generator
    // SEM-04: upgraded from Warning to Major — removing a generator deterministically
    // breaks consumers doing `for...of` iteration on the return value.
    if old_sym.is_generator && !new_sym.is_generator {
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_GENERATOR_REMOVED,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "function `{}` changed from generator to non-generator -- consumers iterating this function will break",
                name
            ),
        });
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExportKind, ExportSymbol};
    use std::path::PathBuf;

    fn make_sym(is_async: bool, is_generator: bool) -> ExportSymbol {
        ExportSymbol {
            name: "foo".to_string(),
            kind: ExportKind::Named,
            file: PathBuf::from("test.ts"),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: Vec::new(),
            type_guard_type: None,
            is_async,
            is_generator,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            compound_members: Vec::new(),
        }
    }

    fn run(old: &ExportSymbol, new: &ExportSymbol) -> Vec<Finding> {
        compare_behavior(old, new, "foo", Path::new("test.ts"), 1, 1)
    }

    #[test]
    fn test_async_to_sync() {
        let old = make_sym(true, false);
        let new = make_sym(false, false);
        let findings = run(&old, &new);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "async-to-sync");
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn test_sync_to_async() {
        let old = make_sym(false, false);
        let new = make_sym(true, false);
        let findings = run(&old, &new);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "sync-to-async");
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn test_generator_removed() {
        let old = make_sym(false, true);
        let new = make_sym(false, false);
        let findings = run(&old, &new);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "generator-removed");
        // SEM-04: generator-removed is Tier 1 (Major), not Warning
        assert_eq!(findings[0].severity, Severity::Major);
    }

    #[test]
    fn test_no_findings_when_identical() {
        let sym = make_sym(true, true);
        let findings = run(&sym, &sym);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_generator_added_no_finding() {
        // Adding generator is not breaking
        let old = make_sym(false, false);
        let new = make_sym(false, true);
        let findings = run(&old, &new);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_both_async_and_generator_changes() {
        let old = make_sym(true, true);
        let new = make_sym(false, false);
        let findings = run(&old, &new);
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().any(|f| f.rule == "async-to-sync"));
        assert!(findings.iter().any(|f| f.rule == "generator-removed"));
    }

    // ── SEM-04: generator-removed must be Major ───────────────────────────────

    #[test]
    fn sem_04_generator_removed_is_major_severity() {
        let old = make_sym(false, true);
        let new = make_sym(false, false);
        let findings = run(&old, &new);
        assert_eq!(findings.len(), 1, "should fire exactly one finding");
        assert_eq!(
            findings[0].rule, "generator-removed",
            "rule should be generator-removed"
        );
        assert_eq!(
            findings[0].severity,
            Severity::Major,
            "SEM-04: generator-removed must be Severity::Major, not Warning"
        );
    }
}
