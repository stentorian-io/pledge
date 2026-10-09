//! Metadata compatibility checks.

use crate::fixtures::fixture_path;
use interlint::core::findings::Severity;
use interlint::ecosystems::node::compare::metadata;

/// Helper: collect rule IDs from findings.
fn rule_ids(findings: &[interlint::core::findings::Finding]) -> Vec<&'static str> {
    findings.iter().map(|f| f.rule).collect()
}

/// Helper: filter findings by rule.
fn findings_with_rule<'a>(
    findings: &'a [interlint::core::findings::Finding],
    rule: &str,
) -> Vec<&'a interlint::core::findings::Finding> {
    findings.iter().filter(|f| f.rule == rule).collect()
}

/// META-01..07: metadata changes detected from package.json differences.
///
/// meta-change-pkg tests peer dep bumps/adds, engine bumps/adds,
/// entry point changes, and module type changes.
#[test]
fn test_metadata_changes_detected() {
    let old_path = fixture_path("node/packages/metadata/old");
    let new_path = fixture_path("node/packages/metadata/new");

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

    // All findings use package.json path
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
    let path = fixture_path("node/packages/metadata/old");
    let findings = metadata::compare_metadata(&path, &path);
    assert!(
        findings.is_empty(),
        "same metadata should produce no findings"
    );
}

/// Verify that metadata Major findings register as breaking changes.
#[test]
fn test_has_breaking_changes_includes_metadata() {
    let old_path = fixture_path("node/packages/metadata/old");
    let new_path = fixture_path("node/packages/metadata/new");

    let findings = metadata::compare_metadata(&old_path, &new_path);
    assert!(
        interlint::core::findings::has_breaking_changes(&findings),
        "metadata Major findings should count as breaking changes"
    );
}
