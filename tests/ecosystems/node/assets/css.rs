//! Css compatibility checks.

use crate::fixtures::fixture_path;
use interlint::ecosystems::node::compare::assets::css as css_diff;
use interlint::ecosystems::node::compare::rules::{
    RULE_CSS_CLASS_REMOVED, RULE_CSS_CUSTOM_PROPERTY_REMOVED,
};

#[test]
fn test_css_custom_property_removed_integration() {
    let old = fixture_path("node/assets/css/old");
    let new = fixture_path("node/assets/css/new");
    let findings = css_diff::compare_css_files(&old, &new);
    // --color-primary removed -> AST-03
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_CSS_CUSTOM_PROPERTY_REMOVED),
        "Expected css-custom-property-removed finding, got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

#[test]
fn test_css_class_removed_integration() {
    let old = fixture_path("node/assets/css/old");
    let new = fixture_path("node/assets/css/new");
    let findings = css_diff::compare_css_files(&old, &new);
    // .card removed -> AST-04
    assert!(
        findings.iter().any(|f| f.rule == RULE_CSS_CLASS_REMOVED),
        "Expected css-class-removed finding, got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}
