//! Phase 21 CLS-12 integration tests: class static initialization block
//! side-effect exports diffed as class members.
//!
//! The extractor (`src/discover/tree_query.rs::parse_class_members`)
//! synthesizes a `ClassMember { kind: Property, is_static: true }` for each
//! top-level `Foo.bar = expr` or `this.bar = expr` assignment inside a
//! `class_static_block`. Subscript-expression LHS (`Foo[x] = ...`) sets
//! `ClassShape.has_dynamic_static_key`, surfaced by `compare_class_shapes`
//! as a Minor finding with the `(limitation: dynamic-static-member-key)`
//! annotation embedded in the message (TYPE-01 precedent).
//!
//! CLS-12 does NOT introduce a new rule-ID — synthesized member removal is
//! reported as `class-method-removed` (the CLS-03 loop was widened in the
//! same plan to include `ClassMemberKind::Property` gated on `is_static`).

use std::path::{Path, PathBuf};

use interlint::compare::subtype::{RULE_CLASS_METHOD_REMOVED, RULE_CLASS_STATIC_TYPE_CHANGED};
use interlint::compare::{compare_exports, compare_structural_diff};
use interlint::discover::discover_exports;
use interlint::types::{Finding, Severity};

const FIXTURE_ROOT: &str = "tests/fixtures/phase-21/cls-12-static-block";

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
fn cls_12_assignment_removed_fires_major() {
    let findings = run("assignment-removed");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_CLASS_METHOD_REMOVED
                && f.message.contains("defaults")
                && f.message.contains("Registry")
                && f.message.contains("static property")
        })
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 class-method-removed on `defaults` for Registry with `static property` noun; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(hits[0].severity, Severity::Major);
}

#[test]
fn cls_12_assignment_added_no_findings() {
    let findings = run("assignment-added");
    let removed_hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_CLASS_METHOD_REMOVED)
        .collect();
    assert_eq!(
        removed_hits.len(),
        0,
        "added static-block assignment must not fire class-method-removed; got: {:?}",
        removed_hits
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn cls_12_dynamic_key_limitation_fires() {
    let findings = run("dynamic-key");
    let limitation_hits: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains("(limitation: dynamic-static-member-key)"))
        .collect();
    assert_eq!(
        limitation_hits.len(),
        1,
        "expected exactly 1 (limitation: dynamic-static-member-key) finding; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    // Minor severity — matches Phase 20 TYPE-01 precedent for limitation messages.
    assert_eq!(limitation_hits[0].severity, Severity::Minor);
    // Host rule-ID reuses existing RULE_CLASS_STATIC_TYPE_CHANGED (CLS-07).
    assert_eq!(limitation_hits[0].rule, RULE_CLASS_STATIC_TYPE_CHANGED);
}

#[test]
fn cls_12_this_assignment_anon_class_fires() {
    // Anonymous default-exported class with `this.version = '1.0.0'`. The
    // `is_class_ref` gate in the extractor must fall through to the `this`
    // path when class_name is empty (anonymous).
    let findings = run("this-assignment-anon-class");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_CLASS_METHOD_REMOVED && f.message.contains("version")
        })
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 class-method-removed on `version` from anon class; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(hits[0].severity, Severity::Major);
}

#[test]
fn cls_12_multiple_static_blocks_all_walked() {
    // Config class has 2 static blocks. Second block's assignment
    // (Config.version) is removed — the cursor loop must walk BOTH blocks
    // to spot the removal.
    let findings = run("multiple-static-blocks");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_CLASS_METHOD_REMOVED
                && f.message.contains("version")
                && f.message.contains("Config")
        })
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 class-method-removed on `version` from second static block; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(hits[0].severity, Severity::Major);
}
