//! Disabled targets compatibility checks.

use interlint::ecosystems::node::compare::metadata::compare_metadata;

#[test]
fn null_to_real_target_fires_minor_not_major() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let old_dir = dir.path().join("old");
    let new_dir = dir.path().join("new");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::create_dir_all(&new_dir).unwrap();

    std::fs::write(
        old_dir.join("package.json"),
        r#"{"name":"t","version":"1.0.0","exports":{".":{"types":null,"default":"./index.js"}}}"#,
    )
    .unwrap();
    std::fs::write(
        new_dir.join("package.json"),
        r#"{"name":"t","version":"1.0.0","exports":{".":{"types":"./index.d.ts","default":"./index.js"}}}"#,
    )
    .unwrap();

    let findings = compare_metadata(&old_dir, &new_dir);
    // null -> real should not fire Major condition-target-changed
    assert!(
        !findings.iter().any(|f| {
            f.rule == "condition-target-changed"
                && matches!(f.severity, interlint::core::findings::Severity::Major)
        }),
        "SEM-02: null -> real should not fire Major condition-target-changed. Got: {:?}",
        findings
    );
}

#[test]
fn invalid_export_targets_do_not_resolve_as_local_files() {
    for target in ["index.mjs", "./sub/../index.mjs"] {
        let package = tempfile::tempdir().unwrap();
        std::fs::create_dir(package.path().join("sub")).unwrap();
        std::fs::write(package.path().join("index.mjs"), "export const api = 1;").unwrap();
        std::fs::write(
            package.path().join("package.json"),
            format!(r#"{{"exports":"{target}"}}"#),
        )
        .unwrap();

        let (exports, warnings) =
            interlint::ecosystems::node::discovery::discover_exports(package.path()).unwrap();

        assert!(
            exports.is_empty(),
            "invalid target {target} was analyzed as a public API"
        );
        assert!(
            warnings.iter().any(|warning| warning.incomplete
                && warning.message.contains("invalid `exports` target")),
            "{warnings:?}"
        );
    }
}

#[test]
fn invalid_array_targets_allow_valid_fallbacks() {
    let package = tempfile::tempdir().unwrap();
    std::fs::write(
        package.path().join("ignored.mjs"),
        "export const ignored = 1;",
    )
    .unwrap();
    std::fs::write(package.path().join("index.mjs"), "export const api = 1;").unwrap();
    std::fs::write(
        package.path().join("package.json"),
        r#"{"exports":["ignored.mjs","./index.mjs"]}"#,
    )
    .unwrap();

    let (exports, warnings) =
        interlint::ecosystems::node::discovery::discover_exports(package.path()).unwrap();

    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports.keys().any(|key| key.name == "api"));
    assert!(!exports.keys().any(|key| key.name == "ignored"));
}

#[cfg(feature = "lsp-tests")]
#[test]
fn node_rejects_export_targets_without_a_relative_prefix() {
    let package = tempfile::tempdir().unwrap();
    std::fs::write(package.path().join("index.mjs"), "export const api = 1;").unwrap();
    let run = |target| {
        std::fs::write(
            package.path().join("package.json"),
            format!(r#"{{"name":"target-fixture","exports":"{target}"}}"#),
        )
        .unwrap();
        std::process::Command::new("node")
            .current_dir(package.path())
            .args(["-e", "require('target-fixture')"])
            .output()
            .unwrap()
    };
    assert!(run("./index.mjs").status.success());
    let result = run("index.mjs");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ERR_INVALID_PACKAGE_TARGET"));
}
