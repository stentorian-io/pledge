// Exports map diff: detect breaking changes in package.json `exports` field.
//
// Export-map rules include:
// EXP-01: subpath removed (subpath-removed)
// EXP-02: exports field added, encapsulating deep imports (exports-field-added)
// EXP-03: condition removed from subpath (condition-removed)
// EXP-04: condition target file changed (condition-target-changed)
// EXP-05: types condition removed (types-condition-removed)
// EXP-06: wildcard subpath pattern changed (subpath-pattern-changed)
// EXP-07: new subpath added (subpath-added)
//
// Data model ExportsMap preserves exact JSON shape for precise diffing.
// Parser normalizes all 7 valid exports field shapes to ExportsMap.

use crate::core::findings::{Finding, Severity};
use crate::ecosystems::node::compare as package_compare;
use crate::ecosystems::node::compare::files::{scan_package_files, PackageFiles};
use crate::ecosystems::node::compare::rules::{
    RULE_CJS_TO_ESM, RULE_CONDITION_REMOVED, RULE_CONDITION_TARGET_CHANGED, RULE_ESM_TO_CJS,
    RULE_EXPORTS_FIELD_ADDED, RULE_IMPORTABLE_FILE_REMOVED, RULE_SUBPATH_ADDED,
    RULE_SUBPATH_PATTERN_CHANGED, RULE_SUBPATH_REMOVED, RULE_TYPES_CONDITION_REMOVED,
    RULE_TYPES_RUNTIME_MISMATCH,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

// ─── Module format detection ──────────────────────────────────────────────────

/// Module format for CJS/ESM migration detection (EXP-08, EXP-09, EXP-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModuleFormat {
    Cjs,
    Esm,
}

/// Detect module format from a condition key.
/// Priority: condition key (signal 1).
pub(crate) fn format_from_condition_key(key: &str) -> Option<ModuleFormat> {
    match key {
        "require" => Some(ModuleFormat::Cjs),
        "import" => Some(ModuleFormat::Esm),
        _ => None,
    }
}

/// Detect module format from a file path extension.
/// Priority: file extension (signal 2).
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
/// `.d.cts` = CJS, `.d.mts` = ESM, `.d.ts` = format-neutral (per ).
pub(crate) fn types_format_from_extension(path: &str) -> Option<ModuleFormat> {
    if path.ends_with(".d.cts") {
        return Some(ModuleFormat::Cjs);
    }
    if path.ends_with(".d.mts") {
        return Some(ModuleFormat::Esm);
    }
    None // .d.ts is format-neutral.
}

// ─── Data model ───────────────────────────────────────────────────────────────

/// A recursive condition tree node.
///
/// A `Leaf` holds the target file path (or `None` for a null entry).
/// A `Branch` holds a map from condition name to child node.
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionNode {
    Leaf(Option<String>),
    Branch(indexmap::IndexMap<String, ConditionNode>),
}

// None means no matching branch; Some(None) is an explicit null blocker.
pub(crate) fn select_condition<'a>(
    node: &'a ConditionNode,
    active: &BTreeSet<String>,
) -> Option<Option<&'a str>> {
    match node {
        ConditionNode::Leaf(target) => Some(target.as_deref()),
        ConditionNode::Branch(branches) => branches
            .iter()
            .filter(|(key, _)| key.as_str() == "default" || active.contains(*key))
            .find_map(|(_, child)| select_condition(child, active)),
    }
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
pub(crate) fn parse_condition_node(value: &serde_json::Value) -> ConditionNode {
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
        let conditions: indexmap::IndexMap<String, ConditionNode> = obj
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
            arr.first().map(parse_exports_map).unwrap_or_default()
        }
        serde_json::Value::Object(obj) => parse_exports_object(obj),
        // Boolean, Number — treat as empty (no valid exports)
        _ => BTreeMap::new(),
    }
}

// ─── Finding factory ──────────────────────────────────────────────────────────

/// Create a Finding anchored to package.json line 1, col 1 (same pattern as metadata.rs ).
pub(crate) fn pkg_json_finding(severity: Severity, rule: &'static str, message: String) -> Finding {
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

/// Format a condition node target for use in finding messages (per ).
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
                let old_is_null = old_target.is_none();
                let new_is_null = new_target.is_none();
                let old_str = old_target.as_deref().unwrap_or("null").to_string();
                let new_str = new_target.as_deref().unwrap_or("null").to_string();

                // SEM-02: Direction-aware null handling.
                // - null → "./file.js": additive (condition target was absent, now present)
                //  → Minor, use RULE_SUBPATH_ADDED (not breaking for consumers)
                // - "./file.js" → null: breaking (condition target removed)
                //  → Major, RULE_CONDITION_TARGET_CHANGED (keeps existing behavior)
                match (old_is_null, new_is_null) {
                    (true, false) => {
                        findings.push(pkg_json_finding(
                            Severity::Minor,
                            RULE_SUBPATH_ADDED,
                            format!(
                                "condition target in subpath `{subpath}` became `{new_str}` (was null)"
                            ),
                        ));
                    }
                    (false, true) => {
                        findings.push(pkg_json_finding(
                            Severity::Major,
                            RULE_CONDITION_TARGET_CHANGED,
                            format!(
                                "condition target in subpath `{subpath}` became null (was `{old_str}`)"
                            ),
                        ));
                    }
                    _ => {
                        findings.push(pkg_json_finding(
                            Severity::Minor,
                            RULE_CONDITION_TARGET_CHANGED,
                            format!(
                                "condition target changed in subpath `{subpath}` from `{old_str}` to `{new_str}`"
                            ),
                        ));
                    }
                }
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
                Severity::Minor,
                RULE_CONDITION_TARGET_CHANGED,
                format!("condition structure changed in subpath `{subpath}`"),
            ));
        }
    }
}

/// Detect CJS-to-ESM and ESM-to-CJS migrations for a shared subpath (EXP-08, EXP-09).
///
///
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
            format!("ESM-to-CJS migration in subpath `{subpath}`: `import` condition removed"),
        ));
    }
}

/// Detect types/runtime module format mismatch in a subpath entry (EXP-10).
///
/// fires when types condition uses an explicit format-specific extension
/// (`.d.cts` for CJS, `.d.mts` for ESM) that conflicts with the runtime condition format.
/// `.d.ts` is format-neutral and never triggers this rule.
fn check_types_runtime_mismatch(subpath: &str, entry: &SubpathEntry, findings: &mut Vec<Finding>) {
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
    // Use condition key as primary signal (signal 1); file extension as secondary (signal 2).
    for cond_key in conds.keys() {
        // Determine runtime format: first try condition key, then extension of its target file
        let runtime_format =
            format_from_condition_key(cond_key).or_else(|| match conds.get(cond_key) {
                Some(ConditionNode::Leaf(Some(path))) => format_from_extension(path),
                _ => None,
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

/// Importable file extensions for packed output fallback scanning.
const IMPORTABLE_EXTENSIONS: &[&str] = &[
    ".js", ".mjs", ".cjs", ".ts", ".mts", ".cts", ".d.ts", ".jsx", ".tsx", ".json",
];

/// Returns true if the given path has an importable extension (per ).
///
/// Checks `.d.ts` before `.ts` to avoid false positives from the shorter suffix match.
fn is_importable(path: &Path) -> bool {
    let name = match path.to_str() {
        Some(s) => s,
        None => return false,
    };
    // Check compound.d.ts extension first before iterating single-extension entries
    if name.ends_with(".d.ts") {
        return true;
    }
    IMPORTABLE_EXTENSIONS.iter().any(|ext| name.ends_with(ext))
}

/// Scan importable paths, retaining diagnostics for unavailable subtrees.
fn scan_importable_files(pkg_root: &Path) -> PackageFiles {
    scan_package_files(pkg_root, is_importable)
}

/// Compare importable files between old and new package roots (EXP-13).
///
/// Used when neither old nor new package has an `exports` field.
///
/// - Removed files → `importable-file-removed` (Major)
/// - Added files → `subpath-added` (Minor)
fn compare_packed_output_fallback(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let old_files = scan_importable_files(old_root);
    let new_files = scan_importable_files(new_root);
    let mut findings = old_files.findings;
    findings.extend(new_files.findings);

    // Removed files (Major)
    for file in &old_files.paths {
        if !new_files.paths.contains(file) {
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_IMPORTABLE_FILE_REMOVED,
                format!("importable file `{file}` removed from package"),
            ));
        }
    }

    // Added files (Minor)
    for file in &new_files.paths {
        if !old_files.paths.contains(file) {
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
/// Used for semantic matching of wildcard pattern refinements.
pub(crate) fn strip_wildcard_to_base(pattern: &str) -> &str {
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

    // Check for wildcard pattern refinements
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
            let mut previous = Vec::new();
            let mut current = Vec::new();
            check_types_runtime_mismatch(subpath, old_entry, &mut previous);
            check_types_runtime_mismatch(subpath, new_entry, &mut current);
            findings.extend(
                current
                    .into_iter()
                    .filter(|finding| !previous.iter().any(|old| old.message == finding.message)),
            );
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
            // enumerate old_root importable files for the message, cap at 10
            let old_files = scan_importable_files(old_root);
            let message = if old_files.paths.is_empty() {
                "package.json `exports` field added, encapsulating deep imports".to_string()
            } else {
                let file_list: Vec<&String> = old_files.paths.iter().collect();
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
            let mut findings = old_files.findings;
            findings.push(pkg_json_finding(
                Severity::Major,
                RULE_EXPORTS_FIELD_ADDED,
                message,
            ));
            findings
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
            let mut findings = compare_exports_maps(&old_map, &new_map);
            findings.extend(package_compare::conditions::compare_priority(
                &old_map, &new_map, old_root, new_root,
            ));
            findings
        }
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
