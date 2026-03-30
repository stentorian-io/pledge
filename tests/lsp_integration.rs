//! Integration tests for Phase 2: LSP Type Enrichment
//!
//! These tests require a real typescript-language-server to be available.
//! Run with: cargo test --features lsp-tests --test lsp_integration
//! Standard `cargo test` skips these tests entirely.

#![cfg(feature = "lsp-tests")]

use std::path::PathBuf;

use interlint::compare;
use interlint::discover;
use interlint::lsp;
use interlint::types::{ExportKind, ExportSymbol, Finding, Severity};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Run the full pipeline: discover -> enrich -> compare
async fn run_pipeline(old_dir: &str, new_dir: &str) -> (Vec<Finding>, bool) {
    let old_root = fixture_path(old_dir);
    let new_root = fixture_path(new_dir);

    let (mut old_exports, _) = discover::discover_exports(&old_root).unwrap();
    let (mut new_exports, _) = discover::discover_exports(&new_root).unwrap();

    // Enrich both (may degrade if LSP not available — test handles both)
    let old_result = lsp::enrich_exports(&mut old_exports, &old_root).await;
    let new_result = lsp::enrich_exports(&mut new_exports, &new_root).await;

    // Assert enrichment succeeded (these tests require LSP)
    assert!(
        matches!(old_result, Ok(lsp::EnrichResult::Enriched)),
        "LSP enrichment failed for old: {:?}",
        old_result
    );
    assert!(
        matches!(new_result, Ok(lsp::EnrichResult::Enriched)),
        "LSP enrichment failed for new: {:?}",
        new_result
    );

    let findings = compare::compare_exports(&old_exports, &new_exports);
    let has_breaking = compare::has_breaking_changes(&findings);
    (findings, has_breaking)
}

/// Test: renamed type alias (Config -> Settings, same shape) does NOT produce false positive.
/// Validates STRUCT-01 (structural shape comparison) and STRUCT-02 (alias resolution).
///
/// configure() param type changes from Config to Settings but shapes are identical —
/// should NOT produce a `type-changed` finding.
#[tokio::test]
async fn renamed_type_alias_same_shape_no_false_positive() {
    let (findings, _) = run_pipeline("type-change-pkg/old", "type-change-pkg/new").await;

    // Config was removed (different export name), Settings was added
    // But no type-changed finding for configure() param (Config vs Settings have same shape)
    let configure_type_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains("configure") && f.rule == "type-changed")
        .collect();
    assert!(
        configure_type_findings.is_empty(),
        "configure() should not have type-changed when param shapes are identical: {:?}",
        configure_type_findings
    );
}

/// Test: nested field removal (AppSettings.database.ssl removed) is detected.
/// Validates STRUCT-03 (recursive nested object comparison).
#[tokio::test]
async fn nested_field_removal_detected() {
    let (findings, has_breaking) = run_pipeline("type-change-pkg/old", "type-change-pkg/new").await;

    // AppSettings.database.ssl was removed
    let ssl_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains("ssl") || f.message.contains("database"))
        .collect();
    assert!(
        !ssl_findings.is_empty(),
        "Should detect removal of database.ssl field. All findings: {:?}",
        findings
    );
    assert!(has_breaking, "Nested field removal is a breaking change");
}

/// Test: field removal from User type (age removed) is detected.
/// Validates basic field-removed rule.
#[tokio::test]
async fn field_removal_detected() {
    let (findings, _) = run_pipeline("type-change-pkg/old", "type-change-pkg/new").await;

    // User.age was removed
    let age_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains("age") && f.rule == "field-removed")
        .collect();
    assert!(
        !age_findings.is_empty(),
        "Should detect removal of age field from User. All findings: {:?}",
        findings
    );
}

/// Test: type change (timeout: number -> string) is detected.
/// Validates type-incompatible rule for primitive type changes.
/// Note: rule name is "type-incompatible" (the canonical catch-all for primitive/cross-kind diffs).
#[tokio::test]
async fn type_change_detected() {
    let (findings, _) = run_pipeline("type-change-pkg/old", "type-change-pkg/new").await;

    // timeout changed from number to string — emits type-incompatible per D-02
    let timeout_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains("timeout") && f.rule == "type-incompatible")
        .collect();
    assert!(
        !timeout_findings.is_empty(),
        "Should detect timeout type change number->string. All findings: {:?}",
        findings
    );
}

/// Test: JSDoc-annotated JavaScript files produce type information via LSP.
/// Validates STRUCT-04 (LSP type resolution for JSDoc JavaScript).
/// Note: type change findings use "type-incompatible" or "param-type-narrowed" rules per D-02.
#[tokio::test]
async fn jsdoc_javascript_type_resolution() {
    let (findings, has_breaking) = run_pipeline("jsdoc-pkg/old", "jsdoc-pkg/new").await;

    // createUser param 'age' changed from number to string (param-type-narrowed or type-incompatible)
    // maxRetries changed from number to string (type-incompatible)
    let type_findings: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == "type-incompatible"
                || f.rule == "field-type-changed"
                || f.rule == "param-type-narrowed"
        })
        .collect();
    assert!(
        !type_findings.is_empty(),
        "JSDoc JavaScript should yield type changes. All findings: {:?}",
        findings
    );
    assert!(has_breaking, "JSDoc type changes are breaking");
}

/// Test: SIG-01..05 signature changes detected with LSP enrichment.
/// Validates that all five signature change rules produce findings when type shapes are available.
#[tokio::test]
async fn sig_changes_detected_with_lsp() {
    let (findings, has_breaking) = run_pipeline("sig-change-pkg/old", "sig-change-pkg/new").await;
    let rules: Vec<&str> = findings.iter().map(|f| f.rule).collect();

    // SIG-01: added required param (greet now takes 2 required params instead of 1)
    assert!(
        rules.contains(&"param-added"),
        "SIG-01: param-added not found in rules: {:?}",
        rules
    );

    // SIG-02: narrowed param type (process input: string | number -> string)
    assert!(
        rules.contains(&"param-type-narrowed"),
        "SIG-02: param-type-narrowed not found in rules: {:?}",
        rules
    );

    // SIG-03: widened return type (getData returns string -> string | number)
    assert!(
        rules.contains(&"return-type-widened"),
        "SIG-03: return-type-widened not found in rules: {:?}",
        rules
    );

    // SIG-04: optional param added (configure gets optional port?) — should be Minor
    // param-added is used for both required (Major) and optional (Minor) params
    let param_added_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "param-added")
        .collect();
    assert!(
        param_added_findings
            .iter()
            .any(|f| f.severity == Severity::Minor),
        "SIG-04: should have a Minor param-added finding for optional param, got: {:?}",
        param_added_findings
    );

    // SIG-05: param order changed (transform(a, b) -> transform(b, a))
    assert!(
        rules.contains(&"param-reordered"),
        "SIG-05: param-reordered not found in rules: {:?}",
        rules
    );

    // Overall: signature changes ARE breaking
    assert!(has_breaking, "SIG changes should produce breaking findings");
}

/// Test: graceful degradation when LSP cannot initialize for a directory.
/// Validates STRUCT-05 (graceful degradation — no panic, returns Degraded or Enriched).
///
/// We simulate LSP failure by passing a non-existent directory — the LSP cannot
/// open files there, but enrich_exports should not panic or return Err.
#[tokio::test]
async fn graceful_degradation_without_lsp() {
    let fake_root = PathBuf::from("/tmp/interlint-test-nonexistent-dir-a51e5386");

    // Build a minimal ExportMap with one symbol referencing the fake directory
    let mut exports = interlint::types::ExportMap::new();
    exports.insert(
        "test".to_string(),
        ExportSymbol {
            name: "test".to_string(),
            kind: ExportKind::Named,
            file: fake_root.join("index.ts"),
            line: 1,
            col: 0,
            type_signature: None,
            type_shape: None,
        },
    );

    let result = lsp::enrich_exports(&mut exports, &fake_root).await;

    // Must not return Err — graceful degradation is required (D-16, D-18)
    match result {
        Ok(lsp::EnrichResult::Degraded(_)) => {
            // Expected — LSP failed on nonexistent dir; exports unchanged
            assert!(
                exports["test"].type_shape.is_none(),
                "Degraded mode: type_shape should remain None"
            );
        }
        Ok(lsp::EnrichResult::Enriched) => {
            // Also acceptable — LSP may initialize but find no files to resolve
            // Exports should still have no type_shape since file doesn't exist
        }
        Err(e) => {
            panic!(
                "enrich_exports should not return Err — must degrade gracefully: {:?}",
                e
            );
        }
    }
}
