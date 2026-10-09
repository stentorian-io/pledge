use crate::ecosystems::node::compare::metadata::*;
use std::path::PathBuf;
use tempfile::tempdir;

fn make_pkg(json_str: &str) -> serde_json::Value {
    serde_json::from_str(json_str).unwrap()
}

// -------------------------------------------------------------------------
// META-01: peer-dep-version-bumped
// -------------------------------------------------------------------------

#[test]
fn test_peer_dep_version_bumped() {
    let old = make_pkg(r#"{"peerDependencies": {"react": "^17.0.0"}}"#);
    let new = make_pkg(r#"{"peerDependencies": {"react": "^18.0.0"}}"#);
    let mut findings = Vec::new();
    compare_peer_deps(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_PEER_DEP_BUMPED);
    assert_eq!(findings[0].severity, Severity::Major);
}

#[test]
fn test_peer_dep_same_version_no_finding() {
    let old = make_pkg(r#"{"peerDependencies": {"react": "^17.0.0"}}"#);
    let new = make_pkg(r#"{"peerDependencies": {"react": "^17.0.0"}}"#);
    let mut findings = Vec::new();
    compare_peer_deps(&old, &new, &mut findings);
    assert_eq!(findings.len(), 0);
}

// -------------------------------------------------------------------------
// META-02: peer-dep-added
// -------------------------------------------------------------------------

#[test]
fn test_peer_dep_added() {
    let old = make_pkg(r#"{}"#);
    let new = make_pkg(r#"{"peerDependencies": {"react": "^18.0.0"}}"#);
    let mut findings = Vec::new();
    compare_peer_deps(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_PEER_DEP_ADDED);
    assert_eq!(findings[0].severity, Severity::Major);
}

// -------------------------------------------------------------------------
// META-03: peer-dep-removed
// -------------------------------------------------------------------------

#[test]
fn test_peer_dep_removed() {
    let old = make_pkg(r#"{"peerDependencies": {"react": "^17.0.0"}}"#);
    let new = make_pkg(r#"{}"#);
    let mut findings = Vec::new();
    compare_peer_deps(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_PEER_DEP_REMOVED);
    assert_eq!(findings[0].severity, Severity::Major);
}

// -------------------------------------------------------------------------
// META-04: engine-version-bumped
// -------------------------------------------------------------------------

#[test]
fn test_engine_version_bumped() {
    let old = make_pkg(r#"{"engines": {"node": ">=14"}}"#);
    let new = make_pkg(r#"{"engines": {"node": ">=18"}}"#);
    let mut findings = Vec::new();
    compare_engines(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_ENGINE_BUMPED);
    assert_eq!(findings[0].severity, Severity::Major);
}

// -------------------------------------------------------------------------
// META-05: engine-added
// -------------------------------------------------------------------------

#[test]
fn test_engine_added() {
    let old = make_pkg(r#"{}"#);
    let new = make_pkg(r#"{"engines": {"node": ">=18"}}"#);
    let mut findings = Vec::new();
    compare_engines(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_ENGINE_ADDED);
    assert_eq!(findings[0].severity, Severity::Major);
}

// -------------------------------------------------------------------------
// META-06: entry-point-changed
// -------------------------------------------------------------------------

#[test]
fn test_entry_point_main_changed() {
    let old = make_pkg(r#"{"main": "./lib/index.js"}"#);
    let new = make_pkg(r#"{"main": "./dist/index.js"}"#);
    let mut findings = Vec::new();
    compare_entry_points(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_ENTRY_POINT_CHANGED);
    assert_eq!(findings[0].severity, Severity::Major);
}

#[test]
fn test_entry_point_main_removed() {
    let old = make_pkg(r#"{"main": "./lib/index.js"}"#);
    let new = make_pkg(r#"{}"#);
    let mut findings = Vec::new();
    compare_entry_points(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_ENTRY_POINT_CHANGED);
    assert_eq!(findings[0].severity, Severity::Major);
}

#[test]
fn test_exports_field_changed_no_longer_triggers_entry_point_changed() {
    // exports changes are handled by exports_diff.rs, not compare_entry_points
    let old = make_pkg(r#"{"exports": {".": "./src/index.js"}}"#);
    let new = make_pkg(r#"{"exports": {".": "./dist/index.js"}}"#);
    let mut findings = Vec::new();
    compare_entry_points(&old, &new, &mut findings);
    assert_eq!(
        findings.len(),
        0,
        "exports changes should NOT produce entry-point-changed after D-07 removal"
    );
}

#[test]
fn test_entry_point_main_still_works_after_d07() {
    let old = make_pkg(r#"{"main": "./lib/index.js", "exports": {".": "./src/index.js"}}"#);
    let new = make_pkg(r#"{"main": "./dist/index.js", "exports": {".": "./dist/index.js"}}"#);
    let mut findings = Vec::new();
    compare_entry_points(&old, &new, &mut findings);
    assert_eq!(
        findings.len(),
        1,
        "should detect main change but NOT exports change"
    );
    assert_eq!(findings[0].rule, RULE_ENTRY_POINT_CHANGED);
    assert!(findings[0].message.contains("main"));
}

// -------------------------------------------------------------------------
// META-07: module-type-changed
// -------------------------------------------------------------------------

#[test]
fn test_module_type_changed() {
    let old = make_pkg(r#"{"type": "commonjs"}"#);
    let new = make_pkg(r#"{"type": "module"}"#);
    let mut findings = Vec::new();
    compare_module_type(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_MODULE_TYPE_CHANGED);
    assert_eq!(findings[0].severity, Severity::Major);
}

#[test]
fn test_module_type_absent_to_module() {
    // Default is "commonjs", so absent -> "module" is a breaking change
    let old = make_pkg(r#"{}"#);
    let new = make_pkg(r#"{"type": "module"}"#);
    let mut findings = Vec::new();
    compare_module_type(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_MODULE_TYPE_CHANGED);
    assert_eq!(findings[0].severity, Severity::Major);
}

#[test]
fn test_module_type_same_no_finding() {
    let old = make_pkg(r#"{"type": "module"}"#);
    let new = make_pkg(r#"{"type": "module"}"#);
    let mut findings = Vec::new();
    compare_module_type(&old, &new, &mut findings);
    assert_eq!(findings.len(), 0);
}

// -------------------------------------------------------------------------
// all findings use package.json path, line 1, col 1
// -------------------------------------------------------------------------

#[test]
fn test_all_findings_use_package_json_path() {
    let old = make_pkg(
        r#"{"peerDependencies": {"react": "^17.0.0"}, "main": "./lib/index.js", "type": "commonjs"}"#,
    );
    let new = make_pkg(
        r#"{"peerDependencies": {"react": "^18.0.0"}, "main": "./dist/index.js", "type": "module"}"#,
    );
    let mut findings = Vec::new();
    compare_peer_deps(&old, &new, &mut findings);
    compare_entry_points(&old, &new, &mut findings);
    compare_module_type(&old, &new, &mut findings);

    assert!(!findings.is_empty(), "expected at least one finding");
    for f in &findings {
        assert_eq!(
            f.file,
            PathBuf::from("package.json"),
            "expected file to be package.json"
        );
        assert_eq!(f.line, 1, "expected line 1");
        assert_eq!(f.col, 1, "expected col 1");
    }
}

// -------------------------------------------------------------------------
// range_narrowed helpers
// -------------------------------------------------------------------------

#[test]
fn test_range_narrowed_caret() {
    assert!(
        range_narrowed("^17.0.0", "^18.0.0"),
        "^18 has higher min than ^17"
    );
}

#[test]
fn test_range_narrowed_minor() {
    assert!(
        range_narrowed("^17.0.0", "^17.1.0"),
        "^17.1.0 has higher min than ^17.0.0"
    );
}

#[test]
fn test_range_min_not_increased() {
    assert!(
        !range_narrowed("^18.0.0", ">=17.0.0"),
        ">=17 still accepts all of ^18"
    );
}

#[test]
fn test_range_min_wildcard() {
    // "*" parses to an unbounded range (min_version returns 0.0.0 in node-semver)
    // "^18.0.0" has min 18.0.0 which is > 0.0.0
    assert!(
        range_narrowed("*", "^18.0.0"),
        "^18 is more restrictive than *"
    );
}

// -------------------------------------------------------------------------
// Empty peerDependencies
// -------------------------------------------------------------------------

#[test]
fn test_empty_peer_deps_no_findings() {
    let old = make_pkg(r#"{"peerDependencies": {}}"#);
    let new = make_pkg(r#"{"peerDependencies": {}}"#);
    let mut findings = Vec::new();
    compare_peer_deps(&old, &new, &mut findings);
    assert_eq!(findings.len(), 0);
}

// -------------------------------------------------------------------------
// compare_metadata full-path integration tests (using tempdir)
// -------------------------------------------------------------------------

#[test]
fn test_compare_metadata_full_path() {
    let old_dir = tempdir().unwrap();
    let new_dir = tempdir().unwrap();

    std::fs::write(
        old_dir.path().join("package.json"),
        r#"{"name": "my-pkg", "peerDependencies": {"react": "^17.0.0"}, "main": "./lib/index.js"}"#,
    )
    .unwrap();
    std::fs::write(
            new_dir.path().join("package.json"),
            r#"{"name": "my-pkg", "peerDependencies": {"react": "^18.0.0"}, "main": "./dist/index.js"}"#,
        )
        .unwrap();

    let findings = compare_metadata(old_dir.path(), new_dir.path());
    // Expect: peer-dep-version-bumped + entry-point-changed
    assert_eq!(
        findings.len(),
        2,
        "expected 2 findings, got: {:?}",
        findings
    );

    let rules: Vec<&str> = findings.iter().map(|f| f.rule).collect();
    assert!(
        rules.contains(&RULE_PEER_DEP_BUMPED),
        "expected peer-dep-version-bumped"
    );
    assert!(
        rules.contains(&RULE_ENTRY_POINT_CHANGED),
        "expected entry-point-changed"
    );
}

// -------------------------------------------------------------------------
// EXP-11: bin-entry-changed
// -------------------------------------------------------------------------

#[test]
fn test_bin_entry_path_changed() {
    let old = make_pkg(r#"{"bin": {"cli": "./cli.js"}}"#);
    let new = make_pkg(r#"{"bin": {"cli": "./new-cli.js"}}"#);
    let mut findings = Vec::new();
    compare_bin_entries(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_BIN_ENTRY_CHANGED);
    assert_eq!(findings[0].severity, Severity::Major);
    assert!(findings[0].message.contains("./cli.js"));
    assert!(findings[0].message.contains("./new-cli.js"));
}

#[test]
fn test_bin_entry_removed() {
    let old = make_pkg(r#"{"bin": {"cli": "./cli.js", "helper": "./helper.js"}}"#);
    let new = make_pkg(r#"{"bin": {"cli": "./cli.js"}}"#);
    let mut findings = Vec::new();
    compare_bin_entries(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_BIN_ENTRY_CHANGED);
    assert!(findings[0].message.contains("helper"));
}

#[test]
fn test_bin_string_to_object_same_path_no_finding() {
    // old: string shorthand, new: equivalent object form — no finding
    let old = make_pkg(r#"{"name": "my-pkg", "bin": "./cli.js"}"#);
    let new = make_pkg(r#"{"name": "my-pkg", "bin": {"my-pkg": "./cli.js"}}"#);
    let mut findings = Vec::new();
    compare_bin_entries(&old, &new, &mut findings);
    assert_eq!(
        findings.len(),
        0,
        "string bin normalized to same object should produce no finding"
    );
}

#[test]
fn test_bin_string_to_string_changed_path() {
    // old: string form "./cli.js", new: string form "./new-cli.js"
    let old = make_pkg(r#"{"name": "my-pkg", "bin": "./cli.js"}"#);
    let new = make_pkg(r#"{"name": "my-pkg", "bin": "./new-cli.js"}"#);
    let mut findings = Vec::new();
    compare_bin_entries(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_BIN_ENTRY_CHANGED);
}

#[test]
fn test_bin_added_is_non_breaking() {
    // new bin entry added — no finding
    let old = make_pkg(r#"{"bin": {"cli": "./cli.js"}}"#);
    let new = make_pkg(r#"{"bin": {"cli": "./cli.js", "extra": "./extra.js"}}"#);
    let mut findings = Vec::new();
    compare_bin_entries(&old, &new, &mut findings);
    assert_eq!(findings.len(), 0, "added bin entry is non-breaking");
}

// -------------------------------------------------------------------------
// EXP-12: platform-narrowed
// -------------------------------------------------------------------------

#[test]
fn test_os_platform_removed() {
    let old = make_pkg(r#"{"os": ["linux", "darwin", "win32"]}"#);
    let new = make_pkg(r#"{"os": ["linux", "darwin"]}"#);
    let mut findings = Vec::new();
    compare_platform(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_PLATFORM_NARROWED);
    assert!(findings[0].message.contains("win32"));
}

#[test]
fn test_os_field_added_restricts_platforms() {
    // old: no os field (unrestricted), new: restricted to linux
    let old = make_pkg(r#"{}"#);
    let new = make_pkg(r#"{"os": ["linux"]}"#);
    let mut findings = Vec::new();
    compare_platform(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_PLATFORM_NARROWED);
}

#[test]
fn test_os_expansion_is_non_breaking() {
    // old: restricted to linux, new: linux + darwin — expansion, no finding
    let old = make_pkg(r#"{"os": ["linux"]}"#);
    let new = make_pkg(r#"{"os": ["linux", "darwin"]}"#);
    let mut findings = Vec::new();
    compare_platform(&old, &new, &mut findings);
    assert_eq!(findings.len(), 0, "platform expansion is non-breaking");
}

#[test]
fn test_cpu_platform_removed() {
    let old = make_pkg(r#"{"cpu": ["x64", "arm64"]}"#);
    let new = make_pkg(r#"{"cpu": ["x64"]}"#);
    let mut findings = Vec::new();
    compare_platform(&old, &new, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RULE_PLATFORM_NARROWED);
    assert!(findings[0].message.contains("arm64"));
}

#[test]
fn test_no_package_json_returns_empty() {
    let old_dir = tempdir().unwrap();
    let new_dir = tempdir().unwrap();
    // No package.json written — should return empty
    let findings = compare_metadata(old_dir.path(), new_dir.path());
    assert_eq!(findings.len(), 0);
}

// -------------------------------------------------------------------------
// typesVersions diff
// -------------------------------------------------------------------------

#[test]
fn test_types_versions_removed() {
    // Old has typesVersions, new does not — breaking
    let old = make_pkg(r#"{"typesVersions": {">=4.0": {"*": ["dist/types/*"]}}}"#);
    let new = make_pkg(r#"{}"#);
    let mut findings = Vec::new();
    compare_types_versions(&old, &new, &mut findings);
    assert_eq!(
        findings.len(),
        1,
        "expected 1 finding for removed typesVersions, got: {:?}",
        findings
    );
    assert_eq!(findings[0].rule, RULE_TYPES_VERSIONS_CHANGED);
    assert_eq!(findings[0].severity, Severity::Major);
    assert!(findings[0].message.contains("removed"));
}

#[test]
fn test_types_versions_range_removed() {
    // Old has ">=4.0" range, new removes it
    let old = make_pkg(
        r#"{"typesVersions": {">=4.0": {"*": ["dist/types/*"]}, ">=3.0": {"*": ["dist/legacy/*"]}}}"#,
    );
    let new = make_pkg(r#"{"typesVersions": {">=4.0": {"*": ["dist/types/*"]}}}"#);
    let mut findings = Vec::new();
    compare_types_versions(&old, &new, &mut findings);
    assert_eq!(
        findings.len(),
        1,
        "expected 1 finding for removed range, got: {:?}",
        findings
    );
    assert_eq!(findings[0].rule, RULE_TYPES_VERSIONS_CHANGED);
    assert_eq!(findings[0].severity, Severity::Major);
    assert!(
        findings[0].message.contains(">=3.0"),
        "message should mention removed range; got: {}",
        findings[0].message
    );
}

#[test]
fn test_types_versions_mappings_changed() {
    // Both have ">=4.0" but different path mappings
    let old = make_pkg(r#"{"typesVersions": {">=4.0": {"*": ["dist/types/*"]}}}"#);
    let new = make_pkg(r#"{"typesVersions": {">=4.0": {"*": ["dist/new-types/*"]}}}"#);
    let mut findings = Vec::new();
    compare_types_versions(&old, &new, &mut findings);
    assert_eq!(
        findings.len(),
        1,
        "expected 1 finding for changed mappings, got: {:?}",
        findings
    );
    assert_eq!(findings[0].rule, RULE_TYPES_VERSIONS_CHANGED);
    assert_eq!(findings[0].severity, Severity::Major);
    assert!(
        findings[0].message.contains(">=4.0"),
        "message should mention the range; got: {}",
        findings[0].message
    );
}

#[test]
fn test_types_versions_added_no_finding() {
    // Old has no typesVersions, new adds it — non-breaking
    let old = make_pkg(r#"{}"#);
    let new = make_pkg(r#"{"typesVersions": {">=4.0": {"*": ["dist/types/*"]}}}"#);
    let mut findings = Vec::new();
    compare_types_versions(&old, &new, &mut findings);
    assert_eq!(
        findings.len(),
        0,
        "adding typesVersions should produce no finding (non-breaking); got: {:?}",
        findings
    );
}

// ── META-01: side-effects-widened ─────────────────────────────────────────

#[test]
fn side_effects_false_to_true_fires_warning() {
    let old = make_pkg(r#"{"sideEffects": false}"#);
    let new = make_pkg(r#"{"sideEffects": true}"#);
    let mut findings = Vec::new();
    compare_side_effects(&old, &new, &mut findings);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "side-effects-widened" && matches!(f.severity, Severity::Warning)),
        "META-01a: expected side-effects-widened Warning; got: {:?}",
        findings
    );
}

#[test]
fn side_effects_false_to_array_fires_warning() {
    let old = make_pkg(r#"{"sideEffects": false}"#);
    let new = make_pkg(r#"{"sideEffects": ["./*.css"]}"#);
    let mut findings = Vec::new();
    compare_side_effects(&old, &new, &mut findings);
    assert!(
        findings.iter().any(|f| f.rule == "side-effects-widened"),
        "META-01b: expected side-effects-widened; got: {:?}",
        findings
    );
}

#[test]
fn side_effects_array_widened_fires_warning() {
    let old = make_pkg(r#"{"sideEffects": ["./a.css"]}"#);
    let new = make_pkg(r#"{"sideEffects": ["./a.css", "./b.css"]}"#);
    let mut findings = Vec::new();
    compare_side_effects(&old, &new, &mut findings);
    assert!(
        findings.iter().any(|f| f.rule == "side-effects-widened"),
        "META-01c: expected side-effects-widened for array superset; got: {:?}",
        findings
    );
}

#[test]
fn side_effects_true_to_false_no_finding() {
    let old = make_pkg(r#"{"sideEffects": true}"#);
    let new = make_pkg(r#"{"sideEffects": false}"#);
    let mut findings = Vec::new();
    compare_side_effects(&old, &new, &mut findings);
    assert!(
        !findings.iter().any(|f| f.rule == "side-effects-widened"),
        "META-01d: narrowing should produce no finding; got: {:?}",
        findings
    );
}

#[test]
fn side_effects_unchanged_no_finding() {
    let old = make_pkg(r#"{"sideEffects": false}"#);
    let new = make_pkg(r#"{"sideEffects": false}"#);
    let mut findings = Vec::new();
    compare_side_effects(&old, &new, &mut findings);
    assert!(
        !findings.iter().any(|f| f.rule == "side-effects-widened"),
        "META-01e: unchanged should produce no finding; got: {:?}",
        findings
    );
}

// ── META-02: peer-dep-optional-removed ────────────────────────────────────

#[test]
fn peer_dep_optional_true_to_false_fires_major() {
    let old = make_pkg(r#"{"peerDependenciesMeta": {"react": {"optional": true}}}"#);
    let new = make_pkg(r#"{"peerDependenciesMeta": {"react": {"optional": false}}}"#);
    let mut findings = Vec::new();
    compare_peer_deps_meta_optional(&old, &new, &mut findings);
    assert!(
            findings
                .iter()
                .any(|f| f.rule == "peer-dep-optional-removed"
                    && matches!(f.severity, Severity::Major)),
            "META-02a: expected peer-dep-optional-removed Major; got: {:?}",
            findings
        );
}

#[test]
fn peer_dep_optional_entry_removed_fires_major() {
    let old = make_pkg(r#"{"peerDependenciesMeta": {"react": {"optional": true}}}"#);
    let new = make_pkg(r#"{}"#);
    let mut findings = Vec::new();
    compare_peer_deps_meta_optional(&old, &new, &mut findings);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "peer-dep-optional-removed"),
        "META-02b: expected peer-dep-optional-removed when entry removed; got: {:?}",
        findings
    );
}

#[test]
fn peer_dep_optional_unchanged_no_finding() {
    let v = make_pkg(r#"{"peerDependenciesMeta": {"react": {"optional": true}}}"#);
    let mut findings = Vec::new();
    compare_peer_deps_meta_optional(&v, &v, &mut findings);
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == "peer-dep-optional-removed"),
        "META-02c: unchanged optional=true should produce no finding; got: {:?}",
        findings
    );
}

// ── SEM-06: main+exports dedup ────────────────────────────────────────────

#[test]
fn main_change_suppressed_when_exports_dot_stable() {
    // main changes but exports["."] is identical in both — suppress main finding
    let old = make_pkg(r#"{"main": "./old.js", "exports": {".": "./same.js"}}"#);
    let new = make_pkg(r#"{"main": "./new.js", "exports": {".": "./same.js"}}"#);
    let mut findings = Vec::new();
    compare_entry_points(&old, &new, &mut findings);
    assert!(
        !findings.iter().any(
            |f| f.rule == RULE_ENTRY_POINT_CHANGED && f.message.to_lowercase().contains("main")
        ),
        "SEM-06: main-change should be suppressed when exports[\".\"] is stable; got: {:?}",
        findings
    );
}

#[test]
fn main_change_fires_when_exports_dot_absent() {
    // main changes and there is no exports field — finding must still fire
    let old = make_pkg(r#"{"main": "./old.js"}"#);
    let new = make_pkg(r#"{"main": "./new.js"}"#);
    let mut findings = Vec::new();
    compare_entry_points(&old, &new, &mut findings);
    assert!(
        findings.iter().any(|f| f.rule == RULE_ENTRY_POINT_CHANGED),
        "SEM-06: main-change must still fire when exports field absent; got: {:?}",
        findings
    );
}
