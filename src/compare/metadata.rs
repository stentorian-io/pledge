// Package metadata comparison: detect breaking changes in package.json fields.
//
// Covers META-01..07:
//   META-01: peer dependency minimum version bumped (peer-dep-version-bumped)
//   META-02: peer dependency added (peer-dep-added)
//   META-03: peer dependency removed (peer-dep-removed)
//   META-04: engine minimum version bumped (engine-version-bumped)
//   META-05: engine requirement added (engine-added)
//   META-06: entry point changed or removed (entry-point-changed)
//   META-07: module type changed (module-type-changed)
//
// Per D-06: clean separation from type comparison.
// Per D-07: does its own lightweight package.json parsing.
// Per D-08: all findings use "package.json" as file path with line 1, col 1.

use std::collections::HashMap;
use std::path::Path;

use crate::types::{Finding, Severity};

// Rule ID constants imported from subtype.rs (single source of truth)
use super::subtype::{
    RULE_ENGINE_ADDED, RULE_ENGINE_BUMPED, RULE_ENTRY_POINT_CHANGED, RULE_MODULE_TYPE_CHANGED,
    RULE_PEER_DEP_ADDED, RULE_PEER_DEP_BUMPED, RULE_PEER_DEP_REMOVED,
};

/// Compare package metadata from both package roots and return a list of findings.
///
/// Reads `package.json` from each root. If either is missing, returns empty Vec.
pub fn compare_metadata(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let old_path = old_root.join("package.json");
    let new_path = new_root.join("package.json");

    let old_pkg = match read_package_json(&old_path) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let new_pkg = match read_package_json(&new_path) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let mut findings = Vec::new();
    compare_peer_deps(&old_pkg, &new_pkg, &mut findings);
    compare_engines(&old_pkg, &new_pkg, &mut findings);
    compare_entry_points(&old_pkg, &new_pkg, &mut findings);
    compare_module_type(&old_pkg, &new_pkg, &mut findings);
    findings
}

fn read_package_json(path: &Path) -> Option<serde_json::Value> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Create a Finding anchored to package.json line 1, col 1 (D-08).
fn pkg_json_finding(severity: Severity, rule: &'static str, message: String) -> Finding {
    Finding {
        severity,
        rule,
        file: std::path::PathBuf::from("package.json"),
        line: 1,
        col: 1,
        message,
    }
}

/// Extract a `{name: version}` map from a JSON object field.
fn extract_string_map(pkg: &serde_json::Value, field: &str) -> HashMap<String, String> {
    pkg.get(field)
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Return true if the minimum satisfying version of `new_range` is strictly
/// greater than the minimum satisfying version of `old_range`.
///
/// Uses `node_semver::Range::min_version()` to extract the minimum bound from
/// each npm semver range. Falls back to string inequality when parsing fails.
pub(crate) fn range_min_increased(old_range: &str, new_range: &str) -> bool {
    use node_semver::Range;

    let old_parsed = Range::parse(old_range);
    let new_parsed = Range::parse(new_range);

    match (old_parsed, new_parsed) {
        (Ok(old_r), Ok(new_r)) => {
            let old_min = old_r.min_version();
            let new_min = new_r.min_version();
            match (old_min, new_min) {
                (Some(o), Some(n)) => n > o,
                // New range has no minimum (unbounded from below) — not more restrictive
                (Some(_), None) => false,
                // Old range had no minimum but new does — new is more restrictive
                (None, Some(_)) => true,
                // Both unbounded — not more restrictive
                (None, None) => false,
            }
        }
        // If either range doesn't parse, compare as strings; different == assume breaking
        _ => old_range != new_range,
    }
}

/// META-01, META-02, META-03: detect peer dependency changes.
fn compare_peer_deps(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
    findings: &mut Vec<Finding>,
) {
    let old_peers = extract_string_map(old_pkg, "peerDependencies");
    let new_peers = extract_string_map(new_pkg, "peerDependencies");

    // META-02: added peer deps
    for (name, ver) in &new_peers {
        if !old_peers.contains_key(name.as_str()) {
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_PEER_DEP_ADDED,
                format!("peer dependency `{name}` added ({ver})"),
            ));
        }
    }

    // META-03: removed peer deps
    for name in old_peers.keys() {
        if !new_peers.contains_key(name.as_str()) {
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_PEER_DEP_REMOVED,
                format!("peer dependency `{name}` removed"),
            ));
        }
    }

    // META-01: version bumps for existing peer deps
    for (name, old_range_str) in &old_peers {
        if let Some(new_range_str) = new_peers.get(name.as_str()) {
            if old_range_str != new_range_str && range_min_increased(old_range_str, new_range_str) {
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_PEER_DEP_BUMPED,
                    format!(
                        "peer dependency `{name}` minimum version bumped from `{old_range_str}` to `{new_range_str}`"
                    ),
                ));
            }
        }
    }
}

/// META-04, META-05: detect engine requirement changes.
fn compare_engines(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
    findings: &mut Vec<Finding>,
) {
    let old_engines = extract_string_map(old_pkg, "engines");
    let new_engines = extract_string_map(new_pkg, "engines");

    // META-05: added engine requirements
    for (name, ver) in &new_engines {
        if !old_engines.contains_key(name.as_str()) {
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_ENGINE_ADDED,
                format!("engine requirement `{name}` added ({ver})"),
            ));
        }
    }

    // META-04: bumped engine requirements
    for (name, old_range_str) in &old_engines {
        if let Some(new_range_str) = new_engines.get(name.as_str()) {
            if old_range_str != new_range_str && range_min_increased(old_range_str, new_range_str) {
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_ENGINE_BUMPED,
                    format!(
                        "engine `{name}` minimum version bumped from `{old_range_str}` to `{new_range_str}`"
                    ),
                ));
            }
        }
    }
}

/// META-06: detect entry point field changes.
///
/// Checks `main`, `module`, `types` (string fields) and `exports` (structural).
fn compare_entry_points(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
    findings: &mut Vec<Finding>,
) {
    // Simple string fields
    for field in &["main", "module", "types"] {
        let old_val = old_pkg.get(field).and_then(|v| v.as_str());
        let new_val = new_pkg.get(field).and_then(|v| v.as_str());

        match (old_val, new_val) {
            (Some(old_v), Some(new_v)) if old_v != new_v => {
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_ENTRY_POINT_CHANGED,
                    format!("entry point `{field}` changed from `{old_v}` to `{new_v}`"),
                ));
            }
            (Some(old_v), None) => {
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_ENTRY_POINT_CHANGED,
                    format!("entry point `{field}` removed (was `{old_v}`)"),
                ));
            }
            // New entry point added is not breaking — it's additive
            _ => {}
        }
    }

    // `exports` field — structural comparison
    let old_exports = old_pkg.get("exports");
    let new_exports = new_pkg.get("exports");
    match (old_exports, new_exports) {
        (Some(old_e), Some(new_e)) if old_e != new_e => {
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_ENTRY_POINT_CHANGED,
                "package.json `exports` field changed".to_string(),
            ));
        }
        (Some(_), None) => {
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_ENTRY_POINT_CHANGED,
                "package.json `exports` field removed".to_string(),
            ));
        }
        _ => {}
    }
}

/// META-07: detect module type field changes.
///
/// Default is `"commonjs"` when the field is absent.
fn compare_module_type(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
    findings: &mut Vec<Finding>,
) {
    let old_type = old_pkg.get("type").and_then(|v| v.as_str());
    let new_type = new_pkg.get("type").and_then(|v| v.as_str());

    // Default is "commonjs" when field is absent
    let old_effective = old_type.unwrap_or("commonjs");
    let new_effective = new_type.unwrap_or("commonjs");

    if old_effective != new_effective {
        findings.push(pkg_json_finding(
            Severity::Major,
            RULE_MODULE_TYPE_CHANGED,
            format!("package `type` changed from `{old_effective}` to `{new_effective}`"),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_exports_field_changed() {
        let old = make_pkg(r#"{"exports": {".": "./src/index.js"}}"#);
        let new = make_pkg(r#"{"exports": {".": "./dist/index.js"}}"#);
        let mut findings = Vec::new();
        compare_entry_points(&old, &new, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RULE_ENTRY_POINT_CHANGED);
        assert_eq!(findings[0].severity, Severity::Major);
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
    // D-08: all findings use package.json path, line 1, col 1
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
    // range_min_increased helpers
    // -------------------------------------------------------------------------

    #[test]
    fn test_range_min_increased_caret() {
        assert!(
            range_min_increased("^17.0.0", "^18.0.0"),
            "^18 has higher min than ^17"
        );
    }

    #[test]
    fn test_range_min_increased_minor() {
        assert!(
            range_min_increased("^17.0.0", "^17.1.0"),
            "^17.1.0 has higher min than ^17.0.0"
        );
    }

    #[test]
    fn test_range_min_not_increased() {
        assert!(
            !range_min_increased("^18.0.0", "^17.0.0"),
            "^17 does not have higher min than ^18"
        );
    }

    #[test]
    fn test_range_min_wildcard() {
        // "*" parses to an unbounded range (min_version returns 0.0.0 in node-semver)
        // "^18.0.0" has min 18.0.0 which is > 0.0.0
        assert!(
            range_min_increased("*", "^18.0.0"),
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

    #[test]
    fn test_no_package_json_returns_empty() {
        let old_dir = tempdir().unwrap();
        let new_dir = tempdir().unwrap();
        // No package.json written — should return empty
        let findings = compare_metadata(old_dir.path(), new_dir.path());
        assert_eq!(findings.len(), 0);
    }
}
