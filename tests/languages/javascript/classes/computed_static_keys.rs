//! Computed static keys compatibility checks.

use pledge::core::findings::Severity;
use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare::compare_exports;
use pledge::languages::javascript::compare::rules::RULE_LIMITATION_DYNAMIC_STATIC_MEMBER_KEY;
use pledge::languages::javascript::compare::structure::compare_structural_diff;
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
fn constant_static_keys_are_compared_without_a_runtime_limitation() {
    let findings = run_full_pipeline("javascript/classes/evidence/computed-static-key");
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "class-method-removed"
                && finding.severity == Severity::Major
                && finding.message.contains("foo")),
        "{findings:?}"
    );
    assert!(
        !findings
            .iter()
            .any(|finding| finding.rule == RULE_LIMITATION_DYNAMIC_STATIC_MEMBER_KEY),
        "{findings:?}"
    );
}
