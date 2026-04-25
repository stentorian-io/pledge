//! Phase 21 CLS-11 integration tests: ES2022 private-field visibility
//! transition detection on exported classes.
//!
//! Verifies that `compare_class_shapes` emits the correct Major/Minor finding
//! when a field transitions across the public ↔ private boundary (`foo` ↔
//! `#foo`) between old and new versions of a package. Also verifies
//! suppression (CLS-11 silent on private-space renames, CLS-03 suppressed
//! when CLS-11 fires, Risk 2 both-on-both-sides collision guard).

use std::path::{Path, PathBuf};

use interlint::compare::subtype::{
    RULE_CLASS_FIELD_MADE_PRIVATE, RULE_CLASS_FIELD_MADE_PUBLIC, RULE_CLASS_METHOD_REMOVED,
};
use interlint::compare::{compare_exports, compare_structural_diff};
use interlint::discover::discover_exports;
use interlint::types::{Finding, Severity};

const FIXTURE_ROOT: &str = "tests/fixtures/phase-21/cls-11-private-field";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn run(form: &str) -> Vec<Finding> {
    let old_dir = fixture_dir(form, "old");
    let new_dir = fixture_dir(form, "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    // CLS-11 findings originate in compare_structural_diff (which re-parses
    // each file to extract ClassShape / ClassMember). compare_exports alone
    // runs only the base ExportKey-iteration diff which does NOT touch
    // class shapes. Both must run to exercise the full pipeline.
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
fn cls_11_public_to_private() {
    let findings = run("public-to-private");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_CLASS_FIELD_MADE_PRIVATE && f.message.contains("`cache`"))
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 class-field-made-private on `cache`; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(hits[0].severity, Severity::Major);
}

#[test]
fn cls_11_private_to_public() {
    let findings = run("private-to-public");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_CLASS_FIELD_MADE_PUBLIC && f.message.contains("`cache`"))
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 class-field-made-public on `cache`; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(hits[0].severity, Severity::Minor);
}

#[test]
fn cls_11_rename_in_private_space_no_findings() {
    let findings = run("rename-in-private-space");
    let cls11_hits: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_CLASS_FIELD_MADE_PRIVATE || f.rule == RULE_CLASS_FIELD_MADE_PUBLIC
        })
        .collect();
    assert_eq!(
        cls11_hits.len(),
        0,
        "expected zero CLS-11 findings for private-space rename; got: {:?}",
        cls11_hits
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    // Also: no CLS-03 method-removed on `#cache` (the old bare name leaking out).
    let cls03_hits: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_CLASS_METHOD_REMOVED
                && (f.message.contains("#cache") || f.message.contains("#store"))
        })
        .collect();
    assert_eq!(
        cls03_hits.len(),
        0,
        "expected zero CLS-03 findings leaking private member names; got: {:?}",
        cls03_hits
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn cls_11_both_on_both_sides_no_findings() {
    // Class has BOTH `foo` public AND `#foo` private on both old and new.
    // Risk 2: the naive bare-name matcher would pair public `foo` ↔
    // private `#foo` and fire a spurious transition. The collision guard
    // must suppress both directions.
    let findings = run("both-on-both-sides");
    let cls11_hits: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_CLASS_FIELD_MADE_PRIVATE || f.rule == RULE_CLASS_FIELD_MADE_PUBLIC
        })
        .collect();
    assert_eq!(
        cls11_hits.len(),
        0,
        "expected zero CLS-11 findings for class with both `foo` and `#foo` on both sides; got: {:?}",
        cls11_hits.iter().map(|f| (f.rule, &f.message)).collect::<Vec<_>>()
    );
}

#[test]
fn cls_11_private_method_in_private_space_no_findings() {
    // `#compute()` → `#recalculate()`. Private-space rename; no public
    // surface change. Must fire 0 CLS-03 class-method-removed.
    let findings = run("private-method-in-private-space");
    let cls03_hits: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_CLASS_METHOD_REMOVED
                && (f.message.contains("#compute") || f.message.contains("#recalculate"))
        })
        .collect();
    assert_eq!(
        cls03_hits.len(),
        0,
        "expected zero CLS-03 findings for private-method rename; got: {:?}",
        cls03_hits
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn cls_11_suppresses_cls_03_when_transition_fires() {
    // For public-to-private, CLS-11 fires once. CLS-03 must be suppressed
    // for the same bare name — consumers see exactly one finding, not two.
    let findings = run("public-to-private");
    let cls11_hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_CLASS_FIELD_MADE_PRIVATE && f.message.contains("`cache`"))
        .collect();
    let cls03_hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_CLASS_METHOD_REMOVED && f.message.contains("`cache`"))
        .collect();
    assert_eq!(cls11_hits.len(), 1);
    assert_eq!(
        cls03_hits.len(),
        0,
        "CLS-03 must be suppressed when CLS-11 fires on the same bare name; got: {:?}",
        cls03_hits
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
