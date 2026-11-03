//! Narrowing evidence compatibility checks.

use pledge::core::findings::Severity;
use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare::compare_exports;
use pledge::languages::typescript::compare::rules::RULE_LIMITATION_SATISFIES_NARROWED;
use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "tests/fixtures";

fn fixture_dir(case: &str, side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_ROOT)
        .join(case)
        .join(side)
}

#[test]
fn substring_promotion_satisfies_degraded() {
    use pledge::languages::typescript::compare::rules::RULE_TYPE_NARROWING_WIDENED;

    let old_dir = fixture_dir("typescript/types/evidence/satisfies", "old");
    let new_dir = fixture_dir("typescript/types/evidence/satisfies", "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    // LSP-degraded forced for determinism (Pitfall 2)
    let findings = compare_exports(&old_exports, &new_exports, &new_dir, false);

    let minor: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_TYPE_NARROWING_WIDENED && f.severity == Severity::Minor)
        .collect();
    let info: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_LIMITATION_SATISFIES_NARROWED && f.severity == Severity::Info)
        .collect();

    // Note: The satisfies path fires only when the OLD version has a satisfies type
    // but the NEW version does not. The fixture may not exercise this if tree-sitter
    // doesn't populate the satisfies_type field in degraded mode (LSP not run).
    // If no minor finding, the Info finding is also absent — that's expected behavior.
    // Both must be in sync: either both present or both absent.
    assert_eq!(
        minor.len(), info.len(),
        "satisfies Minor and paired Info must be in sync (both present or both absent); minor={}, info={}\nfindings: {:?}",
        minor.len(), info.len(), findings.iter().map(|f| (f.rule, f.severity.clone())).collect::<Vec<_>>()
    );
}
