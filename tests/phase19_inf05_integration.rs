//! Phase 19 INF-05 integration tests: triple-slash directive diffs.
//!
//! Five fixture forms exercise the full pipeline (source -> CommentIndex ->
//! extract_directives -> ExportMap.directives sidecar -> compare_directives
//! -> Finding):
//!
//! 1. `directive-added` — Major `directive-added` when a `/// <X />` appears
//!    only in the new version.
//! 2. `directive-removed` — Major `directive-removed` when a `/// <X />` is
//!    dropped in the new version.
//! 3. `directive-changed` — Major `directive-changed` when an attribute value
//!    differs (NOT 1 added + 1 removed).
//! 4. `directive-reordered` — Minor `directive-reordered` when the same set
//!    of directives appears in a different document order (NOT 2
//!    `directive-changed`).
//! 5. `directive-position-ignored` — a `/// <reference />` after a non-comment
//!    statement is a plain comment per TS compiler semantics; zero directive
//!    findings fire for either side.

use std::path::{Path, PathBuf};

use interlint::compare::compare_exports;
use interlint::compare::subtype::{
    RULE_DIRECTIVE_ADDED, RULE_DIRECTIVE_CHANGED, RULE_DIRECTIVE_REMOVED,
    RULE_DIRECTIVE_REORDERED,
};
use interlint::discover::discover_exports;
use interlint::types::Severity;

const FIXTURE_ROOT: &str = "tests/fixtures/phase-19/inf-05-directives";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn run(form: &str) -> Vec<interlint::types::Finding> {
    let old_dir = fixture_dir(form, "old");
    let new_dir = fixture_dir(form, "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    compare_exports(&old_exports, &new_exports, &new_dir)
}

#[test]
fn inf_05_directive_added() {
    let findings = run("directive-added");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_DIRECTIVE_ADDED)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 directive-added; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        hits[0].severity,
        Severity::Major,
        "directive-added must be Major"
    );
}

#[test]
fn inf_05_directive_removed() {
    let findings = run("directive-removed");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_DIRECTIVE_REMOVED)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 directive-removed; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        hits[0].severity,
        Severity::Major,
        "directive-removed must be Major"
    );
}

#[test]
fn inf_05_directive_changed() {
    let findings = run("directive-changed");
    let changed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_DIRECTIVE_CHANGED)
        .collect();
    let added: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_DIRECTIVE_ADDED)
        .collect();
    let removed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_DIRECTIVE_REMOVED)
        .collect();
    assert_eq!(
        changed.len(),
        1,
        "expected exactly 1 directive-changed; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert!(
        added.is_empty() && removed.is_empty(),
        "attribute change must be 1 `changed`, not 1 added + 1 removed"
    );
    assert_eq!(changed[0].severity, Severity::Major);
}

#[test]
fn inf_05_directive_reordered() {
    let findings = run("directive-reordered");
    let reordered: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_DIRECTIVE_REORDERED)
        .collect();
    let changed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_DIRECTIVE_CHANGED)
        .collect();
    assert_eq!(
        reordered.len(),
        1,
        "expected exactly 1 directive-reordered; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert!(
        changed.is_empty(),
        "pure reorder must not fire directive-changed"
    );
    assert_eq!(
        reordered[0].severity,
        Severity::Minor,
        "directive-reordered must be Minor per D-04"
    );
}

#[test]
fn inf_05_directive_position_ignored() {
    let findings = run("directive-position-ignored");
    // The /// comment lives AFTER a non-comment statement in both sides,
    // so the TS compiler (and our implementation) ignores it as a plain
    // comment. No directive findings must fire for either side.
    let directive_findings: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_DIRECTIVE_ADDED
                || f.rule == RULE_DIRECTIVE_REMOVED
                || f.rule == RULE_DIRECTIVE_CHANGED
                || f.rule == RULE_DIRECTIVE_REORDERED
        })
        .collect();
    assert!(
        directive_findings.is_empty(),
        "directives after a statement must be ignored; got: {:?}",
        directive_findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
