//! Phase 20 TYPE-02 integration tests: `infer U extends C` constraint diff.
//!
//! Two fixture forms exercise the full pipeline (source -> tree-sitter
//! extraction -> LSP hover / type-string parse -> TypeShape::InferExtends ->
//! compare_exports -> diff_recursive (InferExtends, InferExtends) arm ->
//! Finding):
//!
//! 1. `constraint-changed` — Major `infer-extends-constraint-changed` when
//!    the `extends` constraint of an `infer U extends C` clause changes
//!    (e.g. `string` -> `number`).
//! 2. `constraint-unchanged` — zero `infer-extends-constraint-changed`
//!    findings when the constraint is the same across versions, even if
//!    an unrelated symbol was added nearby.

use std::path::{Path, PathBuf};

use interlint::compare::compare_exports;
use interlint::compare::subtype::RULE_INFER_EXTENDS_CONSTRAINT_CHANGED;
use interlint::discover::discover_exports;
use interlint::types::Severity;

const FIXTURE_ROOT: &str = "tests/fixtures/phase-20/type-02-infer-extends";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn run(form: &str) -> Vec<interlint::types::Finding> {
    let old_dir = fixture_dir(form, "old");
    let new_dir = fixture_dir(form, "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    compare_exports(&old_exports, &new_exports, &new_dir)
}

#[test]
fn type_02_infer_extends_constraint_changed() {
    let findings = run("constraint-changed");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_INFER_EXTENDS_CONSTRAINT_CHANGED)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 infer-extends-constraint-changed; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(hits[0].severity, Severity::Major);
    assert!(
        hits[0].message.contains("FirstIfString"),
        "message should name the symbol; got: {}",
        hits[0].message
    );
}

#[test]
fn type_02_infer_extends_constraint_unchanged() {
    let findings = run("constraint-unchanged");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_INFER_EXTENDS_CONSTRAINT_CHANGED)
        .collect();
    assert!(
        hits.is_empty(),
        "expected 0 infer-extends-constraint-changed on unchanged constraint; findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
