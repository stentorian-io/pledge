use interlint::core::findings::has_breaking_changes;
use interlint::ecosystems::node::compare::compare_exports_map_diff;
use interlint::ecosystems::node::discovery::discover_exports;
use interlint::languages::javascript::compare::compare_exports;
use serde_json::Value;

fn package(manifest: &str, a: &str, b: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let _: Value = serde_json::from_str(manifest).unwrap();
    std::fs::write(dir.path().join("package.json"), manifest).unwrap();
    std::fs::write(dir.path().join("a.ts"), a).unwrap();
    std::fs::write(dir.path().join("b.ts"), b).unwrap();
    dir
}

#[test]
fn compatible_target_relocation_and_unused_private_maps_are_not_breaking() {
    for (old_manifest, new_manifest) in [
        (r#"{"exports":"./a.ts"}"#, r#"{"exports":"./b.ts"}"#),
        (
            r##"{"exports":"./a.ts","imports":{"#unused":"./b.ts"}}"##,
            r#"{"exports":"./a.ts"}"#,
        ),
    ] {
        let old = package(
            old_manifest,
            "export const api = 1;",
            "export const api = 1;",
        );
        let new = package(
            new_manifest,
            "export const api = 1;",
            "export const api = 1;",
        );
        let (old_exports, _) = discover_exports(old.path()).unwrap();
        let (new_exports, _) = discover_exports(new.path()).unwrap();
        let mut findings = compare_exports(&old_exports, &new_exports, new.path(), false);
        findings.extend(compare_exports_map_diff(old.path(), new.path()));
        assert!(!has_breaking_changes(&findings), "{findings:?}");
    }
}

#[test]
fn condition_priority_compares_the_api_selected_for_overlapping_conditions() {
    let before = r#"{"exports":{"node":"./a.ts","import":"./b.ts"}}"#;
    let after = r#"{"exports":{"import":"./b.ts","node":"./a.ts"}}"#;
    for (b, breaking) in [("export {};", true), ("export const api = 1;", false)] {
        let old = package(before, "export const api = 1;", b);
        let new = package(after, "export const api = 1;", b);
        let findings = compare_exports_map_diff(old.path(), new.path());
        assert_eq!(has_breaking_changes(&findings), breaking, "{findings:?}");
        assert!(findings
            .iter()
            .any(|f| f.rule == "condition-priority-changed"));
    }
}

#[test]
fn mutually_exclusive_import_and_require_order_is_compatible() {
    let old = package(
        r#"{"exports":{"import":"./a.ts","require":"./b.ts"}}"#,
        "export const api = 1;",
        "export {};",
    );
    let new = package(
        r#"{"exports":{"require":"./b.ts","import":"./a.ts"}}"#,
        "export const api = 1;",
        "export {};",
    );
    assert!(compare_exports_map_diff(old.path(), new.path()).is_empty());
}

#[test]
fn private_import_conditions_follow_public_resolution_context() {
    let dir = package(
        r##"{"exports":{"import":"./a.ts","require":"./b.ts"},"imports":{"#private":{"import":"./import.ts","require":"./require.ts"}}}"##,
        "export * from '#private';",
        "export * from '#private';",
    );
    std::fs::write(dir.path().join("import.ts"), "export const imported = 1;").unwrap();
    std::fs::write(dir.path().join("require.ts"), "export const required = 1;").unwrap();
    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports
        .keys()
        .any(|k| k.condition.as_deref() == Some("import") && k.name == "imported"));
    assert!(exports
        .keys()
        .any(|k| k.condition.as_deref() == Some("require") && k.name == "required"));
    assert!(!exports
        .keys()
        .any(|k| k.condition.as_deref() == Some("require") && k.name == "imported"));
}
