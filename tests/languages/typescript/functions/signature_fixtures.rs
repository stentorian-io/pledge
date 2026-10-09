//! Signature fixtures compatibility checks.

use interlint::compare;
use interlint::discover;
use interlint::types::Severity;
use std::path::Path;

use crate::fixtures::fixture_path;

/// Helper: discover exports from a fixture directory.
fn discover_fixture(path: &Path) -> interlint::types::ExportMap {
    let (exports, _warnings) = discover::discover_exports(path).expect("discover should succeed");
    exports
}

/// Helper: collect rule IDs from findings.
fn rule_ids(findings: &[interlint::types::Finding]) -> Vec<&'static str> {
    findings.iter().map(|f| f.rule).collect()
}

/// Helper: filter findings by rule.
fn findings_with_rule<'a>(
    findings: &'a [interlint::types::Finding],
    rule: &str,
) -> Vec<&'a interlint::types::Finding> {
    findings.iter().filter(|f| f.rule == rule).collect()
}

/// DET-03, DET-04, OUT-05: new exports detected as Minor severity.
///
/// sig-change-pkg/new adds `newFeature` which is absent from old.
/// This verifies the export-added finding is produced and is Minor severity.
#[test]
fn test_signature_changes_detected() {
    let old_path = fixture_path("typescript/functions/signatures/old");
    let new_path = fixture_path("typescript/functions/signatures/new");

    let old_exports = discover_fixture(&old_path);
    let new_exports = discover_fixture(&new_path);
    let findings =
        compare::compare_exports(&old_exports, &new_exports, std::path::Path::new("."), true);

    let rules = rule_ids(&findings);

    // DET-03: new export detected
    assert!(
        rules.contains(&"export-added"),
        "should detect new export (DET-03), got: {:?}",
        rules
    );
    let added = findings_with_rule(&findings, "export-added");
    assert!(
        added.iter().any(|f| f.message.contains("newFeature")),
        "export-added should mention newFeature"
    );
    // OUT-05: new exports are Minor severity
    assert!(
        added.iter().all(|f| f.severity == Severity::Minor),
        "new exports should be Minor (OUT-05)"
    );

    // Note: SIG-01..05 detection depends on type shapes being available.
    // Without LSP enrichment, only structural export presence is detected.
    // Full SIG testing requires LSP enrichment (covered by lsp-tests feature gate).
    // Here we verify that the compare pipeline runs without error on the fixtures.
}
