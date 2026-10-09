//! Private brand evidence compatibility checks.

use interlint::compare::subtype::{
    RULE_CLASS_FIELD_MADE_PRIVATE, RULE_CLASS_FIELD_MADE_PUBLIC,
    RULE_LIMITATION_PRIVATE_BRAND_ACCESS,
};
use interlint::compare::{compare_exports, compare_structural_diff};
use interlint::discover::discover_exports;
use interlint::types::Severity;
use std::path::{Path, PathBuf};

/// Helper: run the full pipeline (export diff + structural diff) on a fixture case.
/// Used for ClassShape-dependent tests (decorators, CLS-11, CLS-12).
fn run_full_pipeline(case: &str) -> Vec<interlint::types::Finding> {
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
fn private_brand_paired_info_on_made_private() {
    let findings = run_full_pipeline("javascript/classes/evidence/made-private");

    let made_private: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_CLASS_FIELD_MADE_PRIVATE && f.severity == Severity::Major)
        .collect();
    let info: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_LIMITATION_PRIVATE_BRAND_ACCESS && f.severity == Severity::Info)
        .collect();
    assert!(
        !made_private.is_empty(),
        "expected class-field-made-private Major; got rules: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
    assert_eq!(
        made_private.len(),
        info.len(),
        "every class-field-made-private must have a paired Info; made_private={}, info={}",
        made_private.len(),
        info.len()
    );
}

#[test]
fn private_brand_paired_info_on_made_public() {
    let findings = run_full_pipeline("javascript/classes/evidence/made-public");

    let made_public: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_CLASS_FIELD_MADE_PUBLIC && f.severity == Severity::Minor)
        .collect();
    let info: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_LIMITATION_PRIVATE_BRAND_ACCESS && f.severity == Severity::Info)
        .collect();
    assert!(
        !made_public.is_empty(),
        "expected class-field-made-public Minor; got rules: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
    assert_eq!(
        made_public.len(),
        info.len(),
        "every class-field-made-public must have a paired Info; made_public={}, info={}",
        made_public.len(),
        info.len()
    );
}
