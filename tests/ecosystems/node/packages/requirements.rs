use pledge::core::findings::Severity;
use pledge::ecosystems::node::compare::metadata::compare_metadata;
use serde_json::{json, Value};

fn compare(old: Value, new: Value) -> Vec<pledge::core::findings::Finding> {
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();
    std::fs::write(old_dir.path().join("package.json"), old.to_string()).unwrap();
    std::fs::write(new_dir.path().join("package.json"), new.to_string()).unwrap();
    compare_metadata(old_dir.path(), new_dir.path())
}

#[test]
fn version_requirements_compare_the_entire_accepted_set() {
    for (old, new, narrowed) in [
        (">=18 <21", ">=18 <20", true),
        ("^18 || ^20 || ^22", "^18 || ^20", true),
        ("^18", "^17", true),
        ("^18 || ^20", "^20 || ^18", false),
        ("^18 || ^20", ">=18", false),
        (">=18 <20", ">=18 <19 || >=19 <20", false),
        ("^18.0.0", "^18.1.0", true),
        ("*", "^18", true),
        (">=1 <2", ">=1 <2.0.0-0", false),
        (">=1.0.0-beta <2", ">=1.0.0-alpha <3", false),
        ("^1.0.0-beta", "^1.0.0-rc", true),
        (">1.0.0-beta <1.0.0", ">=1.0.0-beta.0 <1.0.0", false),
    ] {
        for (field, name, rule) in [
            ("peerDependencies", "host", "peer-dep-version-bumped"),
            ("engines", "node", "engine-version-bumped"),
        ] {
            let findings = compare(json!({field: {name: old}}), json!({field: {name: new}}));
            assert_eq!(
                findings
                    .iter()
                    .any(|f| f.rule == rule && f.severity == Severity::Major),
                narrowed,
                "{field}: {old} -> {new}: {findings:?}"
            );
        }
    }
}

#[test]
fn platform_exclusions_and_unrestricted_support_are_respected() {
    for field in ["os", "cpu"] {
        for (old, new, narrowed) in [
            (json!(["!win32"]), json!(["!win32", "!darwin"]), true),
            (json!(["darwin"]), json!([]), false),
            (json!(["!win32"]), json!([]), false),
            (json!([]), json!(["!win32"]), true),
            (
                json!(["darwin", "linux"]),
                json!(["darwin", "linux", "!darwin"]),
                true,
            ),
            (json!(["linux"]), json!(["!win32"]), false),
        ] {
            let findings = compare(json!({field: old}), json!({field: new}));
            assert_eq!(
                findings.iter().any(|f| f.rule == "platform-narrowed"),
                narrowed,
                "{field}: {old} -> {new}: {findings:?}"
            );
        }
    }
}

#[test]
fn engine_ranges_include_prereleases_without_dependency_core_opt_in() {
    for (old, new, narrowed) in [
        (">=18.0.0 <20.0.0", ">=18.0.0 <20.0.0-0", true),
        (">=18.0.0 <20.0.0-0", ">=18.0.0 <20.0.0", false),
        (">=1.0.0-beta <2", ">=0 <3", false),
    ] {
        let findings = compare(
            json!({"engines":{"node":old}}),
            json!({"engines":{"node":new}}),
        );
        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.rule == "engine-version-bumped"),
            narrowed,
            "{findings:?}"
        );
    }
    let peer_changes = compare(
        json!({"peerDependencies":{"host":">=18.0.0 <20.0.0"}}),
        json!({"peerDependencies":{"host":">=18.0.0 <20.0.0-0"}}),
    );
    assert!(peer_changes.is_empty());
    let dropped_peer_prereleases = compare(
        json!({"peerDependencies":{"host":">=1.0.0-beta <2"}}),
        json!({"peerDependencies":{"host":">=0 <3"}}),
    );
    assert!(dropped_peer_prereleases
        .iter()
        .any(|finding| finding.rule == "peer-dep-version-bumped"));
}

#[test]
fn engine_shorthands_preserve_npm_prerelease_boundaries() {
    for (shorthand, expanded) in [
        (">=1 <2", ">=1.0.0-0 <2.0.0-0"),
        ("^18", ">=18.0.0-0 <19.0.0-0"),
        ("^18.0.0", ">=18.0.0 <19.0.0-0"),
        ("1.x", ">=1.0.0-0 <2.0.0-0"),
        ("~1.0", ">=1.0.0-0 <1.1.0-0"),
        ("1.2.3 - 2.3.4", ">=1.2.3-0 <2.3.5-0"),
        ("> 1.2", ">=1.3.0-0"),
        ("^0.0.1", ">=0.0.1 <0.0.2-0"),
        ("^1.2.3 <2.0.0", ">=1.2.3 <2.0.0-0"),
    ] {
        for (old, new) in [(shorthand, expanded), (expanded, shorthand)] {
            let changes = compare(
                json!({"engines":{"node":old}}),
                json!({"engines":{"node":new}}),
            );
            assert!(changes.is_empty(), "{old} -> {new}: {changes:?}");
        }
    }
}

#[test]
fn any_is_unrestricted_only_as_the_entire_platform_list() {
    for field in ["os", "cpu"] {
        let changes = compare(json!({field:["darwin"]}), json!({field:["any", "linux"]}));
        assert!(changes.iter().any(
            |finding| finding.rule == "platform-narrowed" && finding.message.contains("darwin")
        ));
        assert!(compare(json!({field:["darwin"]}), json!({field:["any"]})).is_empty());
    }
}

#[test]
fn scoped_bin_shorthand_and_unscoped_command_object_are_equivalent() {
    let old = json!({"name":"@audit/tool", "bin":"./cli.js"});
    let same = json!({"name":"@audit/tool", "bin":{"tool":"./cli.js"}});
    let removed = json!({"name":"@audit/tool", "bin":{"other":"./cli.js"}});
    assert!(compare(old.clone(), same).is_empty());
    assert!(compare(old, removed)
        .iter()
        .any(|finding| finding.rule == "bin-entry-changed" && finding.message.contains("tool")));
}

#[test]
fn npm_scalar_platform_requirements_and_libc_constraints_are_preserved() {
    for field in ["os", "cpu", "libc"] {
        for (old, new, narrowed) in [
            (json!({}), json!({field:"linux"}), true),
            (json!({field:["linux"]}), json!({field:"linux"}), false),
            (json!({field:"linux"}), json!({field:["linux"]}), false),
            (json!({field:"any"}), json!({field:["any"]}), false),
            (json!({field:"linux"}), json!({}), false),
            (json!({}), json!({field:"!linux"}), true),
        ] {
            let findings = compare(old.clone(), new.clone());
            assert_eq!(
                findings
                    .iter()
                    .any(|finding| finding.rule == "platform-narrowed"
                        && finding.severity == Severity::Major),
                narrowed,
                "{old} -> {new}: {findings:?}"
            );
        }
    }
    for (old, new, narrowed) in [
        (json!({}), json!({"libc":["glibc"]}), true),
        (
            json!({"libc":["glibc", "musl"]}),
            json!({"libc":["glibc"]}),
            true,
        ),
        (json!({}), json!({"libc":[]}), true),
        (json!({}), json!({"libc":"any"}), true),
        (json!({"libc":[]}), json!({"libc":["any"]}), false),
        (json!({"libc":["glibc"]}), json!({}), false),
    ] {
        let findings = compare(old.clone(), new.clone());
        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.rule == "platform-narrowed"),
            narrowed,
            "{old} -> {new}: {findings:?}"
        );
    }
}
