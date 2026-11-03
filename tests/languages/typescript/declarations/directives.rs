//! INF-05 integration tests: triple-slash directive diffs.
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

use pledge::core::findings::Severity;
use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare::compare_exports;
use pledge::languages::typescript::compare::rules::{
    RULE_DIRECTIVE_ADDED, RULE_DIRECTIVE_CHANGED, RULE_DIRECTIVE_REMOVED, RULE_DIRECTIVE_REORDERED,
};
use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "tests/fixtures/typescript/declarations/directives";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn run(form: &str) -> Vec<pledge::core::findings::Finding> {
    let old_dir = fixture_dir(form, "old");
    let new_dir = fixture_dir(form, "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    compare_exports(&old_exports, &new_exports, &new_dir, true)
}

#[test]
fn directive_added() {
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
fn directive_removed() {
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
fn directive_changed() {
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
fn directive_reordered() {
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
fn directive_position_ignored() {
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

#[test]
fn adding_or_removing_preserve_is_one_directive_change() {
    for attribute in [
        "path=\"./ambient.d.ts\"",
        "types=\"node\"",
        "lib=\"es2022\"",
    ] {
        for (old_preserve, new_preserve) in [("", " preserve=\"true\""), (" preserve=\"true\"", "")]
        {
            let old = tempfile::tempdir().unwrap();
            let new = tempfile::tempdir().unwrap();
            for (directory, preserve) in [(&old, old_preserve), (&new, new_preserve)] {
                std::fs::write(
                    directory.path().join("package.json"),
                    r#"{"types":"index.d.ts"}"#,
                )
                .unwrap();
                std::fs::write(
                    directory.path().join("index.d.ts"),
                    format!(
                    "/// <reference {attribute}{preserve} />\nexport declare const api: number;\n"
                ),
                )
                .unwrap();
            }
            let (before, _) = discover_exports(old.path()).unwrap();
            let (after, _) = discover_exports(new.path()).unwrap();

            let findings = compare_exports(&before, &after, new.path(), false);

            let directives: Vec<_> = findings
                .iter()
                .filter(|finding| finding.rule.starts_with("directive-"))
                .collect();
            assert_eq!(directives.len(), 1, "{directives:?}");
            assert_eq!(directives[0].rule, RULE_DIRECTIVE_CHANGED);
            assert_eq!(directives[0].severity, Severity::Major);
        }
    }
}
