//! REF-10 integration tests: tuple-label detection.
//!
//! Tuple labels are documentation metadata; they do not change assignability.
//!   - tuple-label-renamed (Minor)
//!   - tuple-label-reordered (Patch) — multiset-preserved, position-changed pre-pass
//!   - tuple-label-dropped (Patch)
//!   - tuple-label-added (Minor)

use pledge::core::findings::{Finding, Severity};
use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare::compare_exports;
use pledge::languages::javascript::compare::structure::compare_structural_diff;
use pledge::languages::typescript::compare::rules::{
    RULE_TUPLE_LABEL_ADDED, RULE_TUPLE_LABEL_DROPPED, RULE_TUPLE_LABEL_RENAMED,
    RULE_TUPLE_LABEL_REORDERED,
};
use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "tests/fixtures/typescript/types/tuple-labels";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn run(form: &str) -> Vec<Finding> {
    let old_dir = fixture_dir(form, "old");
    let new_dir = fixture_dir(form, "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    let mut findings = compare_exports(&old_exports, &new_exports, &new_dir, true);
    findings.extend(compare_structural_diff(
        &old_dir,
        &new_dir,
        &old_exports,
        &new_exports,
    ));
    findings
}

#[test]
fn label_renamed_minor() {
    let findings = run("rename");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_TUPLE_LABEL_RENAMED)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 tuple-label-renamed; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert!(
        matches!(hits[0].severity, Severity::Minor),
        "tuple-label-renamed must be Minor; got {:?}",
        hits[0].severity
    );
    assert!(
        hits[0].message.contains('x') && hits[0].message.contains("lat"),
        "message should mention both old label `x` and new label `lat`; got: {}",
        hits[0].message
    );
}

#[test]
fn label_reordered_cosmetic() {
    let findings = run("reorder");
    let reorder_hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_TUPLE_LABEL_REORDERED)
        .collect();
    assert_eq!(
        reorder_hits.len(),
        1,
        "expected exactly 1 tuple-label-reordered; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert!(
        matches!(reorder_hits[0].severity, Severity::Patch),
        "tuple-label-reordered must be Patch; got {:?}",
        reorder_hits[0].severity
    );
    let rename_hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_TUPLE_LABEL_RENAMED)
        .collect();
    assert_eq!(
        rename_hits.len(),
        0,
        "reorder pre-pass must suppress per-position rename findings; got rename hits: {:?}",
        rename_hits.iter().map(|f| &f.message).collect::<Vec<_>>()
    );
}

#[test]
fn label_dropped_cosmetic() {
    let findings = run("drop");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_TUPLE_LABEL_DROPPED)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 tuple-label-dropped; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert!(
        matches!(hits[0].severity, Severity::Patch),
        "tuple-label-dropped must be Patch; got {:?}",
        hits[0].severity
    );
    assert!(
        hits[0].message.contains('x'),
        "message should mention dropped label `x`; got: {}",
        hits[0].message
    );
}

#[test]
fn unchanged_clean() {
    let findings = run("unchanged");
    let label_family: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_TUPLE_LABEL_RENAMED
                || f.rule == RULE_TUPLE_LABEL_REORDERED
                || f.rule == RULE_TUPLE_LABEL_DROPPED
                || f.rule == RULE_TUPLE_LABEL_ADDED
        })
        .collect();
    assert_eq!(
        label_family.len(),
        0,
        "unchanged tuple must produce zero label-family findings; got: {:?}",
        label_family
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn label_added_minor() {
    let findings = run("add");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_TUPLE_LABEL_ADDED)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 tuple-label-added; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert!(
        matches!(hits[0].severity, Severity::Minor),
        "tuple-label-added must be Minor; got {:?}",
        hits[0].severity
    );
}

#[test]
fn variadic_preserved() {
    let findings = run("variadic");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_TUPLE_LABEL_RENAMED)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "variadic rest-position label rename should fire tuple-label-renamed; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
