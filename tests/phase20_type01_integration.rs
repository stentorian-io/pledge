//! Phase 20 TYPE-01 integration tests: `satisfies` narrowing diff.
//!
//! Two fixture forms exercise the full pipeline (source -> tree-sitter
//! `satisfies_expression` extraction -> resolver populate pass ->
//! ExportSymbol.satisfies_type field -> compare_exports -> diff_satisfies_types
//! -> Finding):
//!
//! 1. `satisfies-removed` — Minor `type-narrowing-widened` when a
//!    `satisfies T` clause is removed from an exported const/let/var.
//! 2. `satisfies-unchanged` — zero findings when `satisfies T` is present
//!    on both sides (regardless of internal content changes).
//!
//! Degraded-mode behavior (no LSP): the finding still fires syntactically,
//! and the message carries `limitation: satisfies-narrowed-type-not-verified`
//! per the v0.3 Phase 20 locked decision (never silently drop coverage).

use std::path::{Path, PathBuf};

use interlint::compare::compare_exports;
use interlint::compare::subtype::RULE_TYPE_NARROWING_WIDENED;
use interlint::discover::discover_exports;
use interlint::types::Severity;

const FIXTURE_ROOT: &str = "tests/fixtures/phase-20/type-01-satisfies";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn run(form: &str) -> Vec<interlint::types::Finding> {
    let old_dir = fixture_dir(form, "old");
    let new_dir = fixture_dir(form, "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    compare_exports(&old_exports, &new_exports, &new_dir, true)
}

#[test]
fn type_01_satisfies_removed_fires_minor() {
    let findings = run("satisfies-removed");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_TYPE_NARROWING_WIDENED)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 type-narrowing-widened; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(hits[0].severity, Severity::Minor);
    assert!(
        hits[0].message.contains("routes"),
        "message should name the routes export; got: {}",
        hits[0].message
    );
}

#[test]
fn type_01_satisfies_unchanged_clean() {
    let findings = run("satisfies-unchanged");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_TYPE_NARROWING_WIDENED)
        .collect();
    assert!(
        hits.is_empty(),
        "expected 0 type-narrowing-widened; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn type_01_degraded_mode_finding_carries_limitation() {
    // Integration tests run in an environment that may not have LSP
    // available. When the narrowed-type slot is None, the finding message
    // must carry the `limitation: satisfies-narrowed-type-not-verified`
    // annotation so downstream consumers know narrowing direction was not
    // confirmed. With LSP available and narrowed populated, the message
    // instead contains "narrowed type was surfaced".
    let findings = run("satisfies-removed");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_TYPE_NARROWING_WIDENED)
        .collect();
    assert_eq!(hits.len(), 1);
    let msg = &hits[0].message;
    let verified = msg.contains("narrowed type was surfaced");
    let degraded = msg.contains("limitation: satisfies-narrowed-type-not-verified");
    assert!(
        verified || degraded,
        "finding message must state either verified or degraded narrowing; got: {}",
        msg
    );
}
