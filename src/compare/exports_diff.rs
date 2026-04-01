// Exports map diff: detect breaking changes in package.json `exports` field.
//
// Covers EXP-01..07 (this plan) and EXP-08..13 (Plan 02):
//   EXP-01: subpath removed (subpath-removed)
//   EXP-02: exports field added, encapsulating deep imports (exports-field-added)
//   EXP-03: condition removed from subpath (condition-removed)
//   EXP-04: condition target file changed (condition-target-changed)
//   EXP-05: types condition removed (types-condition-removed)
//   EXP-06: wildcard subpath pattern changed (subpath-pattern-changed)
//   EXP-07: new subpath added (subpath-added)
//
// Data model per D-01: ExportsMap preserves exact JSON shape for precise diffing.
// Parser normalizes all 7 valid exports field shapes to ExportsMap.

use std::collections::BTreeMap;

use crate::types::{Finding, Severity};

use super::subtype::{
    RULE_CJS_TO_ESM, RULE_CONDITION_REMOVED, RULE_CONDITION_TARGET_CHANGED,
    RULE_EXPORTS_FIELD_ADDED, RULE_ESM_TO_CJS, RULE_SUBPATH_ADDED, RULE_SUBPATH_PATTERN_CHANGED,
    RULE_SUBPATH_REMOVED, RULE_TYPES_CONDITION_REMOVED, RULE_TYPES_RUNTIME_MISMATCH,
};

// ─── Module format detection ──────────────────────────────────────────────────

/// Module format for CJS/ESM migration detection (EXP-08, EXP-09, EXP-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModuleFormat {
    Cjs,
    Esm,
}

/// Detect module format from a condition key.
/// Priority: condition key (D-10, signal 1).
pub(crate) fn format_from_condition_key(key: &str) -> Option<ModuleFormat> {
    match key {
        "require" => Some(ModuleFormat::Cjs),
        "import" => Some(ModuleFormat::Esm),
        _ => None,
    }
}

/// Detect module format from a file path extension.
/// Priority: file extension (D-10, signal 2).
pub(crate) fn format_from_extension(path: &str) -> Option<ModuleFormat> {
    if path.ends_with(".cjs") {
        return Some(ModuleFormat::Cjs);
    }
    if path.ends_with(".mjs") {
        return Some(ModuleFormat::Esm);
    }
    None
}

/// Detect module format from a types declaration file extension.
/// `.d.cts` = CJS, `.d.mts` = ESM, `.d.ts` = format-neutral (per D-12).
pub(crate) fn types_format_from_extension(path: &str) -> Option<ModuleFormat> {
    if path.ends_with(".d.cts") {
        return Some(ModuleFormat::Cjs);
    }
    if path.ends_with(".d.mts") {
        return Some(ModuleFormat::Esm);
    }
    None // .d.ts is format-neutral per D-12
}

// ─── Data model ───────────────────────────────────────────────────────────────

/// A recursive condition tree node.
///
/// A `Leaf` holds the target file path (or `None` for a null entry).
/// A `Branch` holds a map from condition name to child node.
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionNode {
    Leaf(Option<String>),
    Branch(BTreeMap<String, ConditionNode>),
}

/// A single subpath entry in the exports map.
#[derive(Debug, Clone, PartialEq)]
pub struct SubpathEntry {
    pub node: ConditionNode,
}

/// Full exports map: keyed by subpath (e.g. ".", "./utils").
pub type ExportsMap = BTreeMap<String, SubpathEntry>;

// ─── Parser ───────────────────────────────────────────────────────────────────

/// Returns true if `key` is a subpath key (starts with '.').
fn is_subpath_key(key: &str) -> bool {
    key.starts_with('.')
}

/// Parse a `serde_json::Value` condition node into a `ConditionNode`.
fn parse_condition_node(value: &serde_json::Value) -> ConditionNode {
    match value {
        serde_json::Value::String(s) => ConditionNode::Leaf(Some(s.clone())),
        serde_json::Value::Null => ConditionNode::Leaf(None),
        serde_json::Value::Object(obj) => {
            let map = obj
                .iter()
                .map(|(k, v)| (k.clone(), parse_condition_node(v)))
                .collect();
            ConditionNode::Branch(map)
        }
        serde_json::Value::Array(arr) => {
            // Take first element per Node.js spec fallback semantics
            arr.first()
                .map(parse_condition_node)
                .unwrap_or(ConditionNode::Leaf(None))
        }
        // Boolean, Number — treat as null (no valid path)
        _ => ConditionNode::Leaf(None),
    }
}

/// Parse a `serde_json::Value` object into an `ExportsMap`.
///
/// Handles the ambiguity between:
/// - Subpath map: all keys start with "." — each key is a subpath
/// - Conditions-only object: no keys start with "." — implicit "." subpath wrapping
/// - Mixed: use first key as indicator (per Node.js spec ambiguity)
fn parse_exports_object(obj: &serde_json::Map<String, serde_json::Value>) -> ExportsMap {
    if obj.is_empty() {
        return BTreeMap::new();
    }

    // Determine if this is a subpath map or conditions-only object
    let first_key = obj.keys().next().unwrap();
    let is_subpath_map = is_subpath_key(first_key);

    if is_subpath_map {
        // Subpath map: each key is a subpath
        obj.iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    SubpathEntry {
                        node: parse_condition_node(v),
                    },
                )
            })
            .collect()
    } else {
        // Conditions-only object: wrap under implicit "." subpath
        let conditions: BTreeMap<String, ConditionNode> = obj
            .iter()
            .map(|(k, v)| (k.clone(), parse_condition_node(v)))
            .collect();
        let mut map = BTreeMap::new();
        map.insert(
            ".".to_string(),
            SubpathEntry {
                node: ConditionNode::Branch(conditions),
            },
        );
        map
    }
}

/// Parse the `exports` field value from package.json into a normalized `ExportsMap`.
///
/// Handles all 7 valid exports field shapes from the Node.js spec:
/// - Bare string: `"./index.js"` → `{ ".": Leaf(Some("./index.js")) }`
/// - Null: `null` → empty map
/// - Array: `["./index.js"]` → parse first element
/// - Subpath object: `{ ".": "./index.js", "./utils": "./utils.js" }` → subpath map
/// - Conditions-only object: `{ "import": "./index.mjs", "require": "./index.cjs" }` → `{ ".": Branch(...) }`
/// - Nested conditions: fully recursive
pub fn parse_exports_map(value: &serde_json::Value) -> ExportsMap {
    match value {
        serde_json::Value::String(s) => {
            let mut map = BTreeMap::new();
            map.insert(
                ".".to_string(),
                SubpathEntry {
                    node: ConditionNode::Leaf(Some(s.clone())),
                },
            );
            map
        }
        serde_json::Value::Null => BTreeMap::new(),
        serde_json::Value::Array(arr) => {
            // Take first element per Node.js spec fallback semantics
            arr.first()
                .map(parse_exports_map)
                .unwrap_or_default()
        }
        serde_json::Value::Object(obj) => parse_exports_object(obj),
        // Boolean, Number — treat as empty (no valid exports)
        _ => BTreeMap::new(),
    }
}

// ─── Finding factory ──────────────────────────────────────────────────────────

/// Create a Finding anchored to package.json line 1, col 1 (same pattern as metadata.rs D-08).
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

// ─── Diff helpers ─────────────────────────────────────────────────────────────

/// Format a condition node target for use in finding messages (per D-06).
///
/// Returns the leaf path string, or `"[conditions object]"` for branches.
fn format_condition_target(node: &ConditionNode) -> String {
    match node {
        ConditionNode::Leaf(Some(path)) => path.clone(),
        ConditionNode::Leaf(None) => "null".to_string(),
        ConditionNode::Branch(_) => "[conditions object]".to_string(),
    }
}

/// Recursively diff two condition node trees for a given subpath, appending findings.
///
/// Handles EXP-03, EXP-04, EXP-05:
/// - Leaf→Leaf with different targets → EXP-04 condition-target-changed
/// - Branch→Branch: removed conditions → EXP-03 or EXP-05; recurse shared ones
/// - Mismatched variant (Leaf↔Branch) → EXP-04 condition-target-changed (structural change)
fn diff_condition_nodes(
    subpath: &str,
    old: &ConditionNode,
    new: &ConditionNode,
    findings: &mut Vec<Finding>,
) {
    match (old, new) {
        (ConditionNode::Leaf(old_target), ConditionNode::Leaf(new_target)) => {
            if old_target != new_target {
                let old_str = old_target
                    .as_deref()
                    .unwrap_or("null")
                    .to_string();
                let new_str = new_target
                    .as_deref()
                    .unwrap_or("null")
                    .to_string();
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_CONDITION_TARGET_CHANGED,
                    format!(
                        "condition target changed in subpath `{subpath}` from `{old_str}` to `{new_str}`"
                    ),
                ));
            }
        }
        (ConditionNode::Branch(old_conds), ConditionNode::Branch(new_conds)) => {
            // Check for removed conditions (EXP-03, EXP-05)
            for (cond_key, old_child) in old_conds {
                if let Some(new_child) = new_conds.get(cond_key) {
                    // Condition exists in both — recurse
                    diff_condition_nodes(subpath, old_child, new_child, findings);
                } else {
                    // Condition removed
                    let was_target = format_condition_target(old_child);
                    if cond_key == "types" {
                        // EXP-05: types condition specifically — do NOT also emit EXP-03
                        findings.push(pkg_json_finding(
                            Severity::Major,
                            RULE_TYPES_CONDITION_REMOVED,
                            format!(
                                "condition `types` removed from subpath `{subpath}` (was `{was_target}`)"
                            ),
                        ));
                    } else {
                        // EXP-03: general condition removed
                        findings.push(pkg_json_finding(
                            Severity::Major,
                            RULE_CONDITION_REMOVED,
                            format!(
                                "condition `{cond_key}` removed from subpath `{subpath}` (was `{was_target}`)"
                            ),
                        ));
                    }
                }
            }
            // New conditions not in old — non-breaking additions (no finding)
        }
        // Mismatched variant: structural change → EXP-04
        _ => {
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_CONDITION_TARGET_CHANGED,
                format!("condition structure changed in subpath `{subpath}`"),
            ));
        }
    }
}

/// Detect CJS-to-ESM and ESM-to-CJS migrations for a shared subpath (EXP-08, EXP-09).
///
/// Per D-11:
/// - EXP-08 `cjs-to-esm`: old Branch has "require" key AND new Branch does NOT have "require"
/// - EXP-09 `esm-to-cjs`: old Branch has "import" key AND new Branch does NOT have "import"
///
/// Not triggered when both conditions still present but targets changed (that's EXP-04).
fn check_format_migration(
    subpath: &str,
    old_entry: &SubpathEntry,
    new_entry: &SubpathEntry,
    findings: &mut Vec<Finding>,
) {
    let (old_conds, new_conds) = match (&old_entry.node, &new_entry.node) {
        (ConditionNode::Branch(o), ConditionNode::Branch(n)) => (o, n),
        _ => return, // Only applies to Branch nodes
    };

    // EXP-08: require condition removed (CJS-to-ESM migration)
    if old_conds.contains_key("require") && !new_conds.contains_key("require") {
        findings.push(pkg_json_finding(
            Severity::Major,
            RULE_CJS_TO_ESM,
            format!(
                "CJS-to-ESM migration in subpath `{subpath}`: `require` condition removed (consumers using require() will fail)"
            ),
        ));
    }

    // EXP-09: import condition removed (ESM-to-CJS migration)
    if old_conds.contains_key("import") && !new_conds.contains_key("import") {
        findings.push(pkg_json_finding(
            Severity::Major,
            RULE_ESM_TO_CJS,
            format!(
                "ESM-to-CJS migration in subpath `{subpath}`: `import` condition removed"
            ),
        ));
    }
}

/// Detect types/runtime module format mismatch in a subpath entry (EXP-10).
///
/// Per D-12: fires when types condition uses an explicit format-specific extension
/// (`.d.cts` for CJS, `.d.mts` for ESM) that conflicts with the runtime condition format.
/// `.d.ts` is format-neutral and never triggers this rule.
fn check_types_runtime_mismatch(
    subpath: &str,
    entry: &SubpathEntry,
    findings: &mut Vec<Finding>,
) {
    let conds = match &entry.node {
        ConditionNode::Branch(c) => c,
        _ => return, // Only applies to Branch nodes
    };

    // Find the types condition leaf value
    let types_path = match conds.get("types") {
        Some(ConditionNode::Leaf(Some(path))) => path.as_str(),
        _ => return, // No types condition or null/branch types
    };

    // Check if types path uses explicit format extension
    let types_format = match types_format_from_extension(types_path) {
        Some(fmt) => fmt,
        None => return, // .d.ts is format-neutral — no mismatch possible
    };

    // Find runtime conditions and check for mismatch.
    // Use condition key as primary signal (D-10, signal 1); file extension as secondary (signal 2).
    for cond_key in conds.keys() {
        // Determine runtime format: first try condition key, then extension of its target file
        let runtime_format = format_from_condition_key(cond_key).or_else(|| {
            match conds.get(cond_key) {
                Some(ConditionNode::Leaf(Some(path))) => format_from_extension(path),
                _ => None,
            }
        });

        if let Some(runtime_format) = runtime_format {
            if runtime_format != types_format {
                let types_format_str = match types_format {
                    ModuleFormat::Cjs => "CJS",
                    ModuleFormat::Esm => "ESM",
                };
                let runtime_format_str = match runtime_format {
                    ModuleFormat::Cjs => "CJS",
                    ModuleFormat::Esm => "ESM",
                };
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_TYPES_RUNTIME_MISMATCH,
                    format!(
                        "types/runtime module format mismatch in subpath `{subpath}`: types condition points to `{types_path}` ({types_format_str}) but runtime condition is `{cond_key}` ({runtime_format_str})"
                    ),
                ));
                return; // Emit at most one mismatch per subpath
            }
        }
    }
}

/// Compare two parsed ExportsMaps and return findings.
///
/// Handles EXP-01, EXP-06, EXP-07 at the subpath level; delegates to
/// `diff_condition_nodes` for EXP-03, EXP-04, EXP-05.
fn compare_exports_maps(old_map: &ExportsMap, new_map: &ExportsMap) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Removed subpaths (EXP-01, EXP-06)
    for subpath in old_map.keys() {
        if !new_map.contains_key(subpath) {
            if subpath.contains('*') {
                // EXP-06: wildcard pattern removed — subpath-pattern-changed
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_SUBPATH_PATTERN_CHANGED,
                    format!("subpath pattern `{subpath}` removed from exports"),
                ));
            } else {
                // EXP-01: regular subpath removed
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_SUBPATH_REMOVED,
                    format!("subpath `{subpath}` removed from exports"),
                ));
            }
        }
    }

    // Added subpaths (EXP-07)
    for subpath in new_map.keys() {
        if !old_map.contains_key(subpath) {
            findings.push(pkg_json_finding(
                Severity::Minor,
                RULE_SUBPATH_ADDED,
                format!("subpath `{subpath}` added to exports"),
            ));
        }
    }

    // Shared subpaths: diff condition nodes, then check migration and mismatch
    for (subpath, old_entry) in old_map {
        if let Some(new_entry) = new_map.get(subpath) {
            diff_condition_nodes(subpath, &old_entry.node, &new_entry.node, &mut findings);
            check_format_migration(subpath, old_entry, new_entry, &mut findings);
            check_types_runtime_mismatch(subpath, new_entry, &mut findings);
        }
    }

    findings
}

/// Top-level entry point: compare the `exports` field of two package.json values.
///
/// Handles 4 cases:
/// - Both None: no findings (no exports field in either version)
/// - Old None, New Some: EXP-02 exports-field-added
/// - Old Some, New None: EXP-01 for every subpath in old exports
/// - Both Some: parse and compare full ExportsMaps
pub fn compare_exports_diff(old_pkg: &serde_json::Value, new_pkg: &serde_json::Value) -> Vec<Finding> {
    let old_exports = old_pkg.get("exports");
    let new_exports = new_pkg.get("exports");

    match (old_exports, new_exports) {
        (None, None) => Vec::new(),

        (None, Some(_new_val)) => {
            // EXP-02: exports field added — previously importable paths are now encapsulated
            vec![pkg_json_finding(
                Severity::Major,
                RULE_EXPORTS_FIELD_ADDED,
                "package.json `exports` field added, encapsulating previously importable paths"
                    .to_string(),
            )]
        }

        (Some(old_val), None) => {
            // Exports field removed — every subpath in old is now gone
            let old_map = parse_exports_map(old_val);
            old_map
                .keys()
                .map(|subpath| {
                    pkg_json_finding(
                        Severity::Major,
                        RULE_SUBPATH_REMOVED,
                        format!("subpath `{subpath}` removed from exports"),
                    )
                })
                .collect()
        }

        (Some(old_val), Some(new_val)) => {
            let old_map = parse_exports_map(old_val);
            let new_map = parse_exports_map(new_val);
            compare_exports_maps(&old_map, &new_map)
        }
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Task 1 TDD: CJS/ESM migration and types/runtime mismatch tests ───────

    #[test]
    fn test_exp08_cjs_to_esm_require_removed() {
        // EXP-08: old has require condition, new drops it -> cjs-to-esm finding
        let old = make_pkg(r#"{"exports": {".": {"require": "./index.cjs", "import": "./index.mjs"}}}"#);
        let new = make_pkg(r#"{"exports": {".": {"import": "./index.mjs"}}}"#);
        let findings = compare_exports_diff(&old, &new);
        let cjs_to_esm: Vec<_> = findings.iter().filter(|f| f.rule == RULE_CJS_TO_ESM).collect();
        assert_eq!(cjs_to_esm.len(), 1, "expected 1 cjs-to-esm finding, got: {:?}", findings);
        assert_eq!(cjs_to_esm[0].severity, Severity::Major);
        assert!(cjs_to_esm[0].message.contains("."));
    }

    #[test]
    fn test_exp08_no_cjs_to_esm_when_both_conditions_present() {
        // EXP-08 negative: both require and import exist in both versions, targets changed -> NO cjs-to-esm
        let old = make_pkg(r#"{"exports": {".": {"require": "./index.cjs", "import": "./index.mjs"}}}"#);
        let new = make_pkg(r#"{"exports": {".": {"require": "./dist/index.cjs", "import": "./dist/index.mjs"}}}"#);
        let findings = compare_exports_diff(&old, &new);
        assert!(
            findings.iter().all(|f| f.rule != RULE_CJS_TO_ESM),
            "cjs-to-esm should NOT fire when both conditions still present, got: {:?}", findings
        );
    }

    #[test]
    fn test_exp09_esm_to_cjs_import_removed() {
        // EXP-09: old has import condition only, new has only require -> esm-to-cjs finding
        let old = make_pkg(r#"{"exports": {".": {"import": "./index.mjs"}}}"#);
        let new = make_pkg(r#"{"exports": {".": {"require": "./index.cjs"}}}"#);
        let findings = compare_exports_diff(&old, &new);
        let esm_to_cjs: Vec<_> = findings.iter().filter(|f| f.rule == RULE_ESM_TO_CJS).collect();
        assert_eq!(esm_to_cjs.len(), 1, "expected 1 esm-to-cjs finding, got: {:?}", findings);
        assert_eq!(esm_to_cjs[0].severity, Severity::Major);
    }

    #[test]
    fn test_exp10_types_runtime_mismatch_dcts_with_import() {
        // EXP-10: types -> .d.cts (CJS types) but only import condition (ESM) -> mismatch
        let new = make_pkg(r#"{"exports": {".": {"types": "./index.d.cts", "import": "./index.mjs"}}}"#);
        let old = make_pkg(r#"{"exports": {".": {"types": "./index.d.cts", "import": "./index.mjs"}}}"#);
        let findings = compare_exports_diff(&old, &new);
        let mismatch: Vec<_> = findings.iter().filter(|f| f.rule == RULE_TYPES_RUNTIME_MISMATCH).collect();
        assert_eq!(mismatch.len(), 1, "expected 1 types-runtime-mismatch finding, got: {:?}", findings);
        assert_eq!(mismatch[0].severity, Severity::Major);
        assert!(mismatch[0].message.contains(".d.cts"));
    }

    #[test]
    fn test_exp10_no_mismatch_for_format_neutral_dts() {
        // EXP-10 negative: types -> .d.ts (format-neutral) with import -> NO mismatch
        let old = make_pkg(r#"{"exports": {".": {"types": "./index.d.ts", "import": "./index.mjs"}}}"#);
        let new = make_pkg(r#"{"exports": {".": {"types": "./index.d.ts", "import": "./index.mjs"}}}"#);
        let findings = compare_exports_diff(&old, &new);
        assert!(
            findings.iter().all(|f| f.rule != RULE_TYPES_RUNTIME_MISMATCH),
            "types-runtime-mismatch should NOT fire for .d.ts (format-neutral), got: {:?}", findings
        );
    }

    #[test]
    fn test_module_format_from_condition_key() {
        assert_eq!(format_from_condition_key("require"), Some(ModuleFormat::Cjs));
        assert_eq!(format_from_condition_key("import"), Some(ModuleFormat::Esm));
        assert_eq!(format_from_condition_key("default"), None);
        assert_eq!(format_from_condition_key("types"), None);
    }

    #[test]
    fn test_module_format_from_extension() {
        assert_eq!(format_from_extension("./index.cjs"), Some(ModuleFormat::Cjs));
        assert_eq!(format_from_extension("./index.mjs"), Some(ModuleFormat::Esm));
        assert_eq!(format_from_extension("./index.js"), None);
    }

    #[test]
    fn test_types_format_from_extension() {
        assert_eq!(types_format_from_extension("./index.d.cts"), Some(ModuleFormat::Cjs));
        assert_eq!(types_format_from_extension("./index.d.mts"), Some(ModuleFormat::Esm));
        assert_eq!(types_format_from_extension("./index.d.ts"), None);
    }

    fn make_pkg(json_str: &str) -> serde_json::Value {
        serde_json::from_str(json_str).unwrap()
    }

    // ─── Parser tests ─────────────────────────────────────────────────────────

    #[test]
    fn test_parse_bare_string() {
        let v = serde_json::Value::String("./index.js".to_string());
        let map = parse_exports_map(&v);
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get(".").unwrap().node,
            ConditionNode::Leaf(Some("./index.js".to_string()))
        );
    }

    #[test]
    fn test_parse_subpath_object() {
        let v = serde_json::json!({".": "./index.js", "./utils": "./utils.js"});
        let map = parse_exports_map(&v);
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get(".").unwrap().node,
            ConditionNode::Leaf(Some("./index.js".to_string()))
        );
        assert_eq!(
            map.get("./utils").unwrap().node,
            ConditionNode::Leaf(Some("./utils.js".to_string()))
        );
    }

    #[test]
    fn test_parse_conditions_only_object() {
        // No keys start with "." — treat as conditions for implicit "." subpath
        let v = serde_json::json!({"import": "./index.mjs", "require": "./index.cjs"});
        let map = parse_exports_map(&v);
        assert_eq!(map.len(), 1);
        let node = &map.get(".").unwrap().node;
        match node {
            ConditionNode::Branch(conds) => {
                assert!(conds.contains_key("import"));
                assert!(conds.contains_key("require"));
            }
            other => panic!("expected Branch, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_nested_conditions() {
        let v = serde_json::json!({
            ".": {
                "import": {
                    "types": "./index.d.mts",
                    "default": "./index.mjs"
                },
                "require": "./index.cjs"
            }
        });
        let map = parse_exports_map(&v);
        assert_eq!(map.len(), 1);
        let node = &map.get(".").unwrap().node;
        match node {
            ConditionNode::Branch(top) => {
                assert!(top.contains_key("import"));
                match top.get("import").unwrap() {
                    ConditionNode::Branch(inner) => {
                        assert!(inner.contains_key("types"));
                        assert!(inner.contains_key("default"));
                    }
                    other => panic!("expected nested Branch, got {:?}", other),
                }
            }
            other => panic!("expected Branch at top level, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_null_returns_empty() {
        let map = parse_exports_map(&serde_json::Value::Null);
        assert!(map.is_empty());
    }

    #[test]
    fn test_parse_array_takes_first_element() {
        let v = serde_json::json!(["./index.js", "./fallback.js"]);
        let map = parse_exports_map(&v);
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get(".").unwrap().node,
            ConditionNode::Leaf(Some("./index.js".to_string()))
        );
    }

    #[test]
    fn test_is_subpath_key() {
        assert!(is_subpath_key("."));
        assert!(is_subpath_key("./utils"));
        assert!(is_subpath_key("./*"));
        assert!(!is_subpath_key("import"));
        assert!(!is_subpath_key("require"));
        assert!(!is_subpath_key("types"));
        assert!(!is_subpath_key("default"));
    }

    // ─── Diff tests: EXP-01..07 ───────────────────────────────────────────────

    #[test]
    fn test_exp01_subpath_removed() {
        let old = make_pkg(r#"{"exports": {"./utils": "./utils.js", ".": "./index.js"}}"#);
        let new = make_pkg(r#"{"exports": {".": "./index.js"}}"#);
        let findings = compare_exports_diff(&old, &new);
        assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
        assert_eq!(findings[0].rule, RULE_SUBPATH_REMOVED);
        assert_eq!(findings[0].severity, Severity::Major);
        assert!(findings[0].message.contains("./utils"));
    }

    #[test]
    fn test_exp02_exports_field_added() {
        let old = make_pkg(r#"{"name": "my-pkg"}"#);
        let new = make_pkg(r#"{"name": "my-pkg", "exports": {".": "./index.js"}}"#);
        let findings = compare_exports_diff(&old, &new);
        assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
        assert_eq!(findings[0].rule, RULE_EXPORTS_FIELD_ADDED);
        assert_eq!(findings[0].severity, Severity::Major);
    }

    #[test]
    fn test_exp03_condition_removed() {
        // Removing the `require` condition fires both EXP-03 (condition-removed) and
        // EXP-08 (cjs-to-esm migration). Both are correct co-occurrences.
        let old = make_pkg(r#"{"exports": {".": {"import": "./index.mjs", "require": "./index.cjs"}}}"#);
        let new = make_pkg(r#"{"exports": {".": {"import": "./index.mjs"}}}"#);
        let findings = compare_exports_diff(&old, &new);
        // EXP-03: condition-removed must fire
        let condition_removed: Vec<_> = findings.iter().filter(|f| f.rule == RULE_CONDITION_REMOVED).collect();
        assert_eq!(condition_removed.len(), 1, "expected 1 condition-removed finding, got: {:?}", findings);
        assert_eq!(condition_removed[0].severity, Severity::Major);
        assert!(condition_removed[0].message.contains("require"));
        // EXP-08: cjs-to-esm migration also fires for this scenario
        let cjs_to_esm: Vec<_> = findings.iter().filter(|f| f.rule == RULE_CJS_TO_ESM).collect();
        assert_eq!(cjs_to_esm.len(), 1, "expected 1 cjs-to-esm finding, got: {:?}", findings);
    }

    #[test]
    fn test_exp04_condition_target_changed() {
        let old = make_pkg(r#"{"exports": {".": {"import": "./index.mjs"}}}"#);
        let new = make_pkg(r#"{"exports": {".": {"import": "./dist/index.mjs"}}}"#);
        let findings = compare_exports_diff(&old, &new);
        assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
        assert_eq!(findings[0].rule, RULE_CONDITION_TARGET_CHANGED);
        assert_eq!(findings[0].severity, Severity::Major);
    }

    #[test]
    fn test_exp05_types_condition_removed_no_double_fire() {
        let old = make_pkg(r#"{"exports": {".": {"types": "./index.d.ts", "import": "./index.mjs"}}}"#);
        let new = make_pkg(r#"{"exports": {".": {"import": "./index.mjs"}}}"#);
        let findings = compare_exports_diff(&old, &new);
        // Should fire EXP-05 (types-condition-removed) but NOT EXP-03 (condition-removed)
        assert_eq!(findings.len(), 1, "expected exactly 1 finding, got: {:?}", findings);
        assert_eq!(findings[0].rule, RULE_TYPES_CONDITION_REMOVED);
        assert_eq!(findings[0].severity, Severity::Major);
        // Verify no condition-removed double-fire
        assert!(
            findings.iter().all(|f| f.rule != RULE_CONDITION_REMOVED),
            "EXP-03 should not fire when EXP-05 fires"
        );
    }

    #[test]
    fn test_exp06_wildcard_pattern_changed() {
        let old = make_pkg(r#"{"exports": {"./*": "./features/*.js", ".": "./index.js"}}"#);
        let new = make_pkg(r#"{"exports": {".": "./index.js"}}"#);
        let findings = compare_exports_diff(&old, &new);
        assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
        assert_eq!(findings[0].rule, RULE_SUBPATH_PATTERN_CHANGED);
        assert_eq!(findings[0].severity, Severity::Major);
        // Verify it did NOT fire subpath-removed for the wildcard
        assert!(
            findings.iter().all(|f| f.rule != RULE_SUBPATH_REMOVED),
            "EXP-01 should not fire for wildcard patterns"
        );
    }

    #[test]
    fn test_exp07_subpath_added() {
        let old = make_pkg(r#"{"exports": {".": "./index.js"}}"#);
        let new = make_pkg(r#"{"exports": {".": "./index.js", "./newpath": "./newpath.js"}}"#);
        let findings = compare_exports_diff(&old, &new);
        assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
        assert_eq!(findings[0].rule, RULE_SUBPATH_ADDED);
        assert_eq!(findings[0].severity, Severity::Minor);
        assert!(findings[0].message.contains("./newpath"));
    }

    #[test]
    fn test_unchanged_exports_no_findings() {
        let old = make_pkg(r#"{"exports": {".": {"import": "./index.mjs", "require": "./index.cjs"}}}"#);
        let new = make_pkg(r#"{"exports": {".": {"import": "./index.mjs", "require": "./index.cjs"}}}"#);
        let findings = compare_exports_diff(&old, &new);
        assert!(findings.is_empty(), "expected no findings for unchanged exports, got: {:?}", findings);
    }
}
