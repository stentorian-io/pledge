//! Files compatibility checks.

use crate::fixtures::fixture_path;
#[cfg(unix)]
use pledge::core::findings::Severity;
use pledge::ecosystems::node::compare::assets::files as asset_diff;
use pledge::ecosystems::node::compare::rules::{
    RULE_ASSET_FILE_REMOVED, RULE_CSS_CLASS_REMOVED, RULE_JSON_FIELD_REMOVED,
};

#[test]
fn test_asset_file_removed_integration() {
    let old = fixture_path("node/assets/files/old");
    let new = fixture_path("node/assets/files/new");
    let findings = asset_diff::compare_asset_files(&old, &new);
    // icon.png removed -> AST-05
    assert!(
        findings.iter().any(|f| f.rule == RULE_ASSET_FILE_REMOVED),
        "Expected asset-file-removed finding, got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

#[cfg(unix)]
#[test]
fn directory_cycles_do_not_duplicate_removed_assets() {
    let old = tempfile::tempdir().unwrap();
    let new = tempfile::tempdir().unwrap();
    for root in [old.path(), new.path()] {
        std::fs::write(
            root.join("package.json"),
            r#"{"name":"cycle","main":"index.js"}"#,
        )
        .unwrap();
        std::fs::write(root.join("index.js"), "exports.api = 1;").unwrap();
        std::os::unix::fs::symlink(".", root.join("again")).unwrap();
    }
    std::fs::write(old.path().join("icon.svg"), "<svg/>").unwrap();

    let mut findings =
        pledge::ecosystems::node::compare::assets::compare_asset_diff(old.path(), new.path());
    findings.extend(pledge::ecosystems::node::compare::compare_exports_map_diff(
        old.path(),
        new.path(),
    ));

    let removed: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule == RULE_ASSET_FILE_REMOVED)
        .collect();
    assert_eq!(removed.len(), 1, "{removed:?}");
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete"
                && finding.message.contains("cycle")),
        "{findings:?}"
    );
}

#[cfg(unix)]
#[test]
fn directory_aliases_keep_their_paths_but_external_links_are_incomplete() {
    let old = tempfile::tempdir().unwrap();
    let new = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    for root in [old.path(), new.path()] {
        std::fs::create_dir(root.join("icons")).unwrap();
        std::os::unix::fs::symlink("icons", root.join("alias")).unwrap();
        std::os::unix::fs::symlink(external.path(), root.join("outside")).unwrap();
    }
    std::fs::write(old.path().join("icons/icon.svg"), "<svg/>").unwrap();

    let findings = asset_diff::compare_asset_files(old.path(), new.path());

    let removed: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule == RULE_ASSET_FILE_REMOVED)
        .collect();
    assert_eq!(removed.len(), 2, "{removed:?}");
    assert!(removed
        .iter()
        .any(|finding| finding.message.contains("icons/icon.svg")));
    assert!(removed
        .iter()
        .any(|finding| finding.message.contains("alias/icon.svg")));
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete"
                && finding.severity == Severity::Warning),
        "{findings:?}"
    );
}

#[test]
fn test_orchestrator_combines_all_findings() {
    // Use tempdir with JSON + CSS + asset changes to verify compare_asset_diff
    let dir_old = tempfile::tempdir().unwrap();
    let dir_new = tempfile::tempdir().unwrap();
    // package.json
    std::fs::write(
        dir_old.path().join("package.json"),
        r#"{"name":"x","version":"1.0.0"}"#,
    )
    .unwrap();
    std::fs::write(
        dir_new.path().join("package.json"),
        r#"{"name":"x","version":"2.0.0"}"#,
    )
    .unwrap();
    // JSON change (field removed)
    std::fs::write(dir_old.path().join("data.json"), r#"{"a":1,"b":2}"#).unwrap();
    std::fs::write(dir_new.path().join("data.json"), r#"{"a":1}"#).unwrap();
    // CSS change (class removed)
    std::fs::write(dir_old.path().join("style.css"), ".foo { } .bar { }").unwrap();
    std::fs::write(dir_new.path().join("style.css"), ".foo { }").unwrap();
    // Asset removal
    std::fs::write(dir_old.path().join("logo.svg"), "<svg></svg>").unwrap();

    let findings = pledge::ecosystems::node::compare::assets::compare_asset_diff(
        dir_old.path(),
        dir_new.path(),
    );

    let rules: Vec<&str> = findings.iter().map(|f| f.rule).collect();
    assert!(
        rules.contains(&RULE_JSON_FIELD_REMOVED),
        "Missing json-field-removed"
    );
    assert!(
        rules.contains(&RULE_CSS_CLASS_REMOVED),
        "Missing css-class-removed"
    );
    assert!(
        rules.contains(&RULE_ASSET_FILE_REMOVED),
        "Missing asset-file-removed"
    );
}
