//! Phase 25 INT-01 — Reference validation suite.
//!
//! Runs the full Interlint pipeline against pinned `from.tgz` + `to.tgz` for
//! 5 published packages and asserts (exit_code, sorted unique rule_ids) matches
//! the curated expected-findings.json per package.
//!
//! Per v0.3-decisions: shape-regression is the target, message churn is noise.
//! The reference suite is the v0.3 milestone-close gate — wired into CI via
//! .github/workflows/ci.yml so a regression on any package fails the build.
//!
//! LSP-degraded mode is FORCED for determinism (RESEARCH §Pitfall 2): CI
//! runners may not have typescript-language-server installed and the
//! reference-suite test must produce identical findings on every machine.

use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

use interlint::acquire::extract_tgz_from_bytes;
use interlint::compare;
use interlint::discover::discover_exports;
use interlint::types::Finding;

#[derive(Deserialize)]
struct Expected {
    /// Package name for diagnostic output (e.g., "react").
    #[serde(default)]
    package: String,
    /// Source from-version for diagnostic output (informational).
    #[serde(default)]
    from_version: String,
    /// Source to-version for diagnostic output.
    #[serde(default)]
    to_version: String,
    /// Asserted exit code: 0 (clean / minor / patch / warning / info), 1 (breaking), 3 (degraded).
    /// 2 is reserved for tool-internal errors and never appears here.
    exit_code: i32,
    /// Sorted, deduplicated list of `Finding.rule` strings the suite should produce.
    rule_ids: Vec<String>,
    /// Free-form notes from the curator. Ignored by the harness; informational only.
    #[serde(default, rename = "_curation_notes")]
    _curation_notes: String,
}

fn fixture_dir(pkg: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/reference-suite")
        .join(pkg)
}

/// Mirrors src/main.rs:202-242 with `lsp_was_available=false` forced.
/// Returns (exit_code, sorted_unique_rule_ids).
fn run_pipeline(old_root: &Path, new_root: &Path) -> (i32, Vec<String>) {
    let (old_exports, _w_old) = discover_exports(old_root).expect("discover_exports failed for old version");
    let (new_exports, _w_new) = discover_exports(new_root).expect("discover_exports failed for new version");

    // LSP-degraded forced (RESEARCH §Pitfall 2).
    let mut findings: Vec<Finding> = compare::compare_exports(&old_exports, &new_exports, new_root, false);
    findings.extend(compare::metadata::compare_metadata(old_root, new_root));
    findings.extend(compare::compare_exports_map_diff(old_root, new_root));
    findings.extend(compare::compare_structural_diff(old_root, new_root, &old_exports, &new_exports));
    findings.extend(compare::compare_asset_diff(old_root, new_root));

    // Sort + dedup mirrors src/main.rs's two-pass dedup at lines 233-239.
    let mut seen = HashSet::new();
    findings.retain(|f| seen.insert((f.rule, f.file.clone(), f.line, f.col, f.message.clone())));
    findings.sort_by(|a, b| a.severity.cmp(&b.severity));
    seen.clear();
    findings.retain(|f| seen.insert((f.rule, f.file.clone(), f.line, f.col, f.message.clone())));

    let exit_code = if compare::has_breaking_changes(&findings) { 1 } else { 0 };
    let mut rule_ids: Vec<String> = findings.iter().map(|f| f.rule.to_string()).collect();
    rule_ids.sort();
    rule_ids.dedup();
    (exit_code, rule_ids)
}

fn run_reference_case(pkg: &str) {
    let dir = fixture_dir(pkg);
    let from_bytes = std::fs::read(dir.join("from.tgz"))
        .unwrap_or_else(|e| panic!("read from.tgz for {}: {}", pkg, e));
    let to_bytes = std::fs::read(dir.join("to.tgz"))
        .unwrap_or_else(|e| panic!("read to.tgz for {}: {}", pkg, e));
    let expected_bytes = std::fs::read(dir.join("expected-findings.json"))
        .unwrap_or_else(|e| panic!("read expected-findings.json for {}: {}", pkg, e));
    let expected: Expected = serde_json::from_slice(&expected_bytes)
        .unwrap_or_else(|e| panic!("parse expected-findings.json for {}: {}", pkg, e));

    let (_old_dir, old_root) = extract_tgz_from_bytes(&from_bytes)
        .unwrap_or_else(|e| panic!("extract from.tgz for {}: {}", pkg, e));
    let (_new_dir, new_root) = extract_tgz_from_bytes(&to_bytes)
        .unwrap_or_else(|e| panic!("extract to.tgz for {}: {}", pkg, e));

    let (actual_exit, actual_rules) = run_pipeline(&old_root, &new_root);

    assert_eq!(
        actual_exit, expected.exit_code,
        "{}: exit_code mismatch — expected {}, got {}. Rules: {:?}",
        pkg, expected.exit_code, actual_exit, actual_rules
    );
    assert_eq!(
        actual_rules, expected.rule_ids,
        "{}: rule-IDs mismatch.\n  Expected: {:?}\n  Actual:   {:?}\n  Diff (in actual not expected): {:?}\n  Diff (in expected not actual): {:?}",
        pkg, expected.rule_ids, actual_rules,
        actual_rules.iter().filter(|r| !expected.rule_ids.contains(r)).collect::<Vec<_>>(),
        expected.rule_ids.iter().filter(|r| !actual_rules.contains(r)).collect::<Vec<_>>(),
    );

    // Suppress unused-field warning for informational fields.
    let _ = &expected.package;
    let _ = &expected.from_version;
    let _ = &expected.to_version;
}

#[test]
fn reference_react() {
    run_reference_case("react");
}

#[test]
fn reference_zod() {
    run_reference_case("zod");
}

#[test]
fn reference_prisma() {
    run_reference_case("prisma");
}

#[test]
fn reference_tanstack_query() {
    run_reference_case("tanstack-query");
}

#[test]
fn reference_trpc() {
    run_reference_case("trpc");
}
