use pledge::core::findings::Severity;
use pledge::ecosystems::node::compare::{compare_exports_map_diff, metadata::compare_metadata};
use serde_json::{json, Value};

fn package(engine: Option<&str>, source: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let mut manifest = json!({"type":"module", "exports":{"import":"./index.js"}});
    if let Some(engine) = engine {
        manifest["engines"] = json!({"node":engine});
    }
    std::fs::write(root.path().join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(root.path().join("index.js"), source).unwrap();
    root
}

#[test]
fn resource_syntax_rejects_explicitly_supported_older_node_versions() {
    for range in [">=22.12.0", "^22.12.0 || >=26", "22.12.0 - 24.0.0"] {
        for source in [
            "using resource=null; export const api=1;",
            "await using resource=null; export const api=1;",
            "export function api() { using resource=null; }",
        ] {
            let old = package(Some(range), "export const api=1;");
            let new = package(Some(range), source);
            let findings = compare_exports_map_diff(old.path(), new.path());
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.rule == "node-syntax-requirement-raised"
                        && finding.severity == Severity::Major),
                "{range} {source}: {findings:?}"
            );
        }
    }
}

#[test]
fn supported_or_unchanged_resource_syntax_does_not_raise_a_requirement() {
    for range in [">=24.0.0", "^24 || ^26", "~24.1.0"] {
        let old = package(Some(range), "export const api=1;");
        let new = package(Some(range), "using resource=null; export const api=1;");
        assert!(compare_exports_map_diff(old.path(), new.path()).is_empty());
    }
    let existing = package(
        Some(">=22.12.0"),
        "using resource=null; export const api=1;",
    );
    assert!(compare_exports_map_diff(existing.path(), existing.path()).is_empty());
}

#[test]
fn missing_or_unparseable_runtime_ranges_leave_syntax_compatibility_incomplete() {
    for range in [None, Some("not-a-node-range")] {
        let old = package(range, "export const api=1;");
        let new = package(range, "using resource=null; export const api=1;");
        let findings = compare_exports_map_diff(old.path(), new.path());
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"
                    && finding.message.contains("engines.node")),
            "{findings:?}"
        );
        assert!(!findings
            .iter()
            .any(|finding| finding.severity == Severity::Major));
    }
}

#[test]
fn manifest_engine_bumps_are_not_reported_again_as_syntax_bumps() {
    let old = package(Some(">=22.12.0"), "export const api=1;");
    let new = package(Some(">=24.0.0"), "using resource=null; export const api=1;");
    assert!(compare_exports_map_diff(old.path(), new.path()).is_empty());
    assert!(compare_metadata(old.path(), new.path())
        .iter()
        .any(|finding| finding.rule == "engine-version-bumped"));
}

#[test]
fn import_only_entry_points_follow_nested_runtime_dependencies_for_syntax() {
    let old = package(
        Some(">=22.12.0"),
        "import './middle.js'; export const api=1;",
    );
    let new = package(
        Some(">=22.12.0"),
        "import './middle.js'; export const api=1;",
    );
    for root in [&old, &new] {
        std::fs::write(root.path().join("middle.js"), "import './nested.js';").unwrap();
    }
    std::fs::write(old.path().join("nested.js"), "export function run() {}").unwrap();
    std::fs::write(
        new.path().join("nested.js"),
        "export function run() { using resource=null; }",
    )
    .unwrap();
    let findings = compare_exports_map_diff(old.path(), new.path());
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "node-syntax-requirement-raised"
                && finding.file.to_str() == Some("nested.js")),
        "{findings:?}"
    );
    assert!(!findings
        .iter()
        .any(|finding| finding.rule == "module-became-async"));
}

#[test]
fn dependencies_after_an_existing_top_level_await_are_still_checked_for_new_syntax() {
    let source = "await Promise.resolve(); import './dependency.js'; export const api=1;";
    let old = package(Some(">=22.12.0"), source);
    let new = package(Some(">=22.12.0"), source);
    std::fs::write(old.path().join("dependency.js"), "export function run() {}").unwrap();
    std::fs::write(
        new.path().join("dependency.js"),
        "export function run() { using resource=null; }",
    )
    .unwrap();
    let findings = compare_exports_map_diff(old.path(), new.path());
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "node-syntax-requirement-raised"
                && finding.file.to_str() == Some("dependency.js")),
        "{findings:?}"
    );
    assert!(!findings
        .iter()
        .any(|finding| finding.rule == "module-became-async"));
}

#[test]
fn compiler_companions_do_not_raise_runtime_syntax_requirements() {
    let old = package(Some(">=22.12.0"), "export const api=1;");
    let new = package(Some(">=22.12.0"), "export const api=1;");
    std::fs::write(old.path().join("index.ts"), "export const api=1;").unwrap();
    std::fs::write(
        new.path().join("index.ts"),
        "using resource=null; export const api=1;",
    )
    .unwrap();
    assert!(compare_exports_map_diff(old.path(), new.path()).is_empty());
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(new.path().join("package.json")).unwrap())
            .unwrap();
    manifest["exports"] = json!({"types":"./index.d.ts", "import":"./index.js"});
    std::fs::write(new.path().join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(
        new.path().join("index.d.ts"),
        "export declare const api: number;",
    )
    .unwrap();
    assert!(!compare_exports_map_diff(old.path(), new.path())
        .iter()
        .any(|finding| finding.rule == "node-syntax-requirement-raised"));
}
