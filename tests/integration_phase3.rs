//! Integration tests for Phase 3 detection rules.
//! Tests the compare pipeline with fixture packages.

use interlint::compare;
use interlint::compare::metadata;
use interlint::discover;
use interlint::types::Severity;
use std::path::PathBuf;

/// Helper: resolve a fixture path relative to the crate root.
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Helper: discover exports from a fixture directory.
fn discover_fixture(path: &PathBuf) -> interlint::types::ExportMap {
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
    let old_path = fixture_path("sig-change-pkg/old");
    let new_path = fixture_path("sig-change-pkg/new");

    let old_exports = discover_fixture(&old_path);
    let new_exports = discover_fixture(&new_path);
    let findings = compare::compare_exports(&old_exports, &new_exports, std::path::Path::new("."), true);

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

/// META-01..07: metadata changes detected from package.json differences.
///
/// meta-change-pkg tests peer dep bumps/adds, engine bumps/adds,
/// entry point changes, and module type changes.
#[test]
fn test_metadata_changes_detected() {
    let old_path = fixture_path("meta-change-pkg/old");
    let new_path = fixture_path("meta-change-pkg/new");

    let findings = metadata::compare_metadata(&old_path, &new_path);
    let rules = rule_ids(&findings);

    // META-01: react peer dep bumped
    assert!(
        rules.contains(&"peer-dep-version-bumped"),
        "should detect peer dep bump (META-01), got: {:?}",
        rules
    );
    let bumped = findings_with_rule(&findings, "peer-dep-version-bumped");
    assert!(bumped[0].message.contains("react"), "should mention react");
    assert!(
        bumped[0].message.contains("^17.0.0"),
        "should mention old version"
    );
    assert!(
        bumped[0].message.contains("^18.0.0"),
        "should mention new version"
    );

    // META-02: vue peer dep added
    assert!(
        rules.contains(&"peer-dep-added"),
        "should detect added peer dep (META-02), got: {:?}",
        rules
    );
    let added = findings_with_rule(&findings, "peer-dep-added");
    assert!(added[0].message.contains("vue"), "should mention vue");

    // META-04: node engine bumped
    assert!(
        rules.contains(&"engine-version-bumped"),
        "should detect engine bump (META-04), got: {:?}",
        rules
    );

    // META-05: npm engine added
    assert!(
        rules.contains(&"engine-added"),
        "should detect engine added (META-05), got: {:?}",
        rules
    );

    // META-06: main entry point changed
    assert!(
        rules.contains(&"entry-point-changed"),
        "should detect entry point change (META-06), got: {:?}",
        rules
    );
    let ep = findings_with_rule(&findings, "entry-point-changed");
    assert!(ep[0].message.contains("main"), "should mention main field");

    // META-07: module type changed
    assert!(
        rules.contains(&"module-type-changed"),
        "should detect module type change (META-07), got: {:?}",
        rules
    );

    // D-08: All findings use package.json path
    for finding in &findings {
        assert_eq!(
            finding.file.to_str().unwrap(),
            "package.json",
            "metadata findings should use package.json path (D-08)"
        );
        assert_eq!(finding.line, 1);
        assert_eq!(finding.col, 1);
    }

    // All metadata findings should be Major
    assert!(
        findings.iter().all(|f| f.severity == Severity::Major),
        "all metadata changes should be Major severity"
    );
}

/// Verify that comparing the same metadata produces no findings.
#[test]
fn test_metadata_no_changes() {
    let path = fixture_path("meta-change-pkg/old");
    let findings = metadata::compare_metadata(&path, &path);
    assert!(
        findings.is_empty(),
        "same metadata should produce no findings"
    );
}

/// Verify that metadata Major findings register as breaking changes.
#[test]
fn test_has_breaking_changes_includes_metadata() {
    let old_path = fixture_path("meta-change-pkg/old");
    let new_path = fixture_path("meta-change-pkg/new");

    let findings = metadata::compare_metadata(&old_path, &new_path);
    assert!(
        compare::has_breaking_changes(&findings),
        "metadata Major findings should count as breaking changes"
    );
}
