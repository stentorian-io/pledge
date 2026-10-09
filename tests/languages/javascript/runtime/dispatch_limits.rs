//! Dispatch limits compatibility checks.

use interlint::compare::compare_exports;
use interlint::compare::subtype::RULE_LIMITATION_PROXY_DISPATCHED_EXPORTS;
use interlint::discover::discover_exports;
use interlint::types::Severity;
use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "tests/fixtures";

fn fixture_dir(case: &str, side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_ROOT)
        .join(case)
        .join(side)
}

#[test]
fn proxy_dispatched_exports_flag_set() {
    use interlint::discover::tree_query::{parse_exports, SourceLanguage};
    let source = b"module.exports = new Proxy({}, { get: () => process.env.VALUE });";
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let default_exp = raw
        .iter()
        .find(|e| e.name == "default")
        .expect("default export not captured from Proxy RHS");
    assert!(
        default_exp.is_proxy_dispatched,
        "is_proxy_dispatched flag must be set for Proxy RHS"
    );
}

#[test]
fn proxy_dispatched_exports_full_pipeline() {
    let old_dir = fixture_dir("javascript/runtime/opaque-proxy", "old");
    let new_dir = fixture_dir("javascript/runtime/opaque-proxy", "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    let findings = compare_exports(&old_exports, &new_exports, &new_dir, false);

    let info: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_LIMITATION_PROXY_DISPATCHED_EXPORTS && f.severity == Severity::Info
        })
        .collect();
    assert!(
        !info.is_empty(),
        "expected at least 1 Info limitation: proxy-dispatched-exports; got rules: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, f.severity.clone()))
            .collect::<Vec<_>>()
    );
}
