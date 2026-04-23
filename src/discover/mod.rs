// Export discovery: tree-sitter parsing, entry point resolution
pub mod comment_scan;
pub mod entry_points;
pub mod resolver;
pub mod tree_query;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::types::ExportMap;

#[allow(dead_code)]
/// Discover all public exports for the package rooted at `pkg_root`.
///
/// Returns the `ExportMap` of all symbols found, along with any non-fatal
/// `ResolveWarning`s collected during resolution (e.g., unresolvable files,
/// cycle detection hits, unsupported extensions).
pub fn discover_exports(
    pkg_root: &Path,
) -> anyhow::Result<(ExportMap, Vec<resolver::ResolveWarning>)> {
    let entry_points = entry_points::resolve_entry_points(pkg_root)?;

    // GAP-15-04: Parse tsconfig.json for path alias resolution
    let tsconfig = resolver::parse_tsconfig_paths(pkg_root);

    let mut map = ExportMap::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut warnings: Vec<resolver::ResolveWarning> = Vec::new();

    for entry in entry_points {
        resolver::collect_exports(
            &entry.path,
            pkg_root,
            &mut map,
            &mut visited,
            &mut warnings,
            0,
            tsconfig.as_ref(),
            &entry.subpath,
            entry.condition.as_deref(),
        )?;
    }

    Ok((map, warnings))
}
