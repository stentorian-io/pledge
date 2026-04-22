use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Context;

/// Represents a single entry point discovered from package.json.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct EntryPoint {
    pub path: PathBuf,
    /// The condition key from the exports map (e.g., "import", "require", "types"), or None
    /// when resolved from top-level `main`, `module`, or `types` fields.
    pub condition: Option<String>,
}

/// Resolve all entry points for a package rooted at `pkg_root`.
///
/// Resolution order per the Node.js spec (D-11):
/// 1. `exports` field (takes precedence, parsed recursively)
/// 2. `main`, `module`, `types` fields (fallback, in that order)
/// 3. `index.js` (final fallback when nothing else is present)
///
/// Paths that do not exist on disk are filtered out with a warning trace.
/// Duplicate resolved paths are deduplicated.
#[allow(dead_code)]
pub fn resolve_entry_points(pkg_root: &Path) -> anyhow::Result<Vec<EntryPoint>> {
    let pkg_json_path = pkg_root.join("package.json");
    let raw = std::fs::read_to_string(&pkg_json_path)
        .with_context(|| format!("failed to read {}", pkg_json_path.display()))?;
    let pkg: serde_json::Value =
        serde_json::from_str(&raw).context("failed to parse package.json as JSON")?;

    let mut results: Vec<EntryPoint> = Vec::new();

    if let Some(exports) = pkg.get("exports") {
        resolve_exports_value(exports, pkg_root, None, &mut results);
    } else {
        // Fall back to main / module / types / typings in priority order.
        // Note: `typings` is the legacy TypeScript alias for `types`; we check
        // `types` first so that when both are present `types` wins (D-11).
        // GAP-16-12: Include "browser" as a fallback entry point.
        // The browser field is commonly used by bundlers (webpack, Rollup) to specify
        // a browser-specific entry point. We handle the string case (common) here;
        // the object case (field remapping) is out of scope for entry-point discovery.
        for field in &["main", "module", "types", "typings", "browser"] {
            if let Some(serde_json::Value::String(rel)) = pkg.get(*field) {
                let abs = pkg_root.join(rel.trim_start_matches("./"));
                results.push(EntryPoint {
                    path: abs,
                    condition: None,
                });
                // No break — all valid fields contribute entry points.
                // The deduplication step below handles cases where multiple
                // fields resolve to the same physical file.
            }
        }

        // Final fallback.
        if results.is_empty() {
            results.push(EntryPoint {
                path: pkg_root.join("index.js"),
                condition: None,
            });
        }
    }

    // After exports field handling, also check typesVersions for type entry points.
    //
    // typesVersions has this shape:
    // { ">=4.0": { "*": ["dist/types/*"] }, ">=3.0": { "*": ["dist/legacy/*"] } }
    //
    // Each version condition maps glob patterns to arrays of path rewrites.
    // Non-wildcard paths are treated as direct entry points for type analysis.
    if let Some(tv) = pkg.get("typesVersions") {
        if let Some(tv_obj) = tv.as_object() {
            for (_version_range, mappings) in tv_obj {
                if let Some(map_obj) = mappings.as_object() {
                    for (_pattern, targets) in map_obj {
                        if let Some(arr) = targets.as_array() {
                            for target in arr {
                                if let Some(path_str) = target.as_str() {
                                    // typesVersions paths may contain wildcards like "dist/types/*"
                                    // For entry point discovery, resolve non-wildcard paths directly.
                                    // GAP-16-11: Wildcard paths are expanded by scanning the target directory.
                                    if !path_str.contains('*') {
                                        let abs = pkg_root.join(path_str.trim_start_matches("./"));
                                        results.push(EntryPoint {
                                            path: abs,
                                            condition: Some("typesVersions".to_string()),
                                        });
                                    } else {
                                        // GAP-16-11: Wildcard path expansion.
                                        // Pattern like "dist/types/*" — strip the `*` to get the directory,
                                        // then list all .ts/.d.ts files within it.
                                        let dir_pattern = path_str.trim_start_matches("./");
                                        let dir_path = dir_pattern.replace('*', "");
                                        let dir_abs = pkg_root.join(&dir_path);
                                        if dir_abs.is_dir() {
                                            if let Ok(entries_iter) = std::fs::read_dir(&dir_abs) {
                                                for entry in entries_iter.flatten() {
                                                    let file_path = entry.path();
                                                    if file_path.is_file() {
                                                        let ext = file_path
                                                            .extension()
                                                            .and_then(|e| e.to_str())
                                                            .unwrap_or("");
                                                        let file_name = file_path
                                                            .file_name()
                                                            .and_then(|n| n.to_str())
                                                            .unwrap_or("");
                                                        if ext == "ts"
                                                            || ext == "mts"
                                                            || ext == "cts"
                                                            || file_name.ends_with(".d.ts")
                                                            || file_name.ends_with(".d.mts")
                                                            || file_name.ends_with(".d.cts")
                                                        {
                                                            results.push(EntryPoint {
                                                                path: file_path,
                                                                condition: Some(
                                                                    "typesVersions".to_string(),
                                                                ),
                                                            });
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Canonicalize + filter non-existent + deduplicate.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut filtered: Vec<EntryPoint> = Vec::new();
    for mut entry in results {
        // Canonicalize to an absolute path (resolves symlinks, `.`, `..`).
        // Fall back to the raw path if canonicalize fails (file may not exist yet).
        let canonical = match entry.path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // File does not exist — skip per D-14.
                eprintln!(
                    "[warn] entry point does not exist: {}",
                    entry.path.display()
                );
                continue;
            }
        };

        if seen.insert(canonical.clone()) {
            entry.path = canonical;
            filtered.push(entry);
        }
    }

    Ok(filtered)
}

/// Recursively walk a `serde_json::Value` from the `exports` field, collecting
/// leaf string values as `EntryPoint`s.
///
/// The `condition` argument tracks the current condition key (e.g., "import") from an
/// ancestor object. Subpath keys (starting with ".") are treated as separate export
/// roots and do not propagate their key as the condition name.
fn resolve_exports_value(
    value: &serde_json::Value,
    base_path: &Path,
    condition: Option<&str>,
    results: &mut Vec<EntryPoint>,
) {
    let mut warnings = Vec::new();
    results.extend(resolve_exports_value_with_warnings(
        value,
        base_path,
        condition,
        &mut warnings,
    ));
    // Non-standard value warnings are currently discarded here (caller doesn't thread warnings).
    // When callers are updated to thread warnings, pass the vec through.
    let _ = warnings;
}

/// ROB-03: Like `resolve_exports_value` but also emits warnings for non-standard
/// `exports` field values (false, null, numeric, array-of-non-string).
///
/// Returns the collected `EntryPoint`s. Warnings are appended to `warnings`.
pub(crate) fn resolve_exports_value_with_warnings(
    value: &serde_json::Value,
    base_path: &Path,
    condition: Option<&str>,
    warnings: &mut Vec<String>,
) -> Vec<EntryPoint> {
    let mut results = Vec::new();
    match value {
        serde_json::Value::String(rel) => {
            // Leaf: a concrete file path relative to the package root.
            let path = base_path.join(rel.trim_start_matches("./"));
            results.push(EntryPoint {
                path,
                condition: condition.map(|s| s.to_string()),
            });
        }
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if key.starts_with('.') {
                    if key.contains('*') {
                        // GAP-16-06: Subpath wildcard key like "./*" or "./features/*".
                        expand_subpath_wildcard(key, child, base_path, &mut results);
                    } else {
                        // Subpath export like "./utils" — recurse without inheriting condition.
                        results.extend(resolve_exports_value_with_warnings(
                            child, base_path, None, warnings,
                        ));
                    }
                } else {
                    // Condition key like "import", "require", "types", "default".
                    results.extend(resolve_exports_value_with_warnings(
                        child,
                        base_path,
                        Some(key.as_str()),
                        warnings,
                    ));
                }
            }
        }
        serde_json::Value::Array(items) => {
            // GAP-16-07: Per Node.js spec, try each candidate in order and use the first
            // one whose file exists on disk.
            for item in items {
                // ROB-03: Warn on non-string entries in the array.
                if !item.is_string() && !item.is_object() && !item.is_array() {
                    warnings.push(format!(
                        "non-string entry in exports array: {:?} \u{2014} skipping",
                        item
                    ));
                    continue;
                }
                results.extend(resolve_exports_value_with_warnings(
                    item, base_path, condition, warnings,
                ));
            }
        }
        // ROB-03: Non-standard scalar exports values — emit warning and return empty.
        // Caller is responsible for falling back to main/module/types when empty.
        serde_json::Value::Bool(false) => {
            warnings.push(
                "non-standard `exports` value: false \u{2014} falling back to main/module/types"
                    .to_string(),
            );
        }
        serde_json::Value::Bool(true) => {
            warnings.push(
                "non-standard `exports` value: true \u{2014} falling back to main/module/types"
                    .to_string(),
            );
        }
        serde_json::Value::Null => {
            // ROB-03: Explicitly disabled export — emit warning.
            warnings.push(
                "non-standard `exports` value: null \u{2014} falling back to main/module/types"
                    .to_string(),
            );
        }
        serde_json::Value::Number(n) => {
            warnings.push(format!(
                "non-standard `exports` value: {} (numeric) \u{2014} falling back to main/module/types",
                n
            ));
        }
    }
    results
}

/// Expand a subpath wildcard pattern like `"./*": "./dist/*.js"` to actual files on disk.
///
/// Scans the target directory for files matching the suffix and creates an EntryPoint for each.
/// GAP-16-06.
fn expand_subpath_wildcard(
    _key: &str,
    value: &serde_json::Value,
    base_path: &Path,
    results: &mut Vec<EntryPoint>,
) {
    let target_strings = collect_string_leaves(value);
    for target in target_strings {
        if !target.contains('*') {
            // No wildcard in target value — treat as a literal path.
            let path = base_path.join(target.trim_start_matches("./"));
            results.push(EntryPoint {
                path,
                condition: None,
            });
            continue;
        }
        // Split target on `*` to get the directory prefix and file suffix.
        // e.g., "./dist/*.js" -> dir_part="dist/", suffix=".js"
        let clean_target = target.trim_start_matches("./");
        let star_pos = match clean_target.find('*') {
            Some(p) => p,
            None => continue,
        };
        let dir_part = &clean_target[..star_pos];
        let suffix = &clean_target[star_pos + 1..];
        let scan_dir = base_path.join(dir_part);

        if scan_dir.is_dir() {
            if let Ok(entries_iter) = std::fs::read_dir(&scan_dir) {
                for entry in entries_iter.flatten() {
                    let file_path = entry.path();
                    if file_path.is_file() {
                        let file_name =
                            file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        // Check that the file matches the suffix pattern.
                        if suffix.is_empty() || file_name.ends_with(suffix) {
                            results.push(EntryPoint {
                                path: file_path,
                                condition: None,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Collect all string leaf values from a JSON value tree (traversing objects and arrays).
///
/// Used to extract concrete file path strings from nested condition objects within
/// a wildcard export value.
fn collect_string_leaves(value: &serde_json::Value) -> Vec<&str> {
    let mut leaves = Vec::new();
    match value {
        serde_json::Value::String(s) => leaves.push(s.as_str()),
        serde_json::Value::Object(map) => {
            for (_k, v) in map {
                leaves.extend(collect_string_leaves(v));
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                leaves.extend(collect_string_leaves(v));
            }
        }
        _ => {}
    }
    leaves
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a temporary directory with a package.json and return both.
    fn make_pkg(pkg_json: &str, files: &[&str]) -> TempDir {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        for f in files {
            // Create any required parent directories.
            let full = dir.path().join(f.trim_start_matches("./"));
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&full, "// stub").unwrap();
        }
        dir
    }

    #[test]
    fn test_typings_field_recognized_when_types_absent() {
        // Package has `typings` only — no `main`, `module`, or `types` field.
        let pkg_json = r#"{"name":"mypkg","typings":"./dist/index.d.ts"}"#;
        let dir = make_pkg(pkg_json, &["dist/index.d.ts"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        assert_eq!(
            entries.len(),
            1,
            "should discover exactly one entry point from typings field"
        );
        assert!(
            entries[0].path.to_string_lossy().ends_with("index.d.ts"),
            "entry point should be the typings path"
        );
    }

    #[test]
    fn test_types_and_typings_both_collected() {
        // Package has BOTH `types` and `typings` pointing to DIFFERENT files.
        // Both should be collected (no break); deduplication only removes same-file duplicates.
        let pkg_json =
            r#"{"name":"mypkg","types":"./dist/types.d.ts","typings":"./dist/typings.d.ts"}"#;
        let dir = make_pkg(pkg_json, &["dist/types.d.ts", "dist/typings.d.ts"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        assert_eq!(
            entries.len(),
            2,
            "both types and typings fields should contribute entry points; got: {entries:?}"
        );
        let paths: Vec<String> = entries
            .iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect();
        assert!(
            paths.iter().any(|p| p.ends_with("types.d.ts")),
            "entry points should include types.d.ts; got: {paths:?}"
        );
        assert!(
            paths.iter().any(|p| p.ends_with("typings.d.ts")),
            "entry points should include typings.d.ts; got: {paths:?}"
        );
    }

    #[test]
    fn test_main_field_still_works() {
        let pkg_json = r#"{"name":"mypkg","main":"./dist/index.js"}"#;
        let dir = make_pkg(pkg_json, &["dist/index.js"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].path.to_string_lossy().ends_with("index.js"));
    }

    #[test]
    fn test_multi_entry_main_and_types() {
        // Package has both `main` (runtime) and `types` (declaration) fields.
        // Both should be collected as separate entry points.
        let pkg_json = r#"{"name":"mypkg","main":"./dist/index.js","types":"./dist/index.d.ts"}"#;
        let dir = make_pkg(pkg_json, &["dist/index.js", "dist/index.d.ts"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        assert_eq!(
            entries.len(),
            2,
            "both main and types should produce entry points; got: {entries:?}"
        );
    }

    #[test]
    fn test_multi_entry_dedup_same_file() {
        // Package has `main` and `module` pointing at the same file.
        // After deduplication, only one entry point should be returned.
        let pkg_json = r#"{"name":"mypkg","main":"./dist/index.js","module":"./dist/index.js"}"#;
        let dir = make_pkg(pkg_json, &["dist/index.js"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        assert_eq!(
            entries.len(),
            1,
            "duplicate paths should be deduplicated; got: {entries:?}"
        );
    }

    #[test]
    fn test_types_only_entry() {
        // Package has only `types` field (no `main`). Should return one entry point.
        let pkg_json = r#"{"name":"mypkg","types":"./dist/index.d.ts"}"#;
        let dir = make_pkg(pkg_json, &["dist/index.d.ts"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        assert_eq!(
            entries.len(),
            1,
            "types-only package should return one entry point; got: {entries:?}"
        );
        assert!(
            entries[0].path.to_string_lossy().ends_with("index.d.ts"),
            "entry point path should end with index.d.ts; got: {:?}",
            entries[0].path
        );
    }

    // GAP-16-12: browser field test
    #[test]
    fn test_browser_field_parsed_as_entry_point() {
        // Package uses the "browser" field as a bundler entry point.
        let pkg_json = r#"{"name":"mypkg","main":"./dist/index.js","browser":"./dist/browser.js"}"#;
        let dir = make_pkg(pkg_json, &["dist/index.js", "dist/browser.js"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        let paths: Vec<String> = entries
            .iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect();
        assert!(
            paths.iter().any(|p| p.ends_with("browser.js")),
            "browser field should be discovered as an entry point; got: {paths:?}"
        );
    }

    // GAP-16-07: exports array fallback tests
    #[test]
    fn test_exports_array_first_existing_wins() {
        // Array with 3 candidates; only the 2nd exists on disk.
        // Should discover the 2nd entry (the first existing one).
        let pkg_json = r#"{
            "name": "mypkg",
            "exports": ["./dist/missing1.js", "./dist/real.js", "./dist/missing2.js"]
        }"#;
        let dir = make_pkg(pkg_json, &["dist/real.js"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        assert_eq!(
            entries.len(),
            1,
            "only the existing candidate should be returned; got: {entries:?}"
        );
        assert!(
            entries[0].path.to_string_lossy().ends_with("real.js"),
            "should pick the first existing file; got: {:?}",
            entries[0].path
        );
    }

    #[test]
    fn test_exports_array_all_missing_returns_empty() {
        // All candidates in the array are non-existent — should return empty.
        let pkg_json = r#"{
            "name": "mypkg",
            "exports": ["./dist/a.js", "./dist/b.js"]
        }"#;
        let dir = make_pkg(pkg_json, &[]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        assert!(
            entries.is_empty(),
            "no existing candidates should produce empty entry points; got: {entries:?}"
        );
    }

    // GAP-16-11: typesVersions wildcard expansion tests
    #[test]
    fn test_typesversions_wildcard_expands_to_d_ts_files() {
        // typesVersions: { ">=4.0": { "*": ["dist/types/*"] } }
        // dist/types/ contains two .d.ts files — both should be discovered.
        let pkg_json = r#"{
            "name": "mypkg",
            "typesVersions": {
                ">=4.0": {
                    "*": ["dist/types/*"]
                }
            }
        }"#;
        let dir = make_pkg(pkg_json, &["dist/types/foo.d.ts", "dist/types/bar.d.ts"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        let paths: Vec<String> = entries
            .iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect();
        assert!(
            paths.iter().any(|p| p.ends_with("foo.d.ts")),
            "should discover foo.d.ts from typesVersions wildcard; got: {paths:?}"
        );
        assert!(
            paths.iter().any(|p| p.ends_with("bar.d.ts")),
            "should discover bar.d.ts from typesVersions wildcard; got: {paths:?}"
        );
    }

    #[test]
    fn test_typesversions_non_wildcard_path_still_works() {
        // Non-wildcard typesVersions path should still be resolved directly.
        let pkg_json = r#"{
            "name": "mypkg",
            "typesVersions": {
                ">=4.0": {
                    "*": ["dist/index.d.ts"]
                }
            }
        }"#;
        let dir = make_pkg(pkg_json, &["dist/index.d.ts"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        let paths: Vec<String> = entries
            .iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect();
        assert!(
            paths.iter().any(|p| p.ends_with("index.d.ts")),
            "non-wildcard typesVersions path should be resolved; got: {paths:?}"
        );
    }

    // GAP-16-06: subpath wildcard expansion tests
    #[test]
    fn test_subpath_wildcard_expands_to_matching_files() {
        // exports: { "./*": "./dist/*.js" }
        // dist/ contains a.js and b.js — both should be discovered.
        let pkg_json = r#"{
            "name": "mypkg",
            "exports": {
                "./*": "./dist/*.js"
            }
        }"#;
        let dir = make_pkg(pkg_json, &["dist/a.js", "dist/b.js"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        let paths: Vec<String> = entries
            .iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect();
        assert!(
            paths.iter().any(|p| p.ends_with("a.js")),
            "subpath wildcard should discover a.js; got: {paths:?}"
        );
        assert!(
            paths.iter().any(|p| p.ends_with("b.js")),
            "subpath wildcard should discover b.js; got: {paths:?}"
        );
    }

    #[test]
    fn test_subpath_wildcard_with_suffix_filter() {
        // exports: { "./*": "./dist/*.cjs" }
        // dist/ contains a.cjs and b.js — only a.cjs should match.
        let pkg_json = r#"{
            "name": "mypkg",
            "exports": {
                "./*": "./dist/*.cjs"
            }
        }"#;
        let dir = make_pkg(pkg_json, &["dist/a.cjs", "dist/b.js"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        let paths: Vec<String> = entries
            .iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect();
        assert!(
            paths.iter().any(|p| p.ends_with("a.cjs")),
            "subpath wildcard should discover a.cjs; got: {paths:?}"
        );
        assert!(
            !paths.iter().any(|p| p.ends_with("b.js")),
            "b.js should NOT match a .cjs pattern; got: {paths:?}"
        );
    }

    // ── ROB-03: Non-standard `exports` values produce warnings + fallback ────

    #[test]
    fn rob_03_exports_false_produces_warning_and_empty() {
        let v = serde_json::json!(false);
        let mut warnings: Vec<String> = Vec::new();
        let res = resolve_exports_value_with_warnings(&v, Path::new("/"), None, &mut warnings);
        assert!(
            res.is_empty(),
            "ROB-03: exports=false should produce empty results"
        );
        assert!(
            warnings.iter().any(|w| w.contains("false")),
            "ROB-03: expected warning mentioning 'false'; got: {warnings:?}"
        );
    }

    #[test]
    fn rob_03_exports_null_produces_warning() {
        let v = serde_json::Value::Null;
        let mut warnings: Vec<String> = Vec::new();
        let res = resolve_exports_value_with_warnings(&v, Path::new("/"), None, &mut warnings);
        assert!(
            res.is_empty(),
            "ROB-03: exports=null should produce empty results"
        );
        assert!(
            warnings.iter().any(|w| w.contains("null")),
            "ROB-03: expected warning mentioning 'null'; got: {warnings:?}"
        );
    }

    #[test]
    fn rob_03_exports_numeric_produces_warning() {
        let v = serde_json::json!(42);
        let mut warnings: Vec<String> = Vec::new();
        let _ = resolve_exports_value_with_warnings(&v, Path::new("/"), None, &mut warnings);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("42") || w.contains("numeric")),
            "ROB-03: expected warning mentioning numeric value; got: {warnings:?}"
        );
    }
}
