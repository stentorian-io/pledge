//! Compare Node package contracts independently of syntax-level rules.

pub mod assets;
mod conditions;
pub mod exports;
mod files;
pub mod imports;
mod loading;
pub mod metadata;
pub mod rules;

use crate::core::findings::Finding;
use std::path::Path;

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
