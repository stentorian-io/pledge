//! Phase 22 REF-10 integration tests: tuple-label detection.
//!
//! CONTEXT.md D-08 locks 4 granular rule-IDs:
//!   - tuple-label-renamed (Minor)
//!   - tuple-label-reordered (Major) — multiset-preserved, position-changed pre-pass
//!   - tuple-label-dropped (Major)
//!   - tuple-label-added (Minor)

use std::path::{Path, PathBuf};

use interlint::compare::subtype::{
    RULE_TUPLE_LABEL_ADDED, RULE_TUPLE_LABEL_DROPPED, RULE_TUPLE_LABEL_RENAMED,
    RULE_TUPLE_LABEL_REORDERED,
};
use interlint::compare::{compare_exports, compare_structural_diff};
use interlint::discover::discover_exports;
use interlint::types::{Finding, Severity};

const FIXTURE_ROOT: &str = "tests/fixtures/phase-22/ref-10-tuple-labels";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn run(form: &str) -> Vec<Finding> {
    let old_dir = fixture_dir(form, "old");
    let new_dir = fixture_dir(form, "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    let mut findings = compare_exports(&old_exports, &new_exports, &new_dir);
    findings.extend(compare_structural_diff(
        &old_dir,
        &new_dir,
        &old_exports,
        &new_exports,
    ));
    findings
}

#[test]
fn ref_10_label_renamed_minor() {
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
fn ref_10_label_reordered_major() {
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
        matches!(reorder_hits[0].severity, Severity::Major),
        "tuple-label-reordered must be Major; got {:?}",
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
fn ref_10_label_dropped_major() {
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
        matches!(hits[0].severity, Severity::Major),
        "tuple-label-dropped must be Major; got {:?}",
        hits[0].severity
    );
    assert!(
        hits[0].message.contains('x'),
        "message should mention dropped label `x`; got: {}",
        hits[0].message
    );
}

#[test]
fn ref_10_unchanged_clean() {
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
fn ref_10_label_added_minor() {
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
fn ref_10_variadic_preserved() {
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
