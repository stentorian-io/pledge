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
    RULE_BIN_ENTRY_CHANGED, RULE_ENGINE_ADDED, RULE_ENGINE_BUMPED, RULE_ENTRY_POINT_CHANGED,
    RULE_MODULE_TYPE_CHANGED, RULE_PEER_DEP_ADDED, RULE_PEER_DEP_BUMPED, RULE_PEER_DEP_REMOVED,
    RULE_PEER_DEP_OPTIONAL_REMOVED, RULE_PLATFORM_NARROWED, RULE_SIDE_EFFECTS_WIDENED,
    RULE_TYPES_VERSIONS_CHANGED,
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
    compare_bin_entries(&old_pkg, &new_pkg, &mut findings);
    compare_platform(&old_pkg, &new_pkg, &mut findings);
    compare_types_versions(&old_pkg, &new_pkg, &mut findings);
    compare_side_effects(&old_pkg, &new_pkg, &mut findings); // META-01
    compare_peer_deps_meta_optional(&old_pkg, &new_pkg, &mut findings); // META-02
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

/// Extract a string array from a JSON array field.
fn extract_string_array(pkg: &serde_json::Value, field: &str) -> Vec<String> {
    pkg.get(field)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Normalize the `bin` field to a `{command: path}` map.
///
/// Per npm docs: `"bin": "./cli.js"` is shorthand for `{"<name>": "./cli.js"}`.
fn normalize_bin(pkg: &serde_json::Value) -> HashMap<String, String> {
    let name = pkg
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    match pkg.get("bin") {
        Some(serde_json::Value::String(s)) => {
            let mut map = HashMap::new();
            map.insert(name.to_string(), s.clone());
            map
        }
        Some(serde_json::Value::Object(_)) => extract_string_map(pkg, "bin"),
        _ => HashMap::new(),
    }
}

/// EXP-11: detect bin entry changes (removed or path-changed commands).
fn compare_bin_entries(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
    findings: &mut Vec<Finding>,
) {
    let old_bin = normalize_bin(old_pkg);
    let new_bin = normalize_bin(new_pkg);

    for (cmd, old_path) in &old_bin {
        match new_bin.get(cmd.as_str()) {
            None => {
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_BIN_ENTRY_CHANGED,
                    format!("bin entry `{cmd}` removed (was `{old_path}`)"),
                ));
            }
            Some(new_path) if new_path != old_path => {
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_BIN_ENTRY_CHANGED,
                    format!("bin entry `{cmd}` changed from `{old_path}` to `{new_path}`"),
                ));
            }
            _ => {}
        }
    }
    // Added bin entries are non-breaking (no finding)
}

/// Check a single platform field (os or cpu) for narrowing.
fn compare_platform_field(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
    field: &str,
    findings: &mut Vec<Finding>,
) {
    let old_vals = extract_string_array(old_pkg, field);
    let new_vals = extract_string_array(new_pkg, field);

    // If old was unrestricted (empty) and new restricts: narrowing
    if old_vals.is_empty() && !new_vals.is_empty() {
        findings.push(pkg_json_finding(
            Severity::Major,
            RULE_PLATFORM_NARROWED,
            format!(
                "`{field}` field added, restricting platform support to: {}",
                new_vals.join(", ")
            ),
        ));
        return;
    }

    // Check for removed platforms (old had it, new doesn't)
    for platform in &old_vals {
        if !new_vals.contains(platform) {
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_PLATFORM_NARROWED,
                format!("platform `{platform}` removed from `{field}` field"),
            ));
        }
    }
}

/// EXP-12: detect platform field narrowing (os, cpu).
fn compare_platform(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
    findings: &mut Vec<Finding>,
) {
    compare_platform_field(old_pkg, new_pkg, "os", findings);
    compare_platform_field(old_pkg, new_pkg, "cpu", findings);
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

/// SEM-06: Get `exports["."]` as a bare string, or None for objects/null/arrays.
///
/// Conservative — only suppresses the main/module finding when exports["."] is a
/// string literal equal in both versions. Objects and conditional exports maps are
/// handled by exports_diff.rs at higher granularity (D-07).
fn exports_dot_string(pkg: &serde_json::Value) -> Option<String> {
    pkg.get("exports")
        .and_then(|e| e.get("."))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// META-06: detect entry point field changes.
///
/// Checks `main`, `module`, `types` (string fields).
/// Note: `exports` field diffing is handled by exports_diff.rs (D-07).
fn compare_entry_points(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
    findings: &mut Vec<Finding>,
) {
    // SEM-06: Suppress main/module entry-point-changed when exports["."] is a stable
    // bare string in both versions. The exports field is the authoritative entry point
    // when present; main/module become redundant legacy aliases.
    let old_dot = exports_dot_string(old_pkg);
    let new_dot = exports_dot_string(new_pkg);
    let exports_dot_stable = old_dot.is_some() && old_dot == new_dot;

    // Simple string fields
    for field in &["main", "module", "types"] {
        let old_val = old_pkg.get(field).and_then(|v| v.as_str());
        let new_val = new_pkg.get(field).and_then(|v| v.as_str());

        // SEM-06: Skip main/module finding when exports["."] is stable.
        // `types` is always checked — it's type-resolution-specific, not covered by exports["."].
        if exports_dot_stable && (*field == "main" || *field == "module") {
            continue;
        }

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
    // `exports` field is intentionally NOT checked here (D-07):
    // granular exports diffing is delegated to exports_diff.rs
}

/// Compare `typesVersions` field between old and new package.json.
///
/// Detects:
/// - Version range removed (e.g., ">=4.0" entry deleted)
/// - Path mappings changed within a version range
/// - Version range added (informational — non-breaking)
fn compare_types_versions(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
    findings: &mut Vec<Finding>,
) {
    let old_tv = old_pkg.get("typesVersions").and_then(|v| v.as_object());
    let new_tv = new_pkg.get("typesVersions").and_then(|v| v.as_object());

    match (old_tv, new_tv) {
        (None, None) => {} // No typesVersions in either
        (Some(_old), None) => {
            // typesVersions field removed entirely
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_TYPES_VERSIONS_CHANGED,
                "typesVersions field removed — version-conditional type resolution may break"
                    .to_string(),
            ));
        }
        (None, Some(_)) => {
            // typesVersions added — informational, not breaking
            // (No finding emitted — adding type version mappings is non-breaking)
        }
        (Some(old), Some(new)) => {
            // Compare version ranges
            for (range, old_mappings) in old {
                match new.get(range) {
                    None => {
                        findings.push(pkg_json_finding(
                            Severity::Major,
                            RULE_TYPES_VERSIONS_CHANGED,
                            format!("typesVersions range `{range}` removed"),
                        ));
                    }
                    Some(new_mappings) => {
                        // Compare the JSON values — if they differ, report
                        if old_mappings != new_mappings {
                            findings.push(pkg_json_finding(
                                Severity::Major,
                                RULE_TYPES_VERSIONS_CHANGED,
                                format!("typesVersions mappings changed for range `{range}`"),
                            ));
                        }
                    }
                }
            }
        }
    }
}

/// META-01: sideEffects field widening detection.
///
/// Widening cases (emit side-effects-widened Warning):
///
/// - `false` -> `true`
/// - `false` -> `<any array>` (array implies some side effects)
/// - `<array with N items>` -> `<array with N+M items, M > 0>` (superset)
///
/// Narrowing or removal of sideEffects field when old was true is not widening.
/// Removing explicit false/array (reverting to implicit true) IS widening.
fn compare_side_effects(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
    findings: &mut Vec<Finding>,
) {
    let old = old_pkg.get("sideEffects");
    let new = new_pkg.get("sideEffects");
    let widened = match (old, new) {
        (None, None) => false,
        (None, Some(_)) => {
            // Default is effectively "true" (has side effects). Explicit
            // value in new is at most as broad. No widening.
            false
        }
        (Some(_), None) => {
            // Removing explicit sideEffects reverts to default "true" (has side effects).
            // This is widening from an explicit false/array to implicit true.
            matches!(
                old,
                Some(serde_json::Value::Bool(false)) | Some(serde_json::Value::Array(_))
            )
        }
        (Some(o), Some(n)) => side_effects_widened(o, n),
    };
    if widened {
        findings.push(pkg_json_finding(
            Severity::Warning,
            RULE_SIDE_EFFECTS_WIDENED,
            format!(
                "sideEffects widened: {} -> {}",
                old.map(|v| v.to_string())
                    .unwrap_or_else(|| "(unset)".into()),
                new.map(|v| v.to_string())
                    .unwrap_or_else(|| "(unset)".into())
            ),
        ));
    }
}

/// Helper: determine if moving from `old` sideEffects value to `new` is a widening.
fn side_effects_widened(old: &serde_json::Value, new: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (old, new) {
        (Value::Bool(false), Value::Bool(true)) => true,
        (Value::Bool(false), Value::Array(_)) => true,
        (Value::Bool(true), _) => false,
        (_, Value::Bool(false)) => false,
        (Value::Array(old_arr), Value::Array(new_arr)) => {
            // Widening if new_arr is a strict superset of old_arr
            let old_set: std::collections::HashSet<String> = old_arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            let new_set: std::collections::HashSet<String> = new_arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            new_set.len() > old_set.len() && old_set.is_subset(&new_set)
        }
        (Value::Array(_), Value::Bool(true)) => true,
        _ => false,
    }
}

/// META-02: peerDependenciesMeta.<dep>.optional removed or flipped true -> false.
/// Both produce peer-dep-optional-removed Major because consumers now MUST
/// install the dep where previously they could skip it.
fn compare_peer_deps_meta_optional(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
    findings: &mut Vec<Finding>,
) {
    let Some(old_meta) = old_pkg
        .get("peerDependenciesMeta")
        .and_then(|v| v.as_object())
    else {
        return;
    };
    let new_meta = new_pkg
        .get("peerDependenciesMeta")
        .and_then(|v| v.as_object());
    for (dep, old_entry) in old_meta {
        let old_optional = old_entry
            .get("optional")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !old_optional {
            continue;
        }
        // Was optional=true in old. Check new.
        let new_optional = new_meta
            .and_then(|m| m.get(dep))
            .and_then(|e| e.get("optional"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !new_optional {
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_PEER_DEP_OPTIONAL_REMOVED,
                format!(
                    "peerDependenciesMeta.{}.optional was removed (was true; consumers must now install `{}`)",
                    dep, dep
                ),
            ));
        }
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
    fn test_exports_field_changed_no_longer_triggers_entry_point_changed() {
        // D-07: exports changes are handled by exports_diff.rs, not compare_entry_points
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
    // GAP-15-03: typesVersions diff
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
    fn meta_01_side_effects_false_to_true_fires_warning() {
        let old = make_pkg(r#"{"sideEffects": false}"#);
        let new = make_pkg(r#"{"sideEffects": true}"#);
        let mut findings = Vec::new();
        compare_side_effects(&old, &new, &mut findings);
        assert!(
            findings.iter().any(|f| f.rule == "side-effects-widened"
                && matches!(f.severity, Severity::Warning)),
            "META-01a: expected side-effects-widened Warning; got: {:?}",
            findings
        );
    }

    #[test]
    fn meta_01_side_effects_false_to_array_fires_warning() {
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
    fn meta_01_side_effects_array_widened_fires_warning() {
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
    fn meta_01_side_effects_true_to_false_no_finding() {
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
    fn meta_01_side_effects_unchanged_no_finding() {
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
    fn meta_02_peer_dep_optional_true_to_false_fires_major() {
        let old = make_pkg(r#"{"peerDependenciesMeta": {"react": {"optional": true}}}"#);
        let new = make_pkg(r#"{"peerDependenciesMeta": {"react": {"optional": false}}}"#);
        let mut findings = Vec::new();
        compare_peer_deps_meta_optional(&old, &new, &mut findings);
        assert!(
            findings.iter().any(|f| f.rule == "peer-dep-optional-removed"
                && matches!(f.severity, Severity::Major)),
            "META-02a: expected peer-dep-optional-removed Major; got: {:?}",
            findings
        );
    }

    #[test]
    fn meta_02_peer_dep_optional_entry_removed_fires_major() {
        let old = make_pkg(r#"{"peerDependenciesMeta": {"react": {"optional": true}}}"#);
        let new = make_pkg(r#"{}"#);
        let mut findings = Vec::new();
        compare_peer_deps_meta_optional(&old, &new, &mut findings);
        assert!(
            findings.iter().any(|f| f.rule == "peer-dep-optional-removed"),
            "META-02b: expected peer-dep-optional-removed when entry removed; got: {:?}",
            findings
        );
    }

    #[test]
    fn meta_02_peer_dep_optional_unchanged_no_finding() {
        let v = make_pkg(r#"{"peerDependenciesMeta": {"react": {"optional": true}}}"#);
        let mut findings = Vec::new();
        compare_peer_deps_meta_optional(&v, &v, &mut findings);
        assert!(
            !findings.iter().any(|f| f.rule == "peer-dep-optional-removed"),
            "META-02c: unchanged optional=true should produce no finding; got: {:?}",
            findings
        );
    }

    // ── SEM-06: main+exports dedup ────────────────────────────────────────────

    #[test]
    fn sem_06_main_change_suppressed_when_exports_dot_stable() {
        // main changes but exports["."] is identical in both — suppress main finding
        let old = make_pkg(r#"{"main": "./old.js", "exports": {".": "./same.js"}}"#);
        let new = make_pkg(r#"{"main": "./new.js", "exports": {".": "./same.js"}}"#);
        let mut findings = Vec::new();
        compare_entry_points(&old, &new, &mut findings);
        assert!(
            !findings
                .iter()
                .any(|f| f.rule == RULE_ENTRY_POINT_CHANGED
                    && f.message.to_lowercase().contains("main")),
            "SEM-06: main-change should be suppressed when exports[\".\"] is stable; got: {:?}",
            findings
        );
    }

    #[test]
    fn sem_06_main_change_fires_when_exports_dot_absent() {
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
}
