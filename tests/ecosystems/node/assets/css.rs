//! Css compatibility checks.

use crate::fixtures::fixture_path;
use interlint::core::findings::Severity;
use interlint::ecosystems::node::compare::assets::css as css_diff;
use interlint::ecosystems::node::compare::rules::{
    RULE_CSS_CLASS_REMOVED, RULE_CSS_CUSTOM_PROPERTY_REMOVED,
};

#[test]
fn equivalent_stylesheet_paths_still_compare_classes_and_properties() {
    let old = tempfile::tempdir().unwrap();
    let new = tempfile::tempdir().unwrap();
    std::fs::write(
        old.path().join("package.json"),
        r#"{"style":"./style.css"}"#,
    )
    .unwrap();
    std::fs::write(new.path().join("package.json"), r#"{"style":"style.css"}"#).unwrap();
    std::fs::write(old.path().join("style.css"), ".api { --color: red; }").unwrap();
    std::fs::write(new.path().join("style.css"), "").unwrap();

    let findings = css_diff::compare_css_files(old.path(), new.path());

    for rule in [RULE_CSS_CLASS_REMOVED, RULE_CSS_CUSTOM_PROPERTY_REMOVED] {
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == rule && finding.severity == Severity::Major),
            "{findings:?}"
        );
    }
}

#[test]
fn stylesheet_redirects_compare_the_selected_api() {
    for (replacement, breaking) in [("", true), (".api { --color: red; }", false)] {
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        for directory in [&old, &new] {
            std::fs::write(directory.path().join("a.css"), ".api { --color: red; }").unwrap();
            std::fs::write(directory.path().join("b.css"), replacement).unwrap();
        }
        std::fs::write(old.path().join("package.json"), r#"{"style":"./a.css"}"#).unwrap();
        std::fs::write(new.path().join("package.json"), r#"{"style":"./b.css"}"#).unwrap();

        let findings = css_diff::compare_css_files(old.path(), new.path());

        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.severity == Severity::Major),
            breaking,
            "{findings:?}"
        );
    }
}

#[test]
fn selected_stylesheets_keep_independent_contracts() {
    let old = tempfile::tempdir().unwrap();
    let new = tempfile::tempdir().unwrap();
    for directory in [&old, &new] {
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"style":["a.css","b.css"]}"#,
        )
        .unwrap();
        std::fs::write(directory.path().join("b.css"), ".api { --color: red; }").unwrap();
    }
    std::fs::write(old.path().join("a.css"), ".api { --color: red; }").unwrap();
    std::fs::write(new.path().join("a.css"), "").unwrap();

    let findings = css_diff::compare_css_files(old.path(), new.path());

    for rule in [RULE_CSS_CLASS_REMOVED, RULE_CSS_CUSTOM_PROPERTY_REMOVED] {
        assert!(
            findings.iter().any(|finding| {
                finding.rule == rule && finding.file == std::path::Path::new("a.css")
            }),
            "{findings:?}"
        );
    }
}

#[test]
fn missing_stylesheets_do_not_hide_readable_evidence() {
    let old = tempfile::tempdir().unwrap();
    let new = tempfile::tempdir().unwrap();
    for directory in [&old, &new] {
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"style":["main.css","missing.css"]}"#,
        )
        .unwrap();
    }
    std::fs::write(old.path().join("main.css"), ".api {}").unwrap();
    std::fs::write(new.path().join("main.css"), "").unwrap();

    let findings = css_diff::compare_css_files(old.path(), new.path());

    assert!(
        findings.iter().any(|finding| {
            finding.rule == RULE_CSS_CLASS_REMOVED
                && finding.file == std::path::Path::new("main.css")
        }),
        "{findings:?}"
    );
    assert!(
        findings.iter().any(|finding| {
            finding.rule == "analysis-incomplete"
                && finding.file == std::path::Path::new("missing.css")
        }),
        "{findings:?}"
    );
}

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
