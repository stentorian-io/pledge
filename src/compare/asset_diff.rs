// Asset file diff: detect removed asset files in packed output.
//
// AST-05: asset-file-removed (asset present in old but not new — Major)
//
// Per D-07: Distinct from Phase 6's importable-file-removed. Asset removal
// and code removal are different consumer concerns.
// Per D-08: Asset extensions cover files consumers actually import/reference.
//
// Note: .json and .css files appear in both ASSET_EXTENSIONS and
// IMPORTABLE_EXTENSIONS. Duplication across rules is acceptable by design
// (D-07) — consumers can suppress one rule without affecting the other.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use crate::types::{Finding, Severity};
use super::subtype::RULE_ASSET_FILE_REMOVED;

/// Asset file extensions tracked for removal detection (D-08).
const ASSET_EXTENSIONS: &[&str] = &[
    ".json", ".css", ".wasm", ".png", ".jpg", ".svg", ".gif", ".ico",
    ".woff", ".woff2", ".ttf", ".eot",
];

/// Returns true if the given path has an asset extension.
fn is_asset(path: &Path) -> bool {
    let name = match path.to_str() {
        Some(s) => s,
        None => return false,
    };
    ASSET_EXTENSIONS.iter().any(|ext| name.ends_with(ext))
}

/// Recursively collect asset files from a package directory.
///
/// Returns relative paths (relative to `base`) as a `BTreeSet<String>` for
/// deterministic ordering. Skips `node_modules/` directories and dotfiles.
/// Explicitly excludes `package.json` (already handled by metadata.rs).
fn collect_asset_files(base: &Path, current: &Path, files: &mut BTreeSet<String>) {
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Skip dotfiles
        if file_name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            // Skip node_modules
            if file_name == "node_modules" {
                continue;
            }
            collect_asset_files(base, &path, files);
        } else if is_asset(&path) {
            if let Ok(rel) = path.strip_prefix(base) {
                if let Some(rel_str) = rel.to_str() {
                    let normalized = rel_str.replace('\\', "/");
                    // Explicitly skip package.json — handled by metadata.rs
                    if normalized == "package.json" {
                        continue;
                    }
                    files.insert(normalized);
                }
            }
        }
    }
}

/// Scan all asset files in a package root directory.
fn scan_asset_files(pkg_root: &Path) -> BTreeSet<String> {
    let mut files = BTreeSet::new();
    collect_asset_files(pkg_root, pkg_root, &mut files);
    files
}

/// Compare asset files between old and new package roots (AST-05).
///
/// Detects asset files present in `old_root` but absent in `new_root`.
/// Only removal is breaking — additions are not detected here.
pub fn compare_asset_files(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let old_assets = scan_asset_files(old_root);
    let new_assets = scan_asset_files(new_root);

    let mut findings = Vec::new();

    for file in &old_assets {
        if !new_assets.contains(file) {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_ASSET_FILE_REMOVED,
                // Asset removal is a package-level concern — anchor to package.json
                file: PathBuf::from("package.json"),
                line: 1,
                col: 1,
                message: format!("asset file `{}` removed from package", file),
            });
        }
    }

    findings
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn touch(dir: &TempDir, name: &str) {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"").unwrap();
    }

    #[test]
    fn test_asset_file_removed() {
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();
        touch(&old_dir, "icon.png");
        // new_dir does NOT have icon.png
        let findings = compare_asset_files(old_dir.path(), new_dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RULE_ASSET_FILE_REMOVED);
        assert_eq!(findings[0].severity, Severity::Major);
        assert!(findings[0].message.contains("icon.png"));
    }

    #[test]
    fn test_asset_file_present_both() {
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();
        touch(&old_dir, "icon.png");
        touch(&new_dir, "icon.png");
        let findings = compare_asset_files(old_dir.path(), new_dir.path());
        assert_eq!(findings.len(), 0, "file present in both — no finding expected");
    }

    #[test]
    fn test_non_asset_ignored() {
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();
        touch(&old_dir, "index.ts");
        // index.ts is in old but not new — non-asset, should be ignored
        let findings = compare_asset_files(old_dir.path(), new_dir.path());
        assert_eq!(findings.len(), 0, ".ts files are not assets");
    }

    #[test]
    fn test_package_json_excluded() {
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();
        touch(&old_dir, "package.json");
        touch(&new_dir, "package.json");
        // Even if package.json differs in content, asset_diff should not touch it
        let findings = compare_asset_files(old_dir.path(), new_dir.path());
        assert_eq!(findings.len(), 0, "package.json should be excluded from asset diff");
    }

    #[test]
    fn test_package_json_only_in_old() {
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();
        touch(&old_dir, "package.json");
        // package.json only in old — still excluded
        let findings = compare_asset_files(old_dir.path(), new_dir.path());
        assert_eq!(findings.len(), 0, "package.json removal should not produce asset finding");
    }

    #[test]
    fn test_all_asset_extensions() {
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();
        // Create a file for each of the 12 asset extensions in old_dir only
        let extensions = [
            "schema.json", "styles.css", "module.wasm",
            "image.png", "photo.jpg", "icon.svg", "animation.gif", "favicon.ico",
            "font.woff", "font.woff2", "font.ttf", "font.eot",
        ];
        for name in &extensions {
            touch(&old_dir, name);
        }
        let findings = compare_asset_files(old_dir.path(), new_dir.path());
        assert_eq!(findings.len(), 12, "All 12 asset extension types should produce findings");
    }

    #[test]
    fn test_asset_added_no_finding() {
        let old_dir = TempDir::new().unwrap();
        let new_dir = TempDir::new().unwrap();
        // Asset added in new but not old — should NOT produce a finding (not breaking)
        touch(&new_dir, "new-icon.svg");
        let findings = compare_asset_files(old_dir.path(), new_dir.path());
        assert_eq!(findings.len(), 0, "asset additions are not breaking");
    }
}
