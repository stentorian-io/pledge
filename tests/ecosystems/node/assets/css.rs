//! Css compatibility checks.

use crate::fixtures::fixture_path;
use pledge::core::findings::Severity;
use pledge::ecosystems::node::compare::assets::css as css_diff;
use pledge::ecosystems::node::compare::rules::{
    RULE_CSS_CLASS_REMOVED, RULE_CSS_CUSTOM_PROPERTY_REMOVED,
};

fn compare_stylesheet_sources(old: &str, new: &str) -> Vec<pledge::core::findings::Finding> {
    let before = tempfile::tempdir().unwrap();
    let after = tempfile::tempdir().unwrap();
    std::fs::write(before.path().join("style.css"), old).unwrap();
    std::fs::write(after.path().join("style.css"), new).unwrap();
    css_diff::compare_css_files(before.path(), after.path())
}

#[test]
fn equivalent_css_identifier_escapes_preserve_classes_and_custom_properties() {
    for (old, new) in [
        (".api { --color: red; }", r".\61 pi { --\63 olor: red; }"),
        (".api {}", r".\000061pi {}"),
        (".api {}", r".a\pi {}"),
        (r".a\:b {}", r".a\3a b {}"),
        (r".\31 name {}", r".\000031name {}"),
        (".api {}", ".\\61\r\npi {}"),
    ] {
        for (old, new) in [(old, new), (new, old)] {
            let findings = compare_stylesheet_sources(old, new);
            assert!(findings.is_empty(), "{old:?} -> {new:?}: {findings:?}");
        }
    }
    let findings = compare_stylesheet_sources(r".\61 pi { --\63 olor: red; }", "");
    for rule in [RULE_CSS_CLASS_REMOVED, RULE_CSS_CUSTOM_PROPERTY_REMOVED] {
        assert!(
            findings.iter().any(|finding| finding.rule == rule),
            "{findings:?}"
        );
    }
}

#[test]
fn property_registrations_contribute_custom_property_contracts() {
    let registration =
        "@property --color { syntax: '<color>'; inherits: true; initial-value: red; }";
    let findings = compare_stylesheet_sources(registration, "");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].rule, RULE_CSS_CUSTOM_PROPERTY_REMOVED);
    assert!(findings[0].message.contains("--color"));
    assert!(compare_stylesheet_sources(registration, registration).is_empty());
    assert!(compare_stylesheet_sources(registration, ":root { --color: red; }").is_empty());
}

#[test]
fn css_recovery_trees_report_missing_evidence_without_inventing_removals() {
    for (old, new) in [
        (".api { color: red; }", ".api:not( { color: red; }"),
        (".api:not( { color: red; }", ""),
    ] {
        let findings = compare_stylesheet_sources(old, new);
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"),
            "{findings:?}"
        );
        assert!(
            !findings
                .iter()
                .any(|finding| finding.severity == Severity::Major),
            "{findings:?}"
        );
    }
}

#[test]
fn empty_required_pseudo_arguments_cannot_establish_stylesheet_compatibility() {
    for selector in [
        ":not()",
        ":has()",
        ":lang()",
        ":dir()",
        ":state()",
        ":host()",
        ":host-context()",
        ":nth-child()",
        ":nth-last-child()",
        ":nth-of-type()",
        ":nth-last-of-type()",
        "::part()",
        "::slotted()",
        ":not(/**/)",
        ":has( \t\n )",
        ":NOT()",
        ":dir(/**/)",
        ":STATE( )",
        ":host-context(/*c*/)",
        r":\6e ot()",
    ] {
        let source = format!(".api{selector} {{ color: red; }}");
        for (old, new) in [
            (".api { color: red; }", source.as_str()),
            (source.as_str(), ""),
        ] {
            let findings = compare_stylesheet_sources(old, new);
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.rule == "analysis-incomplete"),
                "{selector}: {findings:?}"
            );
            assert!(
                !findings
                    .iter()
                    .any(|finding| finding.severity == Severity::Major),
                "{selector}: {findings:?}"
            );
        }
    }
}

#[test]
fn forgiving_empty_pseudo_arguments_and_valid_arguments_remain_parseable() {
    for selector in [
        ":is()",
        ":where()",
        ":is(/**/)",
        ":where(/*c*/)",
        ":not(.other)",
        ":has(.child)",
        ":lang(en)",
        ":lang(\"\")",
        ":dir(ltr)",
        ":state(ready)",
        ":host(.api)",
        ":host-context(.theme)",
        ":host",
        ":nth-child(1)",
        ":nth-last-of-type(odd)",
        "::part(label)",
        "::slotted(.api)",
    ] {
        let source = format!(".api{selector} {{ color: red; }}");
        let findings = compare_stylesheet_sources(&source, &source);
        assert!(findings.is_empty(), "{selector}: {findings:?}");
    }
    assert!(compare_stylesheet_sources(".api { content: ':not()'; }", ".api {}").is_empty());
}

#[test]
fn manifest_stylesheets_cannot_read_outside_the_package() {
    let workspace = tempfile::tempdir().unwrap();
    let old = workspace.path().join("old");
    let new = workspace.path().join("new");
    std::fs::create_dir(&old).unwrap();
    std::fs::create_dir(&new).unwrap();
    let external = workspace.path().join("external.css");
    std::fs::write(&external, ".private-marker { --private-token: red; }").unwrap();
    std::fs::write(new.join("package.json"), r#"{"style":"empty.css"}"#).unwrap();
    std::fs::write(new.join("empty.css"), "").unwrap();

    for path in [
        external.to_string_lossy().into_owned(),
        "../external.css".into(),
    ] {
        std::fs::write(
            old.join("package.json"),
            serde_json::json!({"style":path}).to_string(),
        )
        .unwrap();

        let findings = css_diff::compare_css_files(&old, &new);

        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"),
            "{findings:?}"
        );
        assert!(
            !findings
                .iter()
                .any(|finding| finding.severity == Severity::Major),
            "{findings:?}"
        );
        assert!(
            !findings
                .iter()
                .any(|finding| finding.message.contains("private-marker")
                    || finding.message.contains("private-token")),
            "{findings:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn manifest_stylesheets_preserve_internal_symlinks_and_reject_external_ones() {
    let external = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(external.path(), ".private-marker {}").unwrap();
    let old = tempfile::tempdir().unwrap();
    let new = tempfile::tempdir().unwrap();
    for root in [old.path(), new.path()] {
        std::fs::write(root.join("package.json"), r#"{"style":"alias.css"}"#).unwrap();
    }
    std::fs::write(old.path().join("real.css"), ".api {}").unwrap();
    std::fs::write(new.path().join("real.css"), "").unwrap();
    for root in [old.path(), new.path()] {
        std::os::unix::fs::symlink("real.css", root.join("alias.css")).unwrap();
    }

    let internal = css_diff::compare_css_files(old.path(), new.path());

    assert!(internal
        .iter()
        .any(|finding| finding.rule == RULE_CSS_CLASS_REMOVED));
    std::fs::remove_file(old.path().join("alias.css")).unwrap();
    std::os::unix::fs::symlink(external.path(), old.path().join("alias.css")).unwrap();

    let outside = css_diff::compare_css_files(old.path(), new.path());

    assert!(
        outside
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete"),
        "{outside:?}"
    );
    assert!(
        !outside
            .iter()
            .any(|finding| finding.severity == Severity::Major),
        "{outside:?}"
    );
}

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
