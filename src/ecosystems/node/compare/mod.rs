//! Compare Node package contracts independently of syntax-level rules.

pub mod assets;
mod commonjs;
mod conditions;
pub mod exports;
mod files;
pub mod imports;
mod loading;
pub mod metadata;
pub mod rules;

use crate::core::findings::Finding;
use std::path::Path;

/// Compare the installed Node implementation's synthetic CommonJS named imports.
/// This parses packed source and resolved re-exports without executing package code.
pub async fn compare_commonjs_named_imports(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let (before, after) = crate::ecosystems::node::discovery::commonjs::discover_named_import_pair(
        old_root, new_root,
    )
    .await;
    let current_root = new_root
        .canonicalize()
        .unwrap_or_else(|_| new_root.to_path_buf());
    commonjs::compare_named_imports(&before, &after, &current_root)
}

/// Compare exports, imports, and module loading from both package manifests.
/// The application combines these findings with syntax and type comparisons.
pub fn compare_exports_map_diff(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let old_pkg_path = old_root.join("package.json");
    let new_pkg_path = new_root.join("package.json");

    let old_content = match std::fs::read_to_string(&old_pkg_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let new_content = match std::fs::read_to_string(&new_pkg_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let old_pkg: serde_json::Value = match serde_json::from_str(&old_content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let new_pkg: serde_json::Value = match serde_json::from_str(&new_content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut findings = exports::compare_exports_diff(&old_pkg, &new_pkg, old_root, new_root);
    findings.extend(imports::compare_imports_diff(&old_pkg, &new_pkg));
    findings.extend(loading::compare_module_loading(old_root, new_root));
    findings
}
