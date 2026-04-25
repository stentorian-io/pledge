// Imports map diff: detect breaking changes in package.json `imports` field
// (EXP-14, Phase 23).
//
// Rule family (mirroring exports_diff.rs subpath-* rules):
//   import-added            (Minor) — new #subpath / null → target
//   import-removed          (Major) — #subpath disappeared
//   import-condition-changed (Major) — condition removed; Branch↔Leaf flip;
//                                       target → null
//   import-pattern-refined  (Major) — wildcard refinement (#i/* → #i/*.js)
//   import-reroute          (Major) — Leaf→Leaf target change, both non-null
//
// Data model: ImportsMap = BTreeMap<String, SubpathEntry> reusing the
// ConditionNode + SubpathEntry types from exports_diff.rs. The `imports`
// parser is structurally a 5-line variant of `parse_exports_map` — it has
// NO bare-string-shorthand and NO conditions-only-wrapped-under-`.` case
// (because `.` is not a valid `#`-prefixed key per Node.js spec).

use std::collections::BTreeMap;

use crate::types::{Finding, Severity};

use super::exports_diff::{
    parse_condition_node, pkg_json_finding, strip_wildcard_to_base,
    ConditionNode, SubpathEntry,
};
use super::subtype::{
    RULE_IMPORT_ADDED, RULE_IMPORT_CONDITION_CHANGED, RULE_IMPORT_PATTERN_REFINED,
    RULE_IMPORT_REMOVED, RULE_IMPORT_REROUTE,
};

/// Full imports map: keyed by `#`-prefixed subpath. Reuses SubpathEntry from
/// exports_diff so ConditionNode is the single shared recursive shape.
///
/// Newtype (NOT a type alias) so the type system distinguishes ImportsMap
/// from ExportsMap — the two share an identical underlying shape
/// (`BTreeMap<String, SubpathEntry>`) and a function expecting one must not
/// silently accept the other. `Deref<Target = BTreeMap<String, SubpathEntry>>`
/// makes the read-side surface (`.get`, `.iter`, `.keys`, `.contains_key`,
/// `.len`) transparent. Construct via `ImportsMap(map)` or
/// `ImportsMap::default()`.
#[derive(Debug, Clone, Default)]
pub struct ImportsMap(pub BTreeMap<String, SubpathEntry>);

impl std::ops::Deref for ImportsMap {
    type Target = BTreeMap<String, SubpathEntry>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// True if `key` is a valid imports key (starts with '#') per Node.js spec.
fn is_import_key(key: &str) -> bool {
    key.starts_with('#')
}

/// Parse the `imports` field value from package.json into a normalized ImportsMap.
///
/// Per Node spec the `imports` field expects an object whose keys all start
/// with `#`. Unlike `exports`, there is NO bare-string shorthand (because `.`
/// is not a valid imports key) and NO conditions-only-object wrapping. Other
/// shapes (string, array, null, object-with-non-#-keys) are degenerate and
/// produce an empty map.
pub fn parse_imports_map(value: &serde_json::Value) -> ImportsMap {
    match value {
        serde_json::Value::Object(obj) => ImportsMap(
            obj.iter()
                .filter(|(k, _)| is_import_key(k))
                .map(|(k, v)| {
                    (
                        k.clone(),
                        SubpathEntry {
                            node: parse_condition_node(v),
                        },
                    )
                })
                .collect(),
        ),
        // Strings / arrays / null / numbers / bools — degenerate, no valid imports
        _ => ImportsMap::default(),
    }
}

/// Format a condition node target for use in finding messages.
fn format_condition_target(node: &ConditionNode) -> String {
    match node {
        ConditionNode::Leaf(Some(path)) => path.clone(),
        ConditionNode::Leaf(None) => "null".to_string(),
        ConditionNode::Branch(_) => "[conditions object]".to_string(),
    }
}

/// Recursively diff two condition node trees for a given import key.
///
/// Mirrors `diff_condition_nodes` from exports_diff.rs with rule-ID
/// substitution and the same SEM-02 direction-aware null handling.
fn diff_import_condition_nodes(
    import_key: &str,
    old: &ConditionNode,
    new: &ConditionNode,
    findings: &mut Vec<Finding>,
) {
    match (old, new) {
        (ConditionNode::Leaf(old_target), ConditionNode::Leaf(new_target)) => {
            if old_target != new_target {
                let old_is_null = old_target.is_none();
                let new_is_null = new_target.is_none();
                let old_str = old_target.as_deref().unwrap_or("null").to_string();
                let new_str = new_target.as_deref().unwrap_or("null").to_string();
                match (old_is_null, new_is_null) {
                    (true, false) => {
                        // null → target is additive (Minor)
                        findings.push(pkg_json_finding(
                            Severity::Minor,
                            RULE_IMPORT_ADDED,
                            format!(
                                "import target in `{import_key}` became `{new_str}` (was null)"
                            ),
                        ));
                    }
                    (false, true) => {
                        // target → null is breaking (Major)
                        findings.push(pkg_json_finding(
                            Severity::Major,
                            RULE_IMPORT_CONDITION_CHANGED,
                            format!(
                                "import target in `{import_key}` became null (was `{old_str}`)"
                            ),
                        ));
                    }
                    _ => {
                        // Leaf → Leaf, both non-null = re-route
                        findings.push(pkg_json_finding(
                            Severity::Major,
                            RULE_IMPORT_REROUTE,
                            format!(
                                "import `{import_key}` re-routed from `{old_str}` to `{new_str}`"
                            ),
                        ));
                    }
                }
            }
        }
        (ConditionNode::Branch(old_conds), ConditionNode::Branch(new_conds)) => {
            for (cond_key, old_child) in old_conds {
                if let Some(new_child) = new_conds.get(cond_key) {
                    diff_import_condition_nodes(import_key, old_child, new_child, findings);
                } else {
                    let was_target = format_condition_target(old_child);
                    findings.push(pkg_json_finding(
                        Severity::Major,
                        RULE_IMPORT_CONDITION_CHANGED,
                        format!(
                            "condition `{cond_key}` removed from import `{import_key}` (was `{was_target}`)"
                        ),
                    ));
                }
            }
            // New conditions in new — additive, no finding (matches exports semantics)
        }
        // Branch ↔ Leaf shape flip: classified as condition-changed
        _ => {
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_IMPORT_CONDITION_CHANGED,
                format!("import `{import_key}` condition structure changed"),
            ));
        }
    }
}

/// Compare two parsed ImportsMaps and produce findings.
///
/// Mirrors `compare_exports_maps` shape: wildcard refinement detection first,
/// then removed keys (or pattern-refined where matched), then added keys,
/// then shared keys via `diff_import_condition_nodes`.
fn compare_imports_maps(old_map: &ImportsMap, new_map: &ImportsMap) -> Vec<Finding> {
    let mut findings = Vec::new();

    let old_wildcards: Vec<&String> =
        old_map.keys().filter(|k| k.contains('*')).collect();
    let new_wildcards: Vec<&String> =
        new_map.keys().filter(|k| k.contains('*')).collect();
    let mut matched_old: std::collections::HashSet<&str> =
        std::collections::HashSet::new();
    let mut matched_new: std::collections::HashSet<&str> =
        std::collections::HashSet::new();

    // Wildcard refinement detection (mirror of GAP-15-08 in exports_diff)
    for old_wc in &old_wildcards {
        if new_map.contains_key(old_wc.as_str()) {
            continue;
        }
        let old_base = strip_wildcard_to_base(old_wc);
        for new_wc in &new_wildcards {
            if old_map.contains_key(new_wc.as_str()) {
                continue;
            }
            let new_base = strip_wildcard_to_base(new_wc);
            if old_base == new_base {
                matched_old.insert(old_wc.as_str());
                matched_new.insert(new_wc.as_str());
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_IMPORT_PATTERN_REFINED,
                    format!(
                        "import pattern `{}` refined to `{}` — consumers using the old pattern may break",
                        old_wc, new_wc
                    ),
                ));
                break;
            }
        }
    }

    // Removed import keys
    for key in old_map.keys() {
        if new_map.contains_key(key) {
            continue;
        }
        if matched_old.contains(key.as_str()) {
            continue;
        }
        findings.push(pkg_json_finding(
            Severity::Major,
            RULE_IMPORT_REMOVED,
            format!("import `{key}` removed from package.json `imports`"),
        ));
    }

    // Added import keys
    for key in new_map.keys() {
        if old_map.contains_key(key) {
            continue;
        }
        if matched_new.contains(key.as_str()) {
            continue;
        }
        findings.push(pkg_json_finding(
            Severity::Minor,
            RULE_IMPORT_ADDED,
            format!("import `{key}` added to package.json `imports`"),
        ));
    }

    // Shared import keys: recurse on condition trees.
    // Iterate via Deref-backed `.iter()` because ImportsMap is a newtype and
    // `&ImportsMap` does not implement IntoIterator natively.
    for (key, old_entry) in old_map.iter() {
        if let Some(new_entry) = new_map.get(key) {
            diff_import_condition_nodes(key, &old_entry.node, &new_entry.node, &mut findings);
        }
    }

    findings
}

/// Top-level orchestrator: compare the `imports` field of two package.json values.
///
/// 4 cases:
/// - Both None: no findings (empty Vec — there is no packed-output fallback for imports).
/// - None → Some: every key in new becomes RULE_IMPORT_ADDED Minor.
/// - Some → None: every key in old becomes RULE_IMPORT_REMOVED Major.
/// - Some → Some: parse both and delegate to compare_imports_maps.
pub fn compare_imports_diff(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
) -> Vec<Finding> {
    let old_imports = old_pkg.get("imports");
    let new_imports = new_pkg.get("imports");

    match (old_imports, new_imports) {
        (None, None) => Vec::new(),
        (None, Some(new_val)) => {
            let new_map = parse_imports_map(new_val);
            new_map
                .keys()
                .map(|key| {
                    pkg_json_finding(
                        Severity::Minor,
                        RULE_IMPORT_ADDED,
                        format!("import `{key}` added to package.json `imports`"),
                    )
                })
                .collect()
        }
        (Some(old_val), None) => {
            let old_map = parse_imports_map(old_val);
            old_map
                .keys()
                .map(|key| {
                    pkg_json_finding(
                        Severity::Major,
                        RULE_IMPORT_REMOVED,
                        format!("import `{key}` removed from package.json `imports`"),
                    )
                })
                .collect()
        }
        (Some(old_val), Some(new_val)) => {
            let old_map = parse_imports_map(old_val);
            let new_map = parse_imports_map(new_val);
            compare_imports_maps(&old_map, &new_map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_imports_map_drops_non_hash_keys() {
        let v = json!({ ".": "./a.js", "foo": "./b.js", "#valid": "./c.js" });
        let m = parse_imports_map(&v);
        assert_eq!(m.len(), 1);
        assert!(m.contains_key("#valid"));
    }

    #[test]
    fn parse_imports_map_handles_null_and_arrays_and_strings() {
        assert!(parse_imports_map(&serde_json::Value::Null).is_empty());
        assert!(parse_imports_map(&json!(["./a.js"])).is_empty());
        assert!(parse_imports_map(&json!("./a.js")).is_empty());
    }

    #[test]
    fn parse_imports_map_normalizes_conditions() {
        let v = json!({
            "#a": "./a.js",
            "#b": { "node": "./n.js", "default": "./d.js" },
            "#c": null,
        });
        let m = parse_imports_map(&v);
        assert_eq!(m.len(), 3);
        assert!(matches!(m.get("#a").unwrap().node, ConditionNode::Leaf(Some(_))));
        assert!(matches!(m.get("#b").unwrap().node, ConditionNode::Branch(_)));
        assert!(matches!(m.get("#c").unwrap().node, ConditionNode::Leaf(None)));
    }

    #[test]
    fn compare_imports_diff_added_fires_minor() {
        let old = json!({});
        let new = json!({ "imports": { "#a": "./a.js" } });
        let findings = compare_imports_diff(&old, &new);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RULE_IMPORT_ADDED);
        assert_eq!(findings[0].severity, Severity::Minor);
    }

    #[test]
    fn compare_imports_diff_removed_fires_major() {
        let old = json!({ "imports": { "#a": "./a.js" } });
        let new = json!({});
        let findings = compare_imports_diff(&old, &new);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RULE_IMPORT_REMOVED);
        assert_eq!(findings[0].severity, Severity::Major);
    }

    #[test]
    fn compare_imports_diff_reroute_fires_major() {
        let old = json!({ "imports": { "#a": "./a.js" } });
        let new = json!({ "imports": { "#a": "./b.js" } });
        let findings = compare_imports_diff(&old, &new);
        assert!(findings.iter().any(|f| f.rule == RULE_IMPORT_REROUTE
            && f.severity == Severity::Major));
    }

    #[test]
    fn compare_imports_diff_condition_removed_fires_major() {
        let old = json!({ "imports": { "#a": { "node": "./n.js", "default": "./d.js" } } });
        let new = json!({ "imports": { "#a": { "default": "./d.js" } } });
        let findings = compare_imports_diff(&old, &new);
        assert!(findings.iter().any(|f| f.rule == RULE_IMPORT_CONDITION_CHANGED
            && f.severity == Severity::Major));
    }

    #[test]
    fn compare_imports_diff_pattern_refined_fires_major() {
        let old = json!({ "imports": { "#i/*": "./src/*.js" } });
        let new = json!({ "imports": { "#i/*.js": "./src/*.js" } });
        let findings = compare_imports_diff(&old, &new);
        assert!(findings.iter().any(|f| f.rule == RULE_IMPORT_PATTERN_REFINED
            && f.severity == Severity::Major));
    }

    #[test]
    fn compare_imports_diff_null_to_target_is_minor_added() {
        let old = json!({ "imports": { "#a": null } });
        let new = json!({ "imports": { "#a": "./a.js" } });
        let findings = compare_imports_diff(&old, &new);
        assert!(findings.iter().any(|f| f.rule == RULE_IMPORT_ADDED
            && f.severity == Severity::Minor));
    }

    #[test]
    fn compare_imports_diff_target_to_null_is_major_condition_changed() {
        let old = json!({ "imports": { "#a": "./a.js" } });
        let new = json!({ "imports": { "#a": null } });
        let findings = compare_imports_diff(&old, &new);
        assert!(findings.iter().any(|f| f.rule == RULE_IMPORT_CONDITION_CHANGED
            && f.severity == Severity::Major));
    }
}
