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
        // Fall back to main / module / types in priority order.
        for field in &["main", "module", "types"] {
            if let Some(serde_json::Value::String(rel)) = pkg.get(*field) {
                let abs = pkg_root.join(rel.trim_start_matches("./"));
                results.push(EntryPoint {
                    path: abs,
                    condition: None,
                });
                // Use first hit only — they share the same role.
                break;
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
