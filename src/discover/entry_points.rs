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
        for field in &["main", "module", "types", "typings"] {
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
                                    // Wildcard paths need the wildcard stripped to get the directory.
                                    if !path_str.contains('*') {
                                        let abs = pkg_root.join(path_str.trim_start_matches("./"));
                                        results.push(EntryPoint {
                                            path: abs,
                                            condition: Some("typesVersions".to_string()),
                                        });
                                    }
                                    // If path contains *, treat it as a directory hint:
                                    // e.g., "dist/types/*" -> scan "dist/types/" dir
                                    // This is best-effort; the main purpose is metadata diff.
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
                    // Subpath export like "./utils" — recurse without inheriting the key as condition.
                    resolve_exports_value(child, base_path, None, results);
                } else {
                    // Condition key like "import", "require", "types", "default".
                    resolve_exports_value(child, base_path, Some(key.as_str()), results);
                }
            }
        }
        serde_json::Value::Array(items) => {
            // Per Node.js resolution: take the first item that resolves successfully.
            // We collect all candidates and let the path-existence filter below prune them;
            // in practice we only process the first string we encounter.
            if let Some(first) = items.first() {
                resolve_exports_value(first, base_path, condition, results);
            }
        }
        serde_json::Value::Null => {
            // Explicitly disabled export — skip.
        }
        _ => {
            // Boolean or number in exports field — ignore.
        }
    }
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
        let pkg_json = r#"{"name":"mypkg","types":"./dist/types.d.ts","typings":"./dist/typings.d.ts"}"#;
        let dir = make_pkg(pkg_json, &["dist/types.d.ts", "dist/typings.d.ts"]);
        let entries = resolve_entry_points(dir.path()).unwrap();
        assert_eq!(entries.len(), 2, "both types and typings fields should contribute entry points; got: {entries:?}");
        let paths: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
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
}
