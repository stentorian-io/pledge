//! Resolve the public surface selected by Node package entry points.

pub mod entry_points;
pub mod resolver;

// Export discovery: tree-sitter parsing, entry point resolution

use crate::languages::javascript::model::ExportMap;
use crate::languages::typescript::config as tsconfig;
use crate::languages::typescript::types::reachability;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[allow(dead_code)]
/// Discover all public exports for the package rooted at `pkg_root`.
///
/// Returns the `ExportMap` of all symbols found, along with any non-fatal
/// `ResolveWarning`s collected during resolution (e.g., unresolvable files,
/// cycle detection hits, unsupported extensions).
pub fn discover_exports(
    pkg_root: &Path,
) -> anyhow::Result<(ExportMap, Vec<resolver::ResolveWarning>)> {
    let canonical_root = pkg_root.canonicalize()?;
    let pkg_root = canonical_root.as_path();
    let (mut entry_points, entry_warnings) =
        entry_points::resolve_entry_points_with_warnings(pkg_root)?;
    let package: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(pkg_root.join("package.json"))?)?;
    if package.get("exports").is_none() {
        let explicit: HashSet<_> = entry_points.iter().map(|e| e.path.clone()).collect();
        for path in entry_points::package_source_files(pkg_root) {
            let canonical = path.canonicalize()?;
            if explicit.contains(&canonical) {
                continue;
            }
            let subpath = format!("./{}", path.strip_prefix(pkg_root)?.to_string_lossy());
            entry_points.push(entry_points::EntryPoint {
                path: canonical,
                subpath,
                condition: None,
            });
        }
    }

    // Parse tsconfig.json for path alias resolution
    let tsconfig = tsconfig::parse_tsconfig_paths(pkg_root);

    // EXP-14: parse the imports field once for the entire walk.
    let imports_map = resolver::parse_imports_map_for_pkg(pkg_root);

    let mut map = ExportMap::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut warnings: Vec<_> = entry_warnings
        .into_iter()
        .map(resolver::ResolveWarning::new)
        .collect();

    let mut parse_cache = resolver::ParseCache::default();
    for entry in entry_points {
        resolver::collect_exports_cached(
            &entry.path,
            pkg_root,
            &mut map,
            &mut visited,
            &mut warnings,
            0,
            tsconfig.as_ref(),
            imports_map.as_ref(),
            &entry.subpath,
            entry.condition.as_deref(),
            &mut parse_cache,
        )?;
    }

    warnings.extend(
        reachability::resolve_public_types(&mut map, pkg_root, false)
            .into_iter()
            .map(|message| resolver::ResolveWarning {
                message,
                incomplete: true,
                type_expansion_only: true,
            }),
    );
    Ok((map, warnings))
}
