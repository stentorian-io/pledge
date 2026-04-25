//! Phase 20 REF-09 integration tests: template-literal index signature key pattern diff.
//!
//! Three fixture forms exercise the full pipeline (source -> tree-sitter
//! extraction -> IndexSignature.key_type_text -> parse_template_literal_type
//! -> compare_template_literal_patterns -> interface_diff/ref_diff pre-check
//! emission -> Finding):
//!
//! 1. `narrowing` — Major `index-signature-key-pattern-changed` when
//!    `` `on${string}` `` is narrowed to `` `onClick${string}` ``.
//! 2. `widening` — Minor `index-signature-key-pattern-changed` when
//!    `` `onClick${string}` `` is widened to `` `on${string}` ``.
//! 3. `unchanged` — zero findings when the pattern is unchanged (even with
//!    an unrelated new `Helper` export added).
//!
//! Plus a fourth test confirming REF-07-twin suppression: when REF-09 fires
//! on the narrowing fixture, no `interface-index-signature-changed` finding
//! should accompany it.

use std::path::{Path, PathBuf};

use interlint::compare::subtype::{
    RULE_INDEX_SIGNATURE_CHANGED, RULE_INDEX_SIGNATURE_KEY_PATTERN_CHANGED,
    RULE_INTERFACE_INDEX_SIGNATURE_CHANGED,
};
use interlint::compare::{compare_exports, compare_structural_diff};
use interlint::discover::discover_exports;
use interlint::types::Severity;

const FIXTURE_ROOT: &str = "tests/fixtures/phase-20/ref-09-template-index";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn run(form: &str) -> Vec<interlint::types::Finding> {
    let old_dir = fixture_dir(form, "old");
    let new_dir = fixture_dir(form, "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    // REF-09 fires through both `compare_ref_changes` (type-alias level) and
    // `compare_interface_index_signatures` (interface level). The fixture
    // uses `interface EventMap`, which runs through compare_structural_diff
    // -> interface_diff::compare_interface_index_signatures. Compose both
    // top-level pipelines (mirrors the CLI composition in `src/main.rs`).
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
fn ref_09_template_index_narrowing() {
    let findings = run("narrowing");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_INDEX_SIGNATURE_KEY_PATTERN_CHANGED)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 index-signature-key-pattern-changed; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        hits[0].severity,
        Severity::Major,
        "narrowing must fire Major"
    );
}

#[test]
fn ref_09_template_index_widening() {
    let findings = run("widening");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_INDEX_SIGNATURE_KEY_PATTERN_CHANGED)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 index-signature-key-pattern-changed; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        hits[0].severity,
        Severity::Minor,
        "widening must fire Minor"
    );
}

#[test]
fn ref_09_template_index_unchanged() {
    let findings = run("unchanged");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_INDEX_SIGNATURE_KEY_PATTERN_CHANGED)
        .collect();
    assert!(
        hits.is_empty(),
        "unchanged pattern must fire 0 findings; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn ref_09_suppresses_ref_07_when_fired() {
    // For the narrowing fixture, REF-09 fires once. REF-07
    // (index-signature-changed) — and its interface-level twin
    // (interface-index-signature-changed) — must NOT also fire for the
    // same signature pair. Without dedup the consumer would see THREE
    // findings (removed + added + pattern-changed) instead of one.
    let findings = run("narrowing");
    let pattern_hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_INDEX_SIGNATURE_KEY_PATTERN_CHANGED)
        .collect();
    let string_equality_hits: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_INDEX_SIGNATURE_CHANGED
                || f.rule == RULE_INTERFACE_INDEX_SIGNATURE_CHANGED
        })
        .collect();
    assert_eq!(pattern_hits.len(), 1);
    assert!(
        string_equality_hits.is_empty(),
        "REF-07 / interface-index-signature-changed must be suppressed when REF-09 fires; got: {:?}",
        string_equality_hits
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
