//! Fallback arrays compatibility checks.

#[test]
fn exports_array_does_not_retry_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"exports":["./missing.js","./existing.js"]}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("existing.js"), "export const api = 1;").unwrap();
    let (exports, warnings) =
        interlint::ecosystems::node::discovery::discover_exports(dir.path()).unwrap();
    assert!(exports.is_empty());
    assert!(warnings
        .iter()
        .any(|w| w.incomplete && w.message.contains("missing.js")));
}

fn package(exports: &str, fallback: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        format!(r#"{{"name":"array-fixture","type":"module","exports":{exports}}}"#),
    )
    .unwrap();
    std::fs::write(dir.path().join("browser.mjs"), "export const browser = 1;").unwrap();
    std::fs::write(dir.path().join("fallback.mjs"), fallback).unwrap();
    dir
}

#[test]
fn unmatched_conditions_preserve_the_public_array_fallback() {
    let manifest = r#"[{"browser":"./browser.mjs"},"./fallback.mjs"]"#;
    let old = package(manifest, "export function api() {}");
    let new = package(manifest, "export {};");
    let (old_exports, old_warnings) =
        interlint::ecosystems::node::discovery::discover_exports(old.path()).unwrap();
    let (new_exports, new_warnings) =
        interlint::ecosystems::node::discovery::discover_exports(new.path()).unwrap();

    let findings = interlint::languages::javascript::compare::compare_exports(
        &old_exports,
        &new_exports,
        new.path(),
        false,
    );

    assert!(old_warnings.is_empty() && new_warnings.is_empty());
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "export-removed" && finding.message.contains("api")),
        "{findings:?}"
    );
    assert!(
        interlint::ecosystems::node::compare::compare_exports_map_diff(old.path(), old.path())
            .is_empty()
    );
}

#[test]
fn null_array_entries_allow_later_targets_but_successful_defaults_stop_fallback() {
    for exports in [
        r#"[null,"./fallback.mjs"]"#,
        r#"{"node":[{"browser":"./browser.mjs"},"./fallback.mjs"]}"#,
        r#"[{"default":null},"./fallback.mjs"]"#,
        r#"[{"node":null,"default":"./browser.mjs"},"./fallback.mjs"]"#,
    ] {
        let dir = package(exports, "export const api = 1;");
        let (exports, warnings) =
            interlint::ecosystems::node::discovery::discover_exports(dir.path()).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(exports.keys().any(|key| key.name == "api"));
    }
    let dir = package(
        r#"[{"default":"./browser.mjs"},"./fallback.mjs"]"#,
        "export const unreachable = 1;",
    );
    let (exports, warnings) =
        interlint::ecosystems::node::discovery::discover_exports(dir.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(!exports.keys().any(|key| key.name == "unreachable"));
}

#[test]
fn private_import_arrays_resolve_fallbacks_after_unmatched_conditions() {
    let dir = package(r#""./index.mjs""#, "export const api = 1;");
    std::fs::write(dir.path().join("index.mjs"), "export * from '#api';").unwrap();
    std::fs::write(dir.path().join("package.json"), r##"{"exports":"./index.mjs","imports":{"#api":[{"browser":"./browser.mjs"},"./fallback.mjs"]}}"##).unwrap();

    let (exports, warnings) =
        interlint::ecosystems::node::discovery::discover_exports(dir.path()).unwrap();

    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports.keys().any(|key| key.name == "api"));
}

#[cfg(feature = "lsp-tests")]
#[test]
fn node_consumer_uses_the_unmatched_condition_array_fallback() {
    let manifest = r#"[{"browser":"./browser.mjs"},"./fallback.mjs"]"#;
    let old = package(manifest, "export function api() {}");
    let new = package(manifest, "export {};");
    let run = |root: &std::path::Path| {
        std::process::Command::new("node")
            .current_dir(root)
            .args(["-e", "require('array-fixture').api()"])
            .output()
            .unwrap()
    };
    assert!(run(old.path()).status.success());
    let result = run(new.path());
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("TypeError"));
}

#[test]
fn large_fallback_condition_sets_report_incomplete_analysis() {
    let dir = package(
        r#"[{"a":"./browser.mjs","b":"./browser.mjs","c":"./browser.mjs","d":"./browser.mjs","e":"./browser.mjs","f":"./browser.mjs","g":"./browser.mjs","h":"./browser.mjs"},"./fallback.mjs"]"#,
        "export const api = 1;",
    );
    let (_, warnings) =
        interlint::ecosystems::node::discovery::discover_exports(dir.path()).unwrap();
    assert!(warnings.iter().any(|warning| warning.incomplete
        && warning
            .message
            .contains("fallback condition analysis limit")));
}
