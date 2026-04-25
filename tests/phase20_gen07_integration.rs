//! Phase 20 GEN-07 integration tests: NoInfer<T> wrapper-removal diff.
//!
//! Three fixture forms exercise the full pipeline (source -> tree-sitter
//! type extraction -> parse_type_string -> TypeShape::NoInferWrap ->
//! diff engine -> Finding):
//!
//! 1. `noinfer-param-removed` — Minor `generic-inference-constraint-removed`
//!    when `NoInfer<T>` is removed from a function-parameter position
//!    on an exported function signature.
//! 2. `noinfer-constraint-removed` — Minor `generic-inference-constraint-removed`
//!    when `NoInfer<T>` is removed from a type-parameter constraint
//!    (`<T extends NoInfer<Base>>` -> `<T extends Base>`).
//! 3. `noinfer-unchanged` — zero `generic-inference-constraint-removed`
//!    findings when `NoInfer<T>` is preserved across versions.
//!
//! Both `compare_exports` and `compare_structural_diff` are invoked,
//! mirroring the CLI composition (see `src/main.rs`) and the TYPE-02
//! integration test (`tests/phase20_type02_integration.rs`).

use std::path::{Path, PathBuf};

use interlint::compare::subtype::RULE_GENERIC_INFERENCE_CONSTRAINT_REMOVED;
use interlint::compare::{compare_exports, compare_structural_diff};
use interlint::discover::discover_exports;
use interlint::types::Severity;

const FIXTURE_ROOT: &str = "tests/fixtures/phase-20/gen-07-noinfer";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn run(form: &str) -> Vec<interlint::types::Finding> {
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
fn gen_07_noinfer_removed_from_param() {
    let findings = run("noinfer-param-removed");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_GENERIC_INFERENCE_CONSTRAINT_REMOVED)
        .collect();
    assert!(
        !hits.is_empty(),
        "expected at least 1 generic-inference-constraint-removed at function-param position; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert!(
        hits.iter().all(|f| f.severity == Severity::Minor),
        "all GEN-07 findings must be Minor severity; got: {:?}",
        hits.iter()
            .map(|f| (f.severity.clone(), &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn gen_07_noinfer_removed_from_constraint() {
    let findings = run("noinfer-constraint-removed");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_GENERIC_INFERENCE_CONSTRAINT_REMOVED)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 generic-inference-constraint-removed at type-param constraint position; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(hits[0].severity, Severity::Minor);
    assert!(
        hits[0].message.contains("Store") || hits[0].message.contains("T"),
        "message should name the symbol or param; got: {}",
        hits[0].message
    );
}

#[test]
fn gen_07_noinfer_unchanged() {
    let findings = run("noinfer-unchanged");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_GENERIC_INFERENCE_CONSTRAINT_REMOVED)
        .collect();
    assert!(
        hits.is_empty(),
        "expected 0 generic-inference-constraint-removed when NoInfer preserved; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
