use interlint::compare::metadata::compare_metadata;
use interlint::types::Severity;
use serde_json::{json, Value};

fn compare(old: Value, new: Value) -> Vec<interlint::types::Finding> {
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
        (">=1.0.0-beta <2", ">=0 <3", true),
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
