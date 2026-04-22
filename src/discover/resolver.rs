use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Context;

use super::tree_query::{language_for_file, parse_exports, RawExportKind};
use crate::types::{ExportKind, ExportMap, ExportSymbol};

// ---------------------------------------------------------------------------
// Tsconfig path alias resolution (GAP-15-04)
// ---------------------------------------------------------------------------

/// Parsed tsconfig.json path aliases.
#[derive(Debug, Clone, Default)]
pub struct TsconfigPaths {
    /// Base URL for non-relative module resolution.
    pub base_url: Option<String>,
    /// Path alias mappings: key is the alias pattern (e.g., "@/*"),
    /// value is the list of replacement paths (e.g., ["./src/*"]).
    pub paths: std::collections::HashMap<String, Vec<String>>,
}

/// Parse `tsconfig.json` from `pkg_root` and extract `compilerOptions.paths` and `compilerOptions.baseUrl`.
///
/// Returns `None` if tsconfig.json doesn't exist or is unparseable.
/// This is best-effort — missing fields are silently ignored.
pub fn parse_tsconfig_paths(pkg_root: &Path) -> Option<TsconfigPaths> {
    let tsconfig_path = pkg_root.join("tsconfig.json");
    let content = std::fs::read_to_string(&tsconfig_path).ok()?;
    // tsconfig.json may have comments — strip // line comments before parsing
    let stripped: String = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let json: serde_json::Value = serde_json::from_str(&stripped).ok()?;

    let compiler_opts = json.get("compilerOptions")?;

    let base_url = compiler_opts
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut paths_map = std::collections::HashMap::new();
    if let Some(paths_obj) = compiler_opts.get("paths").and_then(|v| v.as_object()) {
        for (key, value) in paths_obj {
            if let Some(arr) = value.as_array() {
                let replacements: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if !replacements.is_empty() {
                    paths_map.insert(key.clone(), replacements);
                }
            }
        }
    }

    Some(TsconfigPaths {
        base_url,
        paths: paths_map,
    })
}

/// Attempt to resolve a module specifier using tsconfig path aliases.
///
/// Given a specifier like `@/utils` and paths like `{"@/*": ["./src/*"]}`,
/// returns a list of candidate absolute paths to try.
///
/// Returns empty Vec if no alias matches.
pub fn resolve_tsconfig_alias(
    specifier: &str,
    tsconfig: &TsconfigPaths,
    pkg_root: &Path,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // Determine the base directory for resolution
    let base_dir = if let Some(ref base_url) = tsconfig.base_url {
        pkg_root.join(base_url)
    } else {
        pkg_root.to_path_buf()
    };

    for (pattern, replacements) in &tsconfig.paths {
        // Pattern can be exact match or wildcard match (e.g., "@/*")
        if pattern.contains('*') {
            // Wildcard pattern: "@/*" matches "@/utils", "@/components/Button", etc.
            let prefix = pattern.strip_suffix('*').unwrap_or(pattern);
            if let Some(remainder) = specifier.strip_prefix(prefix) {
                for replacement in replacements {
                    if replacement.contains('*') {
                        let replaced = replacement.replace('*', remainder);
                        let resolved = base_dir.join(replaced.trim_start_matches("./"));
                        // Add with standard extension probing
                        candidates.push(resolved.clone());
                        candidates.push(resolved.with_extension("ts"));
                        candidates.push(resolved.with_extension("tsx"));
                        candidates.push(resolved.with_extension("js"));
                        candidates.push(resolved.with_extension("jsx"));
                        // Directory index fallback
                        candidates.push(resolved.join("index.ts"));
                        candidates.push(resolved.join("index.tsx"));
                        candidates.push(resolved.join("index.js"));
                    } else {
                        let resolved = base_dir.join(replacement.trim_start_matches("./"));
                        candidates.push(resolved);
                    }
                }
            }
        } else {
            // Exact pattern match
            if specifier == pattern {
                for replacement in replacements {
                    let resolved = base_dir.join(replacement.trim_start_matches("./"));
                    candidates.push(resolved);
                }
            }
        }
    }

    candidates
}

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
    tsconfig: Option<&TsconfigPaths>,
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
                    is_reexport: false,
                    is_type_only: false,
                    release_tag: None,
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
                    is_reexport: false,
                    is_type_only: false,
                    release_tag: None,
                });
            }
            RawExportKind::StarReexport => {
                if let Some(specifier) = &export.source {
                    let mut candidates = resolve_module_path(&canonical, specifier);
                    // GAP-15-04: Try tsconfig alias resolution if standard resolution fails
                    if let Some(tc) = tsconfig {
                        if !candidates.iter().any(|p| p.exists()) {
                            candidates.extend(resolve_tsconfig_alias(specifier, tc, pkg_root));
                        }
                    }
                    let resolved = candidates.into_iter().find(|p| p.exists());
                    match resolved {
                        Some(target) => {
                            collect_exports(&target, pkg_root, map, visited, warnings, depth + 1, tsconfig)?;
                        }
                        None => {
                            // GAP-15-05: If specifier looks like an external package (not relative),
                            // track it as an external re-export entry rather than just warning.
                            if !specifier.starts_with('.') && !specifier.starts_with('/') {
                                let reexport_key = format!("*<{}>", specifier);
                                map.entry(reexport_key.clone()).or_insert(ExportSymbol {
                                    name: reexport_key,
                                    kind: ExportKind::Named,
                                    file: rel_path.clone(),
                                    line: export.line,
                                    col: export.col,
                                    type_signature: None,
                                    overloads: Vec::new(),
                                    decl_kind: None,
                                    type_guard_type: None,
                                    is_async: false,
                                    is_generator: false,
                                    is_reexport: true,
                                    is_type_only: false,
                                    release_tag: None,
                                });
                            } else {
                                warnings.push(ResolveWarning::new(format!(
                                    "could not resolve star re-export '{}' from {}",
                                    specifier,
                                    file.display()
                                )));
                            }
                        }
                    }
                }
            }
            RawExportKind::NamedReexport => {
                if let Some(specifier) = &export.source {
                    let mut candidates = resolve_module_path(&canonical, specifier);
                    // GAP-15-04: Try tsconfig alias resolution if standard resolution fails
                    if let Some(tc) = tsconfig {
                        if !candidates.iter().any(|p| p.exists()) {
                            candidates.extend(resolve_tsconfig_alias(specifier, tc, pkg_root));
                        }
                    }
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
                                is_reexport: false,
                                is_type_only: false,
                                release_tag: None,
                            });
                        }
                        None => {
                            // GAP-15-05: If specifier looks like an external package (not relative),
                            // track it as an external named re-export entry.
                            if !specifier.starts_with('.') && !specifier.starts_with('/') {
                                map.entry(export.name.clone()).or_insert(ExportSymbol {
                                    name: export.name.clone(),
                                    kind: ExportKind::Named,
                                    file: rel_path.clone(),
                                    line: export.line,
                                    col: export.col,
                                    type_signature: None,
                                    overloads: Vec::new(),
                                    decl_kind: None,
                                    type_guard_type: None,
                                    is_async: false,
                                    is_generator: false,
                                    is_reexport: true,
                                    is_type_only: false,
                                    release_tag: None,
                                });
                            } else {
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

    // 5. Directory index files (GAP-13-07: complete candidate list).
    // Order: TypeScript source → JavaScript source → Declaration files.
    candidates.push(joined.join("index.ts"));
    candidates.push(joined.join("index.tsx"));
    candidates.push(joined.join("index.mts"));
    candidates.push(joined.join("index.cts"));
    candidates.push(joined.join("index.js"));
    candidates.push(joined.join("index.jsx"));
    candidates.push(joined.join("index.mjs"));
    candidates.push(joined.join("index.cjs"));
    candidates.push(joined.join("index.d.ts"));
    candidates.push(joined.join("index.d.mts"));
    candidates.push(joined.join("index.d.cts"));

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

    #[test]
    fn test_js_to_ts_probing() {
        // GAP-13-06: ./foo.js should probe foo.ts before foo.js.ts
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./foo.js");
        let ts_idx = candidates.iter().position(|p| p.to_string_lossy().ends_with("/foo.ts"));
        let js_ts_idx = candidates.iter().position(|p| p.to_string_lossy().ends_with("/foo.js.ts"));
        assert!(ts_idx.is_some(), "candidates must include foo.ts; got: {candidates:?}");
        // foo.ts should appear before foo.js.ts in the list
        if let (Some(ts), Some(js_ts)) = (ts_idx, js_ts_idx) {
            assert!(ts < js_ts, "foo.ts (idx {ts}) should appear before foo.js.ts (idx {js_ts})");
        }
    }

    #[test]
    fn test_mjs_to_mts_probing() {
        // GAP-13-06: ./bar.mjs should probe bar.mts
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./bar.mjs");
        let has_mts = candidates.iter().any(|p| p.to_string_lossy().ends_with("/bar.mts"));
        assert!(has_mts, "candidates must include bar.mts; got: {candidates:?}");
    }

    #[test]
    fn test_cjs_to_cts_probing() {
        // GAP-13-06: ./baz.cjs should probe baz.cts
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./baz.cjs");
        let has_cts = candidates.iter().any(|p| p.to_string_lossy().ends_with("/baz.cts"));
        assert!(has_cts, "candidates must include baz.cts; got: {candidates:?}");
    }

    #[test]
    fn test_js_to_ts_compound_stem() {
        // GAP-13-06 + ECF-06: ./utils.config.js should probe utils.config.ts (not utils.ts)
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./utils.config.js");
        let has_config_ts = candidates.iter().any(|p| p.to_string_lossy().ends_with("/utils.config.ts"));
        assert!(has_config_ts, "candidates must include utils.config.ts; got: {candidates:?}");
        // Should NOT produce just utils.ts (dropping .config)
        let has_bare_utils = candidates.iter().any(|p| {
            let s = p.to_string_lossy();
            s.ends_with("/utils.ts") && !s.contains("config")
        });
        assert!(!has_bare_utils, "must NOT drop compound stem; got: {candidates:?}");
    }

    #[test]
    fn test_resolve_includes_index_cts() {
        // GAP-13-07: directory index should include index.cts
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./dir");
        let has = candidates.iter().any(|p| p.to_string_lossy().ends_with("dir/index.cts"));
        assert!(has, "candidates must include dir/index.cts; got: {candidates:?}");
    }

    #[test]
    fn test_resolve_includes_index_jsx() {
        // GAP-13-07: directory index should include index.jsx
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./dir");
        let has = candidates.iter().any(|p| p.to_string_lossy().ends_with("dir/index.jsx"));
        assert!(has, "candidates must include dir/index.jsx; got: {candidates:?}");
    }

    #[test]
    fn test_resolve_includes_index_cjs() {
        // GAP-13-07: directory index should include index.cjs
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./dir");
        let has = candidates.iter().any(|p| p.to_string_lossy().ends_with("dir/index.cjs"));
        assert!(has, "candidates must include dir/index.cjs; got: {candidates:?}");
    }

    #[test]
    fn test_resolve_includes_index_d_mts() {
        // GAP-13-07: directory index should include index.d.mts
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./dir");
        let has = candidates.iter().any(|p| p.to_string_lossy().ends_with("dir/index.d.mts"));
        assert!(has, "candidates must include dir/index.d.mts; got: {candidates:?}");
    }

    #[test]
    fn test_resolve_includes_index_d_cts() {
        // GAP-13-07: directory index should include index.d.cts
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./dir");
        let has = candidates.iter().any(|p| p.to_string_lossy().ends_with("dir/index.d.cts"));
        assert!(has, "candidates must include dir/index.d.cts; got: {candidates:?}");
    }

    #[test]
    fn test_js_to_dts_probing() {
        // GAP-13-06: ./foo.js should also probe foo.d.ts for pre-built packages
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./foo.js");
        let has_dts = candidates.iter().any(|p| {
            let s = p.to_string_lossy();
            s.ends_with("/foo.d.ts")
        });
        assert!(has_dts, "candidates must include foo.d.ts; got: {candidates:?}");
    }

    // -------------------------------------------------------------------------
    // GAP-15-04: tsconfig path alias resolution tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_resolve_tsconfig_alias_wildcard() {
        // Pattern "@/*" with replacement "./src/*", specifier "@/utils" → {root}/src/utils.ts
        let mut paths = std::collections::HashMap::new();
        paths.insert("@/*".to_string(), vec!["./src/*".to_string()]);
        let tsconfig = TsconfigPaths { base_url: None, paths };
        let pkg_root = Path::new("/project");
        let candidates = resolve_tsconfig_alias("@/utils", &tsconfig, pkg_root);
        let has_utils_ts = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("src/utils.ts"));
        assert!(has_utils_ts, "wildcard alias must produce src/utils.ts; got: {candidates:?}");
    }

    #[test]
    fn test_resolve_tsconfig_alias_exact() {
        // Exact pattern "@app" with replacement "./src/app.ts", specifier "@app" resolves
        let mut paths = std::collections::HashMap::new();
        paths.insert("@app".to_string(), vec!["./src/app.ts".to_string()]);
        let tsconfig = TsconfigPaths { base_url: None, paths };
        let pkg_root = Path::new("/project");
        let candidates = resolve_tsconfig_alias("@app", &tsconfig, pkg_root);
        assert!(!candidates.is_empty(), "exact alias must produce candidates; got: {candidates:?}");
        let has_app_ts = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("src/app.ts"));
        assert!(has_app_ts, "exact alias must produce src/app.ts; got: {candidates:?}");
    }

    #[test]
    fn test_resolve_tsconfig_alias_no_match() {
        // Specifier "lodash" with no matching alias returns empty candidates
        let mut paths = std::collections::HashMap::new();
        paths.insert("@/*".to_string(), vec!["./src/*".to_string()]);
        let tsconfig = TsconfigPaths { base_url: None, paths };
        let pkg_root = Path::new("/project");
        let candidates = resolve_tsconfig_alias("lodash", &tsconfig, pkg_root);
        assert!(candidates.is_empty(), "non-aliased specifier must return empty; got: {candidates:?}");
    }

    #[test]
    fn test_parse_tsconfig_paths_basic() {
        // Create a temp dir with a tsconfig.json and verify parsing
        let dir = tempfile::tempdir().expect("tempdir");
        let tsconfig_content = r#"{
            "compilerOptions": {
                "baseUrl": ".",
                "paths": {
                    "@/*": ["./src/*"],
                    "@app": ["./src/app.ts"]
                }
            }
        }"#;
        std::fs::write(dir.path().join("tsconfig.json"), tsconfig_content).unwrap();

        let result = parse_tsconfig_paths(dir.path());
        assert!(result.is_some(), "must parse tsconfig.json");
        let tc = result.unwrap();
        assert_eq!(tc.base_url.as_deref(), Some("."), "base_url should be '.'");
        assert!(tc.paths.contains_key("@/*"), "paths must contain @/*");
        assert!(tc.paths.contains_key("@app"), "paths must contain @app");
        let wildcard_replacements = &tc.paths["@/*"];
        assert_eq!(wildcard_replacements, &["./src/*"], "replacement for @/* should be ./src/*");
    }

    #[test]
    fn test_parse_tsconfig_paths_missing_file() {
        // Non-existent tsconfig.json returns None
        let result = parse_tsconfig_paths(Path::new("/nonexistent/path"));
        assert!(result.is_none(), "missing tsconfig should return None");
    }

    #[test]
    fn test_resolve_tsconfig_alias_with_base_url() {
        // baseUrl affects resolution path
        let mut paths = std::collections::HashMap::new();
        paths.insert("@/*".to_string(), vec!["src/*".to_string()]);
        let tsconfig = TsconfigPaths { base_url: Some(".".to_string()), paths };
        let pkg_root = Path::new("/project");
        let candidates = resolve_tsconfig_alias("@/utils", &tsconfig, pkg_root);
        let has_ts = candidates
            .iter()
            .any(|p| p.to_string_lossy().contains("src/utils"));
        assert!(has_ts, "alias with baseUrl must produce src/utils candidates; got: {candidates:?}");
    }

    // -------------------------------------------------------------------------
    // GAP-15-05: External re-export tracking tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_external_reexport_tracked() {
        // A JS file with `export * from 'lodash'` should produce *<lodash> in export map
        let dir = tempfile::tempdir().expect("tempdir");
        let index_path = dir.path().join("index.js");
        std::fs::write(&index_path, b"export * from 'lodash';\n").unwrap();

        let mut map = crate::types::ExportMap::new();
        let mut visited = HashSet::new();
        let mut warnings = Vec::new();

        collect_exports(&index_path, dir.path(), &mut map, &mut visited, &mut warnings, 0, None)
            .expect("collect_exports should succeed");

        assert!(
            map.contains_key("*<lodash>"),
            "external star re-export must produce *<lodash> key; got keys: {:?}",
            map.keys().collect::<Vec<_>>()
        );
        let sym = &map["*<lodash>"];
        assert!(sym.is_reexport, "*<lodash> entry must have is_reexport=true");
    }

    #[test]
    fn test_external_reexport_removal_detected() {
        // Old export map with *<lodash> entry, new without → export-removed finding
        use crate::types::{ExportKind, ExportSymbol};
        use std::path::PathBuf;

        let mut old_map = crate::types::ExportMap::new();
        old_map.insert(
            "*<lodash>".to_string(),
            ExportSymbol {
                name: "*<lodash>".to_string(),
                kind: ExportKind::Named,
                file: PathBuf::from("index.js"),
                line: 1,
                col: 1,
                type_signature: None,
                overloads: Vec::new(),
                decl_kind: None,
                type_guard_type: None,
                is_async: false,
                is_generator: false,
                is_reexport: true,
                is_type_only: false,
                release_tag: None,
            },
        );
        let new_map = crate::types::ExportMap::new();

        let findings = crate::compare::compare_exports(&old_map, &new_map);
        assert!(
            findings.iter().any(|f| f.rule == "export-removed" && f.message.contains("*<lodash>")),
            "removal of *<lodash> must fire export-removed; got: {:?}",
            findings
        );
    }

    #[test]
    fn test_relative_reexport_not_tracked_as_external() {
        // `export * from './utils'` where utils doesn't exist → warning, not external entry
        let dir = tempfile::tempdir().expect("tempdir");
        let index_path = dir.path().join("index.js");
        std::fs::write(&index_path, b"export * from './nonexistent';\n").unwrap();

        let mut map = crate::types::ExportMap::new();
        let mut visited = HashSet::new();
        let mut warnings = Vec::new();

        collect_exports(&index_path, dir.path(), &mut map, &mut visited, &mut warnings, 0, None)
            .expect("collect_exports should succeed");

        // No external key should be created for relative specifiers
        let has_external = map.keys().any(|k| k.starts_with("*<"));
        assert!(!has_external, "relative re-export failure must NOT produce external key; got: {:?}", map.keys().collect::<Vec<_>>());
        // Should have produced a warning instead
        assert!(!warnings.is_empty(), "failed relative re-export must produce a warning");
    }
}
