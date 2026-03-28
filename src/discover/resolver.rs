use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::types::{ExportKind, ExportMap, ExportSymbol};
use super::tree_query::{language_for_file, parse_exports, RawExportKind};

/// Maximum number of re-export hops before aborting a resolution chain (D-12).
#[allow(dead_code)]
const MAX_RESOLVE_DEPTH: usize = 20;

/// A non-fatal warning emitted during export resolution.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ResolveWarning {
    pub message: String,
}

impl ResolveWarning {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

/// Recursively collect all exports reachable from `file` into `map`.
///
/// - `pkg_root` — absolute path to the package root; used to compute relative
///   paths stored in `ExportSymbol.file`.
/// - `visited` — set of canonicalized absolute paths already processed; used
///   for cycle detection (D-12).
/// - `warnings` — non-fatal issues encountered during resolution.
/// - `depth` — current recursion depth; aborted at `MAX_RESOLVE_DEPTH` (D-12).
#[allow(dead_code)]
pub fn collect_exports(
    file: &Path,
    pkg_root: &Path,
    map: &mut ExportMap,
    visited: &mut HashSet<PathBuf>,
    warnings: &mut Vec<ResolveWarning>,
    depth: usize,
) -> anyhow::Result<()> {
    if depth > MAX_RESOLVE_DEPTH {
        warnings.push(ResolveWarning::new(format!(
            "maximum resolution depth ({MAX_RESOLVE_DEPTH}) reached at {}",
            file.display()
        )));
        return Ok(());
    }

    if !file.exists() {
        warnings.push(ResolveWarning::new(format!(
            "file not found: {}, skipping",
            file.display()
        )));
        return Ok(());
    }

    let canonical = file
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", file.display()))?;

    if !visited.insert(canonical.clone()) {
        warnings.push(ResolveWarning::new(format!(
            "circular re-export detected at {}, skipping",
            file.display()
        )));
        return Ok(());
    }

    let lang = match language_for_file(file) {
        Some(l) => l,
        None => {
            warnings.push(ResolveWarning::new(format!(
                "unsupported file extension: {}",
                file.display()
            )));
            return Ok(());
        }
    };

    let source = std::fs::read(&canonical)
        .with_context(|| format!("failed to read {}", canonical.display()))?;

    let raw_exports = parse_exports(&source, lang)?;

    // Compute the file path relative to pkg_root for storage in ExportSymbol.
    let rel_path = canonical
        .strip_prefix(pkg_root)
        .unwrap_or(&canonical)
        .to_path_buf();

    for export in raw_exports {
        match export.kind {
            RawExportKind::Named => {
                map.entry(export.name.clone()).or_insert(ExportSymbol {
                    name: export.name,
                    kind: ExportKind::Named,
                    file: rel_path.clone(),
                    line: export.line,
                    col: export.col,
                    type_signature: None,
                    type_shape: None,
                });
            }
            RawExportKind::Default => {
                map.entry("default".to_string()).or_insert(ExportSymbol {
                    name: "default".to_string(),
                    kind: ExportKind::Default,
                    file: rel_path.clone(),
                    line: export.line,
                    col: export.col,
                    type_signature: None,
                    type_shape: None,
                });
            }
            RawExportKind::StarReexport => {
                if let Some(specifier) = &export.source {
                    let candidates = resolve_module_path(&canonical, specifier);
                    let resolved = candidates.into_iter().find(|p| p.exists());
                    match resolved {
                        Some(target) => {
                            collect_exports(
                                &target,
                                pkg_root,
                                map,
                                visited,
                                warnings,
                                depth + 1,
                            )?;
                        }
                        None => {
                            warnings.push(ResolveWarning::new(format!(
                                "could not resolve star re-export '{}' from {}",
                                specifier,
                                file.display()
                            )));
                        }
                    }
                }
            }
            RawExportKind::NamedReexport => {
                if let Some(specifier) = &export.source {
                    let candidates = resolve_module_path(&canonical, specifier);
                    let resolved = candidates.into_iter().find(|p| p.exists());
                    match resolved {
                        Some(target) => {
                            // For named re-exports we only want the specific
                            // symbol, not everything the target file exports.
                            // We insert a placeholder pointing at the target
                            // file's location rather than recursing fully.
                            // The ExportSymbol references the re-export site
                            // (current file) for accurate position reporting.
                            map.entry(export.name.clone()).or_insert(ExportSymbol {
                                name: export.name.clone(),
                                kind: ExportKind::Named,
                                file: target
                                    .strip_prefix(pkg_root)
                                    .unwrap_or(&target)
                                    .to_path_buf(),
                                line: export.line,
                                col: export.col,
                                type_signature: None,
                                type_shape: None,
                            });
                        }
                        None => {
                            warnings.push(ResolveWarning::new(format!(
                                "could not resolve named re-export '{}' from {} (specifier '{}')",
                                export.name,
                                file.display(),
                                specifier,
                            )));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Return an ordered list of candidate absolute paths for a module specifier.
///
/// The caller should pick the first candidate that exists on disk.
/// Extension probing order matches Node.js/TypeScript resolution heuristics.
#[allow(dead_code)]
pub fn resolve_module_path(from_file: &Path, specifier: &str) -> Vec<PathBuf> {
    let base_dir = match from_file.parent() {
        Some(d) => d.to_path_buf(),
        None => PathBuf::from("."),
    };

    // Strip leading "./" from the specifier before joining.
    let clean = specifier.trim_start_matches("./");
    let joined = base_dir.join(clean);

    let mut candidates = Vec::with_capacity(10);

    // 1. Exact path as-is (handles already-extensioned imports).
    candidates.push(joined.clone());
    // 2. TypeScript source extensions (higher priority than .js for TS projects).
    candidates.push(joined.with_extension("ts"));
    candidates.push(joined.with_extension("tsx"));
    // 3. Declaration files.
    let d_ts = joined.with_extension("ts");
    // Build "foo.d.ts" by appending ".d.ts" to the stem.
    let mut d_ts_name = joined
        .file_stem()
        .unwrap_or_default()
        .to_os_string();
    d_ts_name.push(".d.ts");
    candidates.push(joined.with_file_name(d_ts_name));
    let _ = d_ts; // suppress unused warning
    // 4. JavaScript extensions.
    candidates.push(joined.with_extension("js"));
    candidates.push(joined.with_extension("mjs"));
    candidates.push(joined.with_extension("cjs"));
    // 5. Directory index files.
    candidates.push(joined.join("index.ts"));
    candidates.push(joined.join("index.js"));
    candidates.push(joined.join("index.d.ts"));

    candidates
}
