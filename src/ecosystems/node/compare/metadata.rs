// Package metadata comparison: detect breaking changes in package.json fields.
//
// Covers META-01..07:
// META-01: peer dependency minimum version bumped (peer-dep-version-bumped)
// META-02: peer dependency added (peer-dep-added)
// META-03: peer dependency removed (peer-dep-removed)
// META-04: engine minimum version bumped (engine-version-bumped)
// META-05: engine requirement added (engine-added)
// META-06: entry point changed or removed (entry-point-changed)
// META-07: module type changed (module-type-changed)
//
// clean separation from type comparison.
// does its own lightweight package.json parsing.
// all findings use "package.json" as file path with line 1, col 1.

use crate::core::findings::{Finding, Severity};
use std::collections::HashMap;
use std::path::Path;

// Rule ID constants imported from subtype.rs (single source of truth)
use crate::ecosystems::node::compare::rules::{
    RULE_BIN_ENTRY_CHANGED, RULE_ENGINE_ADDED, RULE_ENGINE_BUMPED, RULE_ENTRY_POINT_CHANGED,
    RULE_MODULE_TYPE_CHANGED, RULE_PEER_DEP_ADDED, RULE_PEER_DEP_BUMPED,
    RULE_PEER_DEP_OPTIONAL_REMOVED, RULE_PEER_DEP_REMOVED, RULE_PLATFORM_NARROWED,
    RULE_SIDE_EFFECTS_WIDENED, RULE_TYPES_VERSIONS_CHANGED,
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

/// Create a Finding anchored to package.json line 1, col 1.
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
            map.insert(
                name.rsplit('/').next().unwrap_or(name).to_string(),
                s.clone(),
            );
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

    // A positive list is an allowlist; negative entries exclude names from it.
    // Probe every mentioned name plus one unmentioned platform, representing
    // the unrestricted remainder without hard-coding today's Node platforms.
    let accepts = |entries: &[String], platform: &str| {
        let allowed = entries == ["any"]
            || !entries.iter().any(|s| !s.starts_with('!'))
            || entries.iter().any(|s| s == platform);
        allowed
            && !entries
                .iter()
                .any(|s| s.strip_prefix('!') == Some(platform))
    };
    let mut platforms: std::collections::BTreeSet<&str> = old_vals
        .iter()
        .chain(&new_vals)
        .map(|s| s.trim_start_matches('!'))
        .collect();
    platforms.insert("<other platforms>");
    for platform in platforms {
        if accepts(&old_vals, platform) && !accepts(&new_vals, platform) {
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_PLATFORM_NARROWED,
                format!("platform `{platform}` is no longer accepted by `{field}`"),
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

/// A range changes membership only at comparator boundaries or prerelease cores.
/// Check each boundary and its immediate successors. Interval subtraction alone
/// loses npm's requirement to explicitly admit prereleases of a particular core.
pub(crate) fn range_narrowed(old_range: &str, new_range: &str) -> bool {
    range_narrowed_with_membership(old_range, new_range, RangeMembership::Dependency)
}

enum RangeMembership {
    Dependency,
    Engine,
}

fn range_narrowed_with_membership(
    old_range: &str,
    new_range: &str,
    membership: RangeMembership,
) -> bool {
    use node_semver::{Identifier, Range, Version};
    let parse = |range: &str| match membership {
        RangeMembership::Dependency => Range::parse(range).ok(),
        RangeMembership::Engine => parse_engine_range(range),
    };
    let (Some(old), Some(new)) = (parse(old_range), parse(new_range)) else {
        return old_range != new_range;
    };
    let mut witnesses = vec![Version::from((0, 0, 0))];
    for normalized in [old.to_string(), new.to_string()] {
        for comparator in normalized.split("||").flat_map(str::split_whitespace) {
            let Ok(boundary) = Version::parse(comparator.trim_start_matches(['<', '>', '=']))
            else {
                continue;
            };
            witnesses.push(boundary.clone());
            if boundary.is_prerelease() {
                let mut successor = boundary.clone();
                successor.pre_release.push(Identifier::Numeric(0));
                witnesses.push(successor);
            }
            let stable = Version::from((boundary.major, boundary.minor, boundary.patch));
            let mut first_prerelease = stable.clone();
            first_prerelease.pre_release.push(Identifier::Numeric(0));
            witnesses.push(first_prerelease);
            witnesses.push(stable.clone());
            if let Some(patch) = stable.patch.checked_add(1) {
                witnesses.push(Version::from((stable.major, stable.minor, patch)));
            }
        }
    }
    let accepts = |range: &Range, version: &Version| match membership {
        RangeMembership::Dependency => range.satisfies(version),
        // npm checks engines with includePrerelease enabled. Interval intersection
        // applies comparator bounds without dependency-only prerelease exclusion.
        RangeMembership::Engine => Range::parse(version.to_string())
            .ok()
            .is_some_and(|exact| range.intersect(&exact).is_some()),
    };
    witnesses
        .iter()
        .any(|version| accepts(&old, version) && !accepts(&new, version))
}

/// Expand npm's shorthand bounds with includePrerelease enabled. The existing
/// range parser supplies caret/tilde/wildcard arithmetic; generated boundaries
/// include the first prerelease, while explicitly written full versions retain
/// their original comparator semantics.
pub(crate) fn parse_engine_range(source: &str) -> Option<node_semver::Range> {
    use node_semver::{Range, Version};
    fn expand(source: &str, hyphen: bool) -> Option<String> {
        let normalized = Range::parse(source).ok()?.to_string();
        let version_text = source.trim_start_matches(['<', '>', '=', '~', '^']).trim();
        let explicit = (!hyphen)
            .then(|| Version::parse(version_text).ok())
            .flatten();
        let shorthand = source.starts_with(['~', '^']);
        let mut comparators = Vec::new();
        for comparator in normalized.split_whitespace() {
            let version_text = comparator.trim_start_matches(['<', '>', '=']);
            let Ok(mut version) = Version::parse(version_text) else {
                comparators.push(comparator.to_string());
                continue;
            };
            if version.is_prerelease()
                || explicit.as_ref().is_some_and(|written| {
                    !shorthand || (comparator.starts_with(">=") && written == &version)
                })
            {
                comparators.push(comparator.to_string());
                continue;
            }
            let operator = &comparator[..comparator.len() - version_text.len()];
            if operator == "<=" {
                version.patch = version.patch.checked_add(1)?;
                comparators.push(format!("<{version}-0"));
            } else {
                comparators.push(format!("{operator}{version}-0"));
            }
        }
        Some(comparators.join(" "))
    }
    let mut branches = Vec::new();
    for branch in source.split("||") {
        if branch.contains(" - ") {
            branches.push(expand(branch.trim(), true)?);
            continue;
        }
        let mut tokens = branch.split_whitespace();
        let mut comparators = Vec::new();
        while let Some(token) = tokens.next() {
            let comparator = if matches!(token, "<" | "<=" | ">" | ">=" | "=" | "~" | "^") {
                format!("{token}{}", tokens.next()?)
            } else {
                token.to_string()
            };
            comparators.push(expand(&comparator, false)?);
        }
        branches.push(comparators.join(" "));
    }
    Range::parse(branches.join(" || ")).ok()
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
            if old_range_str != new_range_str && range_narrowed(old_range_str, new_range_str) {
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_PEER_DEP_BUMPED,
                    format!(
                        "peer dependency `{name}` supported version range narrowed from `{old_range_str}` to `{new_range_str}`"
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
            if old_range_str != new_range_str
                && range_narrowed_with_membership(
                    old_range_str,
                    new_range_str,
                    RangeMembership::Engine,
                )
            {
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_ENGINE_BUMPED,
                    format!(
                        "engine `{name}` supported version range narrowed from `{old_range_str}` to `{new_range_str}`"
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
/// handled by exports_diff.rs at higher granularity.
fn exports_dot_string(pkg: &serde_json::Value) -> Option<String> {
    pkg.get("exports")
        .and_then(|e| e.get("."))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// META-06: detect entry point field changes.
///
/// Checks `main`, `module`, `types` (string fields).
/// Note: `exports` field diffing is handled by exports_diff.rs.
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
    // `exports` field is intentionally NOT checked here:
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
            let before: Vec<_> = old
                .keys()
                .filter(|range| new.contains_key(*range))
                .collect();
            let after: Vec<_> = new
                .keys()
                .filter(|range| old.contains_key(*range))
                .collect();
            for (index, left) in before.iter().enumerate() {
                for right in &before[index + 1..] {
                    let reversed = after.iter().position(|range| range == left)
                        > after.iter().position(|range| range == right);
                    if !reversed || old[*left] == old[*right] {
                        continue;
                    }
                    let overlap = match (
                        node_semver::Range::parse(left),
                        node_semver::Range::parse(right),
                    ) {
                        (Ok(left), Ok(right)) => left.intersect(&right).is_some(),
                        _ => false,
                    };
                    if overlap {
                        findings.push(pkg_json_finding(Severity::Major, RULE_TYPES_VERSIONS_CHANGED, format!("typesVersions priority changed between overlapping ranges `{left}` and `{right}`")));
                    }
                }
            }
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
mod tests;
