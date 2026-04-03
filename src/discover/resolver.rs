use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Context;

use super::tree_query::{language_for_file, parse_exports, RawExportKind};
use crate::types::{ExportKind, ExportMap, ExportSymbol};

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
                    overloads: Vec::new(),
                    decl_kind: export.decl_kind,
                    type_guard_type: None,
                    is_async: false,
                    is_generator: false,
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
                    overloads: Vec::new(),
                    decl_kind: export.decl_kind,
                    type_guard_type: None,
                    is_async: false,
                    is_generator: false,
                });
            }
            RawExportKind::StarReexport => {
                if let Some(specifier) = &export.source {
                    let candidates = resolve_module_path(&canonical, specifier);
                    let resolved = candidates.into_iter().find(|p| p.exists());
                    match resolved {
                        Some(target) => {
                            collect_exports(&target, pkg_root, map, visited, warnings, depth + 1)?;
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
                                overloads: Vec::new(),
                                decl_kind: export.decl_kind,
                                type_guard_type: None,
                                is_async: false,
                                is_generator: false,
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

/// Append an extension to a path without replacing existing extensions.
///
/// Unlike `PathBuf::with_extension()`, this preserves compound file stems.
/// Example: `append_extension(Path::new("utils.config"), "ts")` -> "utils.config.ts"
/// (whereas `with_extension` would produce "utils.ts", dropping ".config")
fn append_extension(base: &Path, ext: &str) -> PathBuf {
    let name = base.file_name().unwrap_or_default().to_string_lossy();
    base.with_file_name(format!("{}.{}", name, ext))
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

    let mut candidates = Vec::with_capacity(20);

    // 1. Exact path as-is (handles already-extensioned imports).
    candidates.push(joined.clone());

    // GAP-13-06: TypeScript moduleResolution node16/bundler — .js → .ts probing.
    // When specifier ends with .js/.mjs/.cjs, probe TypeScript equivalents.
    // Must come BEFORE generic extension probing to get correct priority:
    // foo.js (exact) → foo.ts → foo.tsx → foo.d.ts → (then generic extensions)
    if let Some(ext) = joined.extension() {
        let ext_str = ext.to_string_lossy();
        match ext_str.as_ref() {
            "js" => {
                let stem = joined.with_extension("");
                candidates.push(append_extension(&stem, "ts"));
                candidates.push(append_extension(&stem, "tsx"));
                candidates.push(stem.with_file_name(format!(
                    "{}.d.ts",
                    stem.file_name().unwrap_or_default().to_string_lossy()
                )));
            }
            "mjs" => {
                let stem = joined.with_extension("");
                candidates.push(append_extension(&stem, "mts"));
                candidates.push(stem.with_file_name(format!(
                    "{}.d.mts",
                    stem.file_name().unwrap_or_default().to_string_lossy()
                )));
            }
            "cjs" => {
                let stem = joined.with_extension("");
                candidates.push(append_extension(&stem, "cts"));
                candidates.push(stem.with_file_name(format!(
                    "{}.d.cts",
                    stem.file_name().unwrap_or_default().to_string_lossy()
                )));
            }
            _ => {}
        }
    }

    // 2. TypeScript source extensions (higher priority for TS projects).
    candidates.push(append_extension(&joined, "ts"));
    candidates.push(append_extension(&joined, "tsx"));
    candidates.push(append_extension(&joined, "mts"));
    candidates.push(append_extension(&joined, "cts"));

    // 3. Declaration files (.d.ts, .d.mts, .d.cts).
    let stem = joined.file_name().unwrap_or_default().to_string_lossy();
    candidates.push(joined.with_file_name(format!("{}.d.ts", stem)));
    candidates.push(joined.with_file_name(format!("{}.d.mts", stem)));
    candidates.push(joined.with_file_name(format!("{}.d.cts", stem)));

    // 4. JavaScript extensions.
    candidates.push(append_extension(&joined, "js"));
    candidates.push(append_extension(&joined, "jsx"));
    candidates.push(append_extension(&joined, "mjs"));
    candidates.push(append_extension(&joined, "cjs"));

    // 5. Directory index files.
    candidates.push(joined.join("index.ts"));
    candidates.push(joined.join("index.tsx"));
    candidates.push(joined.join("index.mts"));
    candidates.push(joined.join("index.js"));
    candidates.push(joined.join("index.mjs"));
    candidates.push(joined.join("index.d.ts"));

    candidates
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compound_extension_preserved() {
        let result = append_extension(Path::new("/a/utils.config"), "ts");
        let name = result.file_name().unwrap().to_string_lossy();
        assert_eq!(name, "utils.config.ts", "append_extension must preserve compound stem");
    }

    #[test]
    fn test_resolve_includes_mts() {
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./foo");
        let has_mts = candidates
            .iter()
            .any(|p| p.to_string_lossy().contains("foo.mts"));
        assert!(has_mts, "candidates must include foo.mts; got: {candidates:?}");
    }

    #[test]
    fn test_resolve_includes_cts() {
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./foo");
        let has_cts = candidates
            .iter()
            .any(|p| p.to_string_lossy().contains("foo.cts"));
        assert!(has_cts, "candidates must include foo.cts; got: {candidates:?}");
    }

    #[test]
    fn test_resolve_includes_d_mts() {
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./foo");
        let has = candidates
            .iter()
            .any(|p| p.to_string_lossy().contains("foo.d.mts"));
        assert!(has, "candidates must include foo.d.mts; got: {candidates:?}");
    }

    #[test]
    fn test_resolve_includes_d_cts() {
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./foo");
        let has = candidates
            .iter()
            .any(|p| p.to_string_lossy().contains("foo.d.cts"));
        assert!(has, "candidates must include foo.d.cts; got: {candidates:?}");
    }

    #[test]
    fn test_resolve_compound_stem() {
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./utils.config");
        let has_compound = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("utils.config.ts"));
        let has_wrong = candidates
            .iter()
            .any(|p| {
                let s = p.to_string_lossy();
                // "utils.ts" would mean compound stem was dropped
                s.ends_with("/utils.ts") || s.ends_with("\\utils.ts")
            });
        assert!(has_compound, "must include utils.config.ts; got: {candidates:?}");
        assert!(!has_wrong, "must NOT include utils.ts (dropped compound stem); got: {candidates:?}");
    }

    #[test]
    fn test_resolve_includes_index_mts() {
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./dir");
        let has = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("dir/index.mts"));
        assert!(has, "candidates must include dir/index.mts; got: {candidates:?}");
    }
}
