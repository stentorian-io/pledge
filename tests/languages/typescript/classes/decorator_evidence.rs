//! Decorator evidence compatibility checks.

use pledge::core::findings::Severity;
use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare::compare_exports;
use pledge::languages::javascript::compare::structure::compare_structural_diff;
use pledge::languages::typescript::compare::rules::{
    RULE_DECORATOR_ADDED, RULE_DECORATOR_ARGUMENTS_CHANGED, RULE_DECORATOR_REMOVED,
    RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT,
};
use std::path::{Path, PathBuf};

/// Helper: run the full pipeline (export diff + structural diff) on a fixture case.
/// Used for ClassShape-dependent tests (decorators, CLS-11, CLS-12).
fn run_full_pipeline(case: &str) -> Vec<pledge::core::findings::Finding> {
    let old_dir = fixture_dir(case, "old");
    let new_dir = fixture_dir(case, "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    // Class-shape findings (decorators, CLS-11, CLS-12) originate in compare_structural_diff.
    // compare_exports alone only does ExportKey-iteration diff.
    let mut findings = compare_exports(&old_exports, &new_exports, &new_dir, false);
    findings.extend(compare_structural_diff(
        &old_dir,
        &new_dir,
        &old_exports,
        &new_exports,
    ));
    findings
}

const FIXTURE_ROOT: &str = "tests/fixtures";

fn fixture_dir(case: &str, side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_ROOT)
        .join(case)
        .join(side)
}

#[test]
fn decorator_paired_info_emitted_for_added() {
    let findings = run_full_pipeline("typescript/classes/evidence/decorator-added");

    let added: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_DECORATOR_ADDED && f.severity == Severity::Major)
        .collect();
    let info: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT && f.severity == Severity::Info
        })
        .collect();
    assert!(
        !added.is_empty(),
        "expected decorator-added Major; got rules: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
    assert_eq!(
        added.len(),
        info.len(),
        "every decorator-added must have a paired Info; added={}, info={}",
        added.len(),
        info.len()
    );
    // Pairing: same (file, line, col)
    for a in &added {
        let pair = info
            .iter()
            .find(|i| i.file == a.file && i.line == a.line && i.col == a.col);
        assert!(
            pair.is_some(),
            "decorator-added at {}:{}:{} has no paired Info",
            a.file.display(),
            a.line,
            a.col
        );
    }
}

#[test]
fn decorator_paired_info_emitted_for_removed() {
    let findings = run_full_pipeline("typescript/classes/evidence/decorator-removed");

    let removed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_DECORATOR_REMOVED && f.severity == Severity::Major)
        .collect();
    let info: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT && f.severity == Severity::Info
        })
        .collect();
    assert!(
        !removed.is_empty(),
        "expected decorator-removed Major; got rules: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
    assert_eq!(
        removed.len(),
        info.len(),
        "every decorator-removed must have a paired Info; removed={}, info={}",
        removed.len(),
        info.len()
    );
}

#[test]
fn decorator_paired_info_emitted_for_arguments_changed() {
    let findings = run_full_pipeline("typescript/classes/evidence/decorator-arguments");

    let changed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_DECORATOR_ARGUMENTS_CHANGED && f.severity == Severity::Major)
        .collect();
    let info: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT && f.severity == Severity::Info
        })
        .collect();
    assert!(
        !changed.is_empty(),
        "expected decorator-arguments-changed Major; got rules: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
    assert_eq!(
        changed.len(),
        info.len(),
        "every decorator-arguments-changed must have a paired Info; changed={}, info={}",
        changed.len(),
        info.len()
    );
}
