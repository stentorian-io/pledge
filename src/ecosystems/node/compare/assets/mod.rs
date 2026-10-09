//! Compare assets and schemas distributed with a Node package.

pub mod css;
pub mod files;
pub mod json_schema;

use crate::core::findings::Finding;
use std::path::Path;

/// Orchestrates asset & schema comparison (AST-01..06).
/// Runs JSON diff, CSS diff, and asset file diff, merging all findings.
pub fn compare_asset_diff(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    // AST-01, AST-02, AST-06: JSON field diff
    findings.extend(json_schema::compare_json_files(old_root, new_root));

    // AST-03, AST-04: CSS custom property and class name diff
    findings.extend(css::compare_css_files(old_root, new_root));

    // AST-05: Asset file presence diff
    findings.extend(files::compare_asset_files(old_root, new_root));

    findings
}
