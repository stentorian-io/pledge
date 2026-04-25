//! Phase 20 BEH-06 integration tests: discriminated-union variant added.
//!
//! Three fixture forms exercise the full pipeline (source -> tree-sitter
//! extraction -> type-alias RHS parse -> TypeShape::Union ->
//! compare_exports + compare_structural_diff -> diff_recursive (Union, Union)
//! arm -> Finding):
//!
//! 1. `discriminated-variant-added` — Warning `union-variant-added` when a
//!    variant is added to an exported discriminated (tagged) union.
//! 2. `plain-union-widened` — zero `union-variant-added` findings when a
//!    plain union widens (existing TYPE-diff compatible-widening behavior
//!    is preserved; scope is discriminated unions only).
//! 3. `discriminated-union-unchanged` — zero `union-variant-added` findings
//!    when the discriminated union is identical (added unrelated Helper
//!    export in the new version to confirm unrelated additions don't fire
//!    BEH-06).

use std::path::{Path, PathBuf};

use interlint::compare::subtype::RULE_UNION_VARIANT_ADDED;
use interlint::compare::{compare_exports, compare_structural_diff};
use interlint::discover::discover_exports;
use interlint::types::Severity;

const FIXTURE_ROOT: &str = "tests/fixtures/phase-20/beh-06-union-variant";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn run(form: &str) -> Vec<interlint::types::Finding> {
    let old_dir = fixture_dir(form, "old");
    let new_dir = fixture_dir(form, "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    // BEH-06 fires through the Union-Union arm in `diff_recursive`, which is
    // reached for type-alias bodies only via `compare_structural_diff`
    // (mirrors the TYPE-02 integration test and the CLI composition in
    // `src/main.rs`).
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
fn beh_06_discriminated_union_variant_added() {
    let findings = run("discriminated-variant-added");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_UNION_VARIANT_ADDED)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 union-variant-added finding for the added `triangle` variant; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(hits[0].severity, Severity::Warning);
    assert!(
        hits[0].message.contains("triangle") || hits[0].message.contains("Shape"),
        "message should reference the added variant discriminant or the symbol; got: {}",
        hits[0].message
    );
    assert!(
        hits[0].message.contains("kind"),
        "message should reference the discriminant property `kind`; got: {}",
        hits[0].message
    );
}

#[test]
fn beh_06_plain_union_widening_still_clean() {
    let findings = run("plain-union-widened");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_UNION_VARIANT_ADDED)
        .collect();
    assert!(
        hits.is_empty(),
        "plain-union widening must not fire union-variant-added (discriminated-only scope); findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn beh_06_discriminated_union_unchanged() {
    let findings = run("discriminated-union-unchanged");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_UNION_VARIANT_ADDED)
        .collect();
    assert!(
        hits.is_empty(),
        "unchanged discriminated union must not fire union-variant-added; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
