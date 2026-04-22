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

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::types::{Finding, Severity};

use super::subtype::{
    RULE_CJS_TO_ESM, RULE_CONDITION_REMOVED, RULE_CONDITION_TARGET_CHANGED,
    RULE_EXPORTS_FIELD_ADDED, RULE_ESM_TO_CJS, RULE_IMPORTABLE_FILE_REMOVED, RULE_SUBPATH_ADDED,
    RULE_SUBPATH_PATTERN_CHANGED, RULE_SUBPATH_REMOVED, RULE_TYPES_CONDITION_REMOVED,
    RULE_TYPES_RUNTIME_MISMATCH,
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

// ─── Packed output fallback scanner ──────────────────────────────────────────

/// Importable file extensions for packed output fallback scanning (D-13).
const IMPORTABLE_EXTENSIONS: &[&str] = &[
    ".js", ".mjs", ".cjs", ".ts", ".mts", ".cts", ".d.ts", ".jsx", ".tsx", ".json",
];

/// Returns true if the given path has an importable extension (per D-13).
///
/// Checks `.d.ts` before `.ts` to avoid false positives from the shorter suffix match.
fn is_importable(path: &Path) -> bool {
    let name = match path.to_str() {
        Some(s) => s,
        None => return false,
    };
    // Check compound .d.ts extension first before iterating single-extension entries
    if name.ends_with(".d.ts") {
        return true;
    }
    IMPORTABLE_EXTENSIONS
        .iter()
        .any(|ext| name.ends_with(ext))
}

/// Recursively collect importable files from a package directory.
///
/// Returns relative paths (relative to `base`) as a `BTreeSet<String>` for
/// deterministic ordering. Skips `node_modules/` directories and dotfiles.
fn collect_files_recursive(base: &Path, current: &Path, files: &mut BTreeSet<String>) {
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Skip dotfiles (files or dirs starting with '.')
        if file_name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            // Skip node_modules
            if file_name == "node_modules" {
                continue;
            }
            collect_files_recursive(base, &path, files);
        } else if is_importable(&path) {
            if let Ok(rel) = path.strip_prefix(base) {
                if let Some(rel_str) = rel.to_str() {
                    // Normalize path separators to forward slash for cross-platform consistency
                    files.insert(rel_str.replace('\\', "/"));
                }
            }
        }
    }
}

/// Scan all importable files in a package root directory.
///
/// Returns a `BTreeSet<String>` of relative paths (relative to `pkg_root`).
/// Skips dotfiles, `node_modules/`, and files with non-importable extensions.
pub(crate) fn scan_importable_files(pkg_root: &Path) -> BTreeSet<String> {
    let mut files = BTreeSet::new();
    collect_files_recursive(pkg_root, pkg_root, &mut files);
    files
}

/// Compare importable files between old and new package roots (EXP-13).
///
/// Used when neither old nor new package has an `exports` field.
/// Per D-14:
/// - Removed files → `importable-file-removed` (Major)
/// - Added files → `subpath-added` (Minor)
fn compare_packed_output_fallback(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let old_files = scan_importable_files(old_root);
    let new_files = scan_importable_files(new_root);
    let mut findings = Vec::new();

    // Removed files (Major)
    for file in &old_files {
        if !new_files.contains(file) {
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_IMPORTABLE_FILE_REMOVED,
                format!("importable file `{file}` removed from package"),
            ));
        }
    }

    // Added files (Minor)
    for file in &new_files {
        if !old_files.contains(file) {
            findings.push(pkg_json_finding(
                Severity::Minor,
                RULE_SUBPATH_ADDED,
                format!("importable file `{file}` added to package"),
            ));
        }
    }

    findings
}

/// Strip the wildcard `*` and any trailing extension to extract the base path prefix.
///
/// Examples:
/// - "./*" -> "./"
/// - "./*.js" -> "./"
/// - "./utils/*" -> "./utils/"
/// - "./utils/*.js" -> "./utils/"
///
/// Used for GAP-15-08 semantic matching of wildcard pattern refinements.
fn strip_wildcard_to_base(pattern: &str) -> &str {
    // Find the position of '*' and return everything before it
    match pattern.find('*') {
        Some(pos) => &pattern[..pos],
        None => pattern,
    }
}

/// Compare two parsed ExportsMaps and return findings.
///
/// Handles EXP-01, EXP-06, EXP-07 at the subpath level; delegates to
/// `diff_condition_nodes` for EXP-03, EXP-04, EXP-05.
fn compare_exports_maps(old_map: &ExportsMap, new_map: &ExportsMap) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Collect wildcard patterns for semantic matching
    let old_wildcards: Vec<&String> = old_map.keys().filter(|k| k.contains('*')).collect();
    let new_wildcards: Vec<&String> = new_map.keys().filter(|k| k.contains('*')).collect();
    let mut matched_old_wildcards: std::collections::HashSet<&str> =
        std::collections::HashSet::new();
    let mut matched_new_wildcards: std::collections::HashSet<&str> =
        std::collections::HashSet::new();

    // GAP-15-08: Check for wildcard pattern refinements
    // e.g., "./*" removed and "./*.js" added = pattern-changed, not removed+added
    for old_wc in &old_wildcards {
        if new_map.contains_key(old_wc.as_str()) {
            continue; // Pattern exists in both — will be diffed below
        }
        // Check if there's a related new wildcard pattern
        let old_base = strip_wildcard_to_base(old_wc);
        for new_wc in &new_wildcards {
            if old_map.contains_key(new_wc.as_str()) {
                continue; // This new pattern also exists in old — not a replacement
            }
            let new_base = strip_wildcard_to_base(new_wc);
            if old_base == new_base {
                // Related refinement detected
                matched_old_wildcards.insert(old_wc.as_str());
                matched_new_wildcards.insert(new_wc.as_str());
                findings.push(pkg_json_finding(
                    Severity::Major,
                    RULE_SUBPATH_PATTERN_CHANGED,
                    format!(
                        "subpath pattern `{}` changed to `{}` — consumers using the old pattern may break",
                        old_wc, new_wc
                    ),
                ));
                break; // Only match one new pattern per old pattern
            }
        }
    }

    // Removed subpaths (EXP-01, EXP-06)
    for subpath in old_map.keys() {
        if new_map.contains_key(subpath) {
            continue;
        }
        if matched_old_wildcards.contains(subpath.as_str()) {
            continue; // Already reported as pattern-changed
        }
        if subpath.contains('*') {
            // EXP-06: wildcard pattern removed with no related replacement
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

    // Added subpaths (EXP-07)
    for subpath in new_map.keys() {
        if old_map.contains_key(subpath) {
            continue;
        }
        if matched_new_wildcards.contains(subpath.as_str()) {
            continue; // Already reported as part of pattern-changed
        }
        findings.push(pkg_json_finding(
            Severity::Minor,
            RULE_SUBPATH_ADDED,
            format!("subpath `{subpath}` added to exports"),
        ));
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
/// `old_root` and `new_root` are the package root directories used for fallback
/// file scanning when neither package has an `exports` field (EXP-13).
///
/// Handles 4 cases:
/// - Both None: fallback to packed output file diff (EXP-13)
/// - Old None, New Some: EXP-02 exports-field-added (with file listing from old_root)
/// - Old Some, New None: EXP-01 for every subpath in old exports
/// - Both Some: parse and compare full ExportsMaps
pub fn compare_exports_diff(
    old_pkg: &serde_json::Value,
    new_pkg: &serde_json::Value,
    old_root: &Path,
    new_root: &Path,
) -> Vec<Finding> {
    let old_exports = old_pkg.get("exports");
    let new_exports = new_pkg.get("exports");

    match (old_exports, new_exports) {
        (None, None) => {
            // EXP-13: no exports field in either version — fallback to packed output file diff
            compare_packed_output_fallback(old_root, new_root)
        }

        (None, Some(_new_val)) => {
            // EXP-02: exports field added — previously importable paths are now encapsulated
            // Per D-15: enumerate old_root importable files for the message, cap at 10
            let old_files = scan_importable_files(old_root);
            let message = if old_files.is_empty() {
                "package.json `exports` field added, encapsulating deep imports".to_string()
            } else {
                let file_list: Vec<&String> = old_files.iter().collect();
                let (shown, rest) = if file_list.len() > 10 {
                    (&file_list[..10], file_list.len() - 10)
                } else {
                    (&file_list[..], 0)
                };
                let listed = shown
                    .iter()
                    .map(|f| format!("`{f}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                if rest > 0 {
                    format!(
                        "package.json `exports` field added, encapsulating previously importable paths: {listed} and {rest} more"
                    )
                } else {
                    format!(
                        "package.json `exports` field added, encapsulating previously importable paths: {listed}"
                    )
                }
            };
            vec![pkg_json_finding(Severity::Major, RULE_EXPORTS_FIELD_ADDED, message)]
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
        let findings = diff_with_empty_dirs(&old, &new);
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
        let findings = diff_with_empty_dirs(&old, &new);
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
        let findings = diff_with_empty_dirs(&old, &new);
        let esm_to_cjs: Vec<_> = findings.iter().filter(|f| f.rule == RULE_ESM_TO_CJS).collect();
        assert_eq!(esm_to_cjs.len(), 1, "expected 1 esm-to-cjs finding, got: {:?}", findings);
        assert_eq!(esm_to_cjs[0].severity, Severity::Major);
    }

    #[test]
    fn test_exp10_types_runtime_mismatch_dcts_with_import() {
        // EXP-10: types -> .d.cts (CJS types) but only import condition (ESM) -> mismatch
        let new = make_pkg(r#"{"exports": {".": {"types": "./index.d.cts", "import": "./index.mjs"}}}"#);
        let old = make_pkg(r#"{"exports": {".": {"types": "./index.d.cts", "import": "./index.mjs"}}}"#);
        let findings = diff_with_empty_dirs(&old, &new);
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
        let findings = diff_with_empty_dirs(&old, &new);
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

    /// Helper: call compare_exports_diff with empty temp directories as pkg roots.
    /// Used for tests that exercise the exports-field-based diffing (not filesystem scanning).
    fn diff_with_empty_dirs(
        old_pkg: &serde_json::Value,
        new_pkg: &serde_json::Value,
    ) -> Vec<Finding> {
        let old_dir = tempfile::tempdir().unwrap();
        let new_dir = tempfile::tempdir().unwrap();
        compare_exports_diff(old_pkg, new_pkg, old_dir.path(), new_dir.path())
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
        let findings = diff_with_empty_dirs(&old, &new);
        assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
        assert_eq!(findings[0].rule, RULE_SUBPATH_REMOVED);
        assert_eq!(findings[0].severity, Severity::Major);
        assert!(findings[0].message.contains("./utils"));
    }

    #[test]
    fn test_exp02_exports_field_added() {
        let old = make_pkg(r#"{"name": "my-pkg"}"#);
        let new = make_pkg(r#"{"name": "my-pkg", "exports": {".": "./index.js"}}"#);
        // Use empty dirs — no old files, so message uses simpler form
        let findings = diff_with_empty_dirs(&old, &new);
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
        let findings = diff_with_empty_dirs(&old, &new);
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
        let findings = diff_with_empty_dirs(&old, &new);
        assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
        assert_eq!(findings[0].rule, RULE_CONDITION_TARGET_CHANGED);
        assert_eq!(findings[0].severity, Severity::Major);
    }

    #[test]
    fn test_exp05_types_condition_removed_no_double_fire() {
        let old = make_pkg(r#"{"exports": {".": {"types": "./index.d.ts", "import": "./index.mjs"}}}"#);
        let new = make_pkg(r#"{"exports": {".": {"import": "./index.mjs"}}}"#);
        let findings = diff_with_empty_dirs(&old, &new);
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
        let findings = diff_with_empty_dirs(&old, &new);
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
        let findings = diff_with_empty_dirs(&old, &new);
        assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
        assert_eq!(findings[0].rule, RULE_SUBPATH_ADDED);
        assert_eq!(findings[0].severity, Severity::Minor);
        assert!(findings[0].message.contains("./newpath"));
    }

    #[test]
    fn test_unchanged_exports_no_findings() {
        let old = make_pkg(r#"{"exports": {".": {"import": "./index.mjs", "require": "./index.cjs"}}}"#);
        let new = make_pkg(r#"{"exports": {".": {"import": "./index.mjs", "require": "./index.cjs"}}}"#);
        let findings = diff_with_empty_dirs(&old, &new);
        assert!(findings.is_empty(), "expected no findings for unchanged exports, got: {:?}", findings);
    }

    // ─── GAP-15-08: Wildcard pattern semantic matching ────────────────────────

    #[test]
    fn test_gap15_08_wildcard_refinement() {
        // Old has "./*", new has "./*.js" — pattern refinement, single finding
        let old = make_pkg(r#"{"exports": {"./*": "./features/*.js", ".": "./index.js"}}"#);
        let new = make_pkg(r#"{"exports": {"./*.js": "./features/*.js", ".": "./index.js"}}"#);
        let findings = diff_with_empty_dirs(&old, &new);
        // Verify single subpath-pattern-changed with "changed to"
        let pattern_changed: Vec<_> = findings
            .iter()
            .filter(|f| f.rule == RULE_SUBPATH_PATTERN_CHANGED)
            .collect();
        assert_eq!(
            pattern_changed.len(),
            1,
            "expected 1 subpath-pattern-changed finding, got: {:?}",
            findings
        );
        assert!(
            pattern_changed[0].message.contains("changed to"),
            "message should contain 'changed to'; got: {}",
            pattern_changed[0].message
        );
        assert!(
            pattern_changed[0].message.contains("./*"),
            "message should mention old pattern; got: {}",
            pattern_changed[0].message
        );
        assert!(
            pattern_changed[0].message.contains("./*.js"),
            "message should mention new pattern; got: {}",
            pattern_changed[0].message
        );
        // Verify NO subpath-removed or subpath-added findings
        assert!(
            findings.iter().all(|f| f.rule != RULE_SUBPATH_REMOVED),
            "subpath-removed should NOT fire for wildcard refinement, got: {:?}",
            findings
        );
        assert!(
            findings.iter().all(|f| f.rule != RULE_SUBPATH_ADDED),
            "subpath-added should NOT fire for wildcard refinement, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_gap15_08_wildcard_unrelated() {
        // Old has "./*", new has "./utils/*.js" — different base paths, separate removed+added
        let old = make_pkg(r#"{"exports": {"./*": "./features/*.js", ".": "./index.js"}}"#);
        let new = make_pkg(r#"{"exports": {"./utils/*.js": "./utils/*.js", ".": "./index.js"}}"#);
        let findings = diff_with_empty_dirs(&old, &new);
        let pattern_changed: Vec<_> = findings
            .iter()
            .filter(|f| f.rule == RULE_SUBPATH_PATTERN_CHANGED)
            .collect();
        let subpath_added: Vec<_> = findings
            .iter()
            .filter(|f| f.rule == RULE_SUBPATH_ADDED)
            .collect();
        // ./* removed -> subpath-pattern-changed (no replacement found)
        assert_eq!(
            pattern_changed.len(),
            1,
            "expected 1 subpath-pattern-changed for unrelated wildcard removal, got: {:?}",
            findings
        );
        assert!(
            pattern_changed[0].message.contains("removed"),
            "removal message should say 'removed'; got: {}",
            pattern_changed[0].message
        );
        // ./utils/*.js added -> subpath-added
        assert_eq!(
            subpath_added.len(),
            1,
            "expected 1 subpath-added for new unrelated wildcard, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_gap15_08_wildcard_both_present() {
        // Old and new both have "./*" — no pattern-changed finding (just normal condition diffing)
        let old = make_pkg(r#"{"exports": {"./*": "./features/*.js", ".": "./index.js"}}"#);
        let new = make_pkg(r#"{"exports": {"./*": "./features/*.js", ".": "./index.js"}}"#);
        let findings = diff_with_empty_dirs(&old, &new);
        assert!(
            findings.iter().all(|f| f.rule != RULE_SUBPATH_PATTERN_CHANGED),
            "subpath-pattern-changed should NOT fire when pattern exists in both, got: {:?}",
            findings
        );
        assert!(
            findings.is_empty(),
            "expected no findings when wildcard unchanged, got: {:?}",
            findings
        );
    }

    // ─── Task 2 TDD: Packed output fallback scanner tests ─────────────────────

    #[test]
    fn test_exp13_importable_file_removed() {
        use tempfile::tempdir;
        use std::fs;

        // Old dir: lib/index.js, lib/utils.js
        let old_dir = tempdir().unwrap();
        let lib_dir = old_dir.path().join("lib");
        fs::create_dir_all(&lib_dir).unwrap();
        fs::write(lib_dir.join("index.js"), "").unwrap();
        fs::write(lib_dir.join("utils.js"), "").unwrap();

        // New dir: lib/index.js only
        let new_dir = tempdir().unwrap();
        let new_lib_dir = new_dir.path().join("lib");
        fs::create_dir_all(&new_lib_dir).unwrap();
        fs::write(new_lib_dir.join("index.js"), "").unwrap();

        let old_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
        let new_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
        let findings = compare_exports_diff(&old_pkg, &new_pkg, old_dir.path(), new_dir.path());

        let removed: Vec<_> = findings.iter().filter(|f| f.rule == RULE_IMPORTABLE_FILE_REMOVED).collect();
        assert_eq!(removed.len(), 1, "expected 1 importable-file-removed finding, got: {:?}", findings);
        assert_eq!(removed[0].severity, Severity::Major);
        assert!(removed[0].message.contains("utils.js"), "message should contain utils.js, got: {}", removed[0].message);
    }

    #[test]
    fn test_exp13_added_file_produces_subpath_added() {
        use tempfile::tempdir;
        use std::fs;

        // Old dir: lib/index.js only
        let old_dir = tempdir().unwrap();
        let old_lib = old_dir.path().join("lib");
        fs::create_dir_all(&old_lib).unwrap();
        fs::write(old_lib.join("index.js"), "").unwrap();

        // New dir: lib/index.js + lib/new-feature.js
        let new_dir = tempdir().unwrap();
        let new_lib = new_dir.path().join("lib");
        fs::create_dir_all(&new_lib).unwrap();
        fs::write(new_lib.join("index.js"), "").unwrap();
        fs::write(new_lib.join("new-feature.js"), "").unwrap();

        let old_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
        let new_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
        let findings = compare_exports_diff(&old_pkg, &new_pkg, old_dir.path(), new_dir.path());

        let added: Vec<_> = findings.iter().filter(|f| f.rule == RULE_SUBPATH_ADDED).collect();
        assert_eq!(added.len(), 1, "expected 1 subpath-added finding, got: {:?}", findings);
        assert_eq!(added[0].severity, Severity::Minor);
    }

    #[test]
    fn test_exp13_dotfiles_skipped() {
        use tempfile::tempdir;
        use std::fs;

        let old_dir = tempdir().unwrap();
        fs::write(old_dir.path().join(".hidden"), "").unwrap();
        fs::write(old_dir.path().join("index.js"), "").unwrap();

        let new_dir = tempdir().unwrap();
        fs::write(new_dir.path().join("index.js"), "").unwrap();
        // .hidden removed from new — should NOT produce finding since dotfiles are skipped

        let old_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
        let new_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
        let findings = compare_exports_diff(&old_pkg, &new_pkg, old_dir.path(), new_dir.path());

        // No findings because .hidden is skipped
        assert!(findings.is_empty(), "expected no findings (dotfiles skipped), got: {:?}", findings);
    }

    #[test]
    fn test_exp13_only_importable_extensions_included() {
        use tempfile::tempdir;
        use std::fs;

        let old_dir = tempdir().unwrap();
        fs::write(old_dir.path().join("README.md"), "").unwrap();
        fs::write(old_dir.path().join("notes.txt"), "").unwrap();
        fs::write(old_dir.path().join("index.js"), "").unwrap();

        let new_dir = tempdir().unwrap();
        fs::write(new_dir.path().join("index.js"), "").unwrap();
        // README.md and notes.txt removed from new — should NOT produce findings

        let old_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
        let new_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
        let findings = compare_exports_diff(&old_pkg, &new_pkg, old_dir.path(), new_dir.path());

        // No findings because .md and .txt are not importable
        assert!(findings.is_empty(), "expected no findings (.md/.txt not importable), got: {:?}", findings);
    }

    #[test]
    fn test_exp02_enhanced_with_file_listing() {
        use tempfile::tempdir;
        use std::fs;

        // Old dir: lib/index.js, lib/utils.js, lib/helper.js
        let old_dir = tempdir().unwrap();
        let old_lib = old_dir.path().join("lib");
        fs::create_dir_all(&old_lib).unwrap();
        fs::write(old_lib.join("index.js"), "").unwrap();
        fs::write(old_lib.join("utils.js"), "").unwrap();
        fs::write(old_lib.join("helper.js"), "").unwrap();

        let new_dir = tempdir().unwrap();

        // Old has no exports, new adds exports
        let old_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
        let new_pkg = make_pkg(r#"{"name": "my-pkg", "exports": {".": "./index.js"}}"#);
        let findings = compare_exports_diff(&old_pkg, &new_pkg, old_dir.path(), new_dir.path());

        let exp02: Vec<_> = findings.iter().filter(|f| f.rule == RULE_EXPORTS_FIELD_ADDED).collect();
        assert_eq!(exp02.len(), 1, "expected 1 exports-field-added finding, got: {:?}", findings);
        // Message should include file listing
        assert!(
            exp02[0].message.contains("index.js") || exp02[0].message.contains("encapsulating"),
            "message should mention files, got: {}",
            exp02[0].message
        );
    }

    #[test]
    fn test_scan_importable_files_basic() {
        use tempfile::tempdir;
        use std::fs;

        let dir = tempdir().unwrap();
        fs::write(dir.path().join("index.js"), "").unwrap();
        fs::write(dir.path().join("types.d.ts"), "").unwrap();
        fs::write(dir.path().join("module.mjs"), "").unwrap();
        fs::write(dir.path().join("README.md"), "").unwrap(); // not importable

        let files = scan_importable_files(dir.path());
        assert!(files.contains("index.js"), "should include index.js");
        assert!(files.contains("types.d.ts"), "should include types.d.ts");
        assert!(files.contains("module.mjs"), "should include module.mjs");
        assert!(!files.contains("README.md"), "should NOT include README.md");
    }

    // ── SEM-02: Direction-aware null handling in condition targets ────────────

    #[test]
    fn sem_02_null_to_string_fires_minor_addition() {
        // "types": null → "types": "./real.d.ts" is additive — Minor (subpath-added)
        let old = make_pkg(r#"{"exports": {".": {"types": null, "default": "./index.js"}}}"#);
        let new = make_pkg(r#"{"exports": {".": {"types": "./real.d.ts", "default": "./index.js"}}}"#);
        let findings = diff_with_empty_dirs(&old, &new);
        assert!(
            !findings.iter().any(|f| f.rule == RULE_CONDITION_TARGET_CHANGED),
            "SEM-02: null→string should NOT fire condition-target-changed (Major); got: {:?}",
            findings
        );
        // Should fire a Minor finding (subpath-added or similar)
        assert!(
            findings.iter().any(|f| f.severity == Severity::Minor),
            "SEM-02: null→string should produce a Minor finding; got: {:?}",
            findings
        );
    }

    #[test]
    fn sem_02_string_to_null_fires_major() {
        // "types": "./real.d.ts" → "types": null is breaking — Major (condition-target-changed)
        let old = make_pkg(r#"{"exports": {".": {"types": "./real.d.ts", "default": "./index.js"}}}"#);
        let new = make_pkg(r#"{"exports": {".": {"types": null, "default": "./index.js"}}}"#);
        let findings = diff_with_empty_dirs(&old, &new);
        assert!(
            findings.iter().any(|f| f.rule == RULE_CONDITION_TARGET_CHANGED && f.severity == Severity::Major),
            "SEM-02: string→null should fire condition-target-changed Major; got: {:?}",
            findings
        );
    }
}
