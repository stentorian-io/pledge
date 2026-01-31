//! Unit tests for the diff module
//!
//! Tests cover:
//! - analyzer.rs: get_diff_between function
//! - printer.rs: print functions (output validation)
//!
//! Note: The results and assets modules are private, so we test them
//! indirectly through the public analyzer API.

use breakpoint::diff::analyzer;
use breakpoint::diff::printer;
use breakpoint::pkg::contents::PkgContents;
use breakpoint::pkg::entries::{PkgEntries, PkgEntryType};
use breakpoint::pkg::Pkg;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Instant;
use tempfile::TempDir;
use url::Url;

/// Test helper module for creating mock packages and entries
mod test_helpers {
    use super::*;

    /// Creates a temporary package directory with the specified configuration
    pub struct TempPackage {
        pub _temp_dir: TempDir,
        pub pkg_dir: PathBuf,
    }

    impl TempPackage {
        pub fn new(pkg_json_content: &str) -> Self {
            let temp_dir = TempDir::new().expect("Failed to create temp directory");
            let pkg_dir = temp_dir.path().to_path_buf();

            let pkg_json_path = pkg_dir.join("package.json");
            fs::write(&pkg_json_path, pkg_json_content).expect("Failed to write package.json");

            Self {
                _temp_dir: temp_dir,
                pkg_dir,
            }
        }

        pub fn with_file(self, path: &str, content: &str) -> Self {
            let file_path = self.pkg_dir.join(path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::write(&file_path, content).expect("Failed to write file");
            self
        }
    }

    /// Creates a minimal package.json content
    pub fn minimal_pkg_json(name: &str, version: &str) -> String {
        format!(r#"{{"name": "{}", "version": "{}"}}"#, name, version)
    }

    /// Creates a Pkg struct from a TempPackage
    pub fn create_pkg(temp_pkg: &TempPackage, name: &str, version: &str) -> Pkg {
        let config = json::parse(&minimal_pkg_json(name, version)).unwrap();
        let registry_url = Url::parse("https://registry.npmjs.org/").unwrap();
        let pkg_contents =
            Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());
        let pkg_entries = PkgEntries::new(&config, Rc::clone(&pkg_contents)).unwrap();

        Pkg::new(
            temp_pkg.pkg_dir.clone(),
            config,
            registry_url,
            pkg_contents,
            pkg_entries,
        )
    }

    /// Creates a complete Pkg from pkg_json string
    pub fn create_pkg_from_json(temp_pkg: &TempPackage, pkg_json: &str) -> Pkg {
        let config = json::parse(pkg_json).unwrap();
        let registry_url = Url::parse("https://registry.npmjs.org/").unwrap();
        let pkg_contents =
            Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());
        let pkg_entries = PkgEntries::new(&config, Rc::clone(&pkg_contents)).unwrap();

        Pkg::new(
            temp_pkg.pkg_dir.clone(),
            config,
            registry_url,
            pkg_contents,
            pkg_entries,
        )
    }
}

// ============================================================================
// Tests for diff/analyzer.rs - get_diff_between function
// ============================================================================

mod analyzer_tests {
    use super::test_helpers::*;
    use super::*;

    #[test]
    fn test_get_diff_between_identical_packages() {
        // Create two identical packages
        let pkg_json = minimal_pkg_json("test-pkg", "1.0.0");

        let prev_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default function foo() { return 1; }");

        let curr_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default function foo() { return 1; }");

        let previous = create_pkg(&prev_pkg, "test-pkg", "1.0.0");
        let current = create_pkg(&curr_pkg, "test-pkg", "1.0.0");

        let result = analyzer::get_diff_between(previous, current);

        assert!(result.is_ok());
        let diff = result.unwrap();
        // Identical packages should have no removed assets
        assert!(diff.removed_assets.is_empty());
    }

    #[test]
    fn test_get_diff_between_with_removed_asset() {
        let pkg_json = minimal_pkg_json("test-pkg", "1.0.0");

        // Previous package has an asset file
        let prev_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default 1")
            .with_file("logo.png", "fake png data");

        // Current package doesn't have the asset file
        let curr_pkg =
            TempPackage::new(&pkg_json).with_file("index.js", "export default 1");

        let previous = create_pkg(&prev_pkg, "test-pkg", "1.0.0");
        let current = create_pkg(&curr_pkg, "test-pkg", "1.0.0");

        let result = analyzer::get_diff_between(previous, current);

        assert!(result.is_ok());
        let diff = result.unwrap();
        // Should detect the removed asset
        assert!(diff.removed_assets.iter().any(|p| p.to_string_lossy().contains("logo.png")));
    }

    #[test]
    fn test_get_diff_between_with_multiple_removed_assets() {
        let pkg_json = minimal_pkg_json("test-pkg", "1.0.0");

        let prev_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default 1")
            .with_file("logo.png", "png")
            .with_file("icon.svg", "svg")
            .with_file("data/config.json", "{}");

        let curr_pkg =
            TempPackage::new(&pkg_json).with_file("index.js", "export default 1");

        let previous = create_pkg(&prev_pkg, "test-pkg", "1.0.0");
        let current = create_pkg(&curr_pkg, "test-pkg", "1.0.0");

        let result = analyzer::get_diff_between(previous, current);

        assert!(result.is_ok());
        let diff = result.unwrap();
        // Should detect all removed assets
        assert!(diff.removed_assets.len() >= 2); // At least png, svg
    }

    #[test]
    fn test_get_diff_between_with_added_asset_no_break() {
        let pkg_json = minimal_pkg_json("test-pkg", "1.0.0");

        // Previous package has just the main entry
        let prev_pkg =
            TempPackage::new(&pkg_json).with_file("index.js", "export default 1");

        // Current package adds new assets (not a breaking change)
        let curr_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default 1")
            .with_file("new-logo.png", "new png");

        let previous = create_pkg(&prev_pkg, "test-pkg", "1.0.0");
        let current = create_pkg(&curr_pkg, "test-pkg", "1.0.0");

        let result = analyzer::get_diff_between(previous, current);

        assert!(result.is_ok());
        let diff = result.unwrap();
        // Adding new assets is not a breaking change
        assert!(diff.removed_assets.is_empty());
    }
}

// ============================================================================
// Tests for diff/printer.rs
// ============================================================================

mod printer_tests {
    use super::test_helpers::*;
    use super::*;

    #[test]
    fn test_print_exit_returns_success_when_no_issues() {
        let pkg_json = minimal_pkg_json("test-pkg", "1.0.0");
        let prev_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default function foo() {}");
        let curr_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default function foo() {}");

        let previous = create_pkg(&prev_pkg, "test-pkg", "1.0.0");
        let current = create_pkg(&curr_pkg, "test-pkg", "1.0.0");

        let diff = analyzer::get_diff_between(previous, current).unwrap();
        let start = Instant::now();

        let exit_code = printer::print_exit(&diff, start);

        // When there are no breaking changes, should return success
        assert_eq!(format!("{:?}", exit_code), format!("{:?}", ExitCode::SUCCESS));
    }

    #[test]
    fn test_print_exit_returns_failure_when_has_removed_assets() {
        let pkg_json = minimal_pkg_json("test-pkg", "1.0.0");

        let prev_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default 1")
            .with_file("important.json", "{}");

        let curr_pkg =
            TempPackage::new(&pkg_json).with_file("index.js", "export default 1");

        let previous = create_pkg(&prev_pkg, "test-pkg", "1.0.0");
        let current = create_pkg(&curr_pkg, "test-pkg", "1.0.0");

        let diff = analyzer::get_diff_between(previous, current).unwrap();
        let start = Instant::now();

        let exit_code = printer::print_exit(&diff, start);

        // When there are breaking changes, should return failure
        assert_eq!(format!("{:?}", exit_code), format!("{:?}", ExitCode::FAILURE));
    }

    #[test]
    fn test_print_asset_issues_executes_without_panic() {
        let pkg_json = minimal_pkg_json("test-pkg", "1.0.0");

        let prev_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default 1")
            .with_file("removed.png", "png data");

        let curr_pkg =
            TempPackage::new(&pkg_json).with_file("index.js", "export default 1");

        let previous = create_pkg(&prev_pkg, "test-pkg", "1.0.0");
        let current = create_pkg(&curr_pkg, "test-pkg", "1.0.0");

        let diff = analyzer::get_diff_between(previous, current).unwrap();

        // Should not panic when printing
        printer::print_asset_issues(&diff);
    }

    #[test]
    fn test_print_entry_issues_executes_without_panic() {
        let pkg_json = minimal_pkg_json("test-pkg", "1.0.0");

        let prev_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default function foo() {}");

        let curr_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default function foo() {}");

        let previous = create_pkg(&prev_pkg, "test-pkg", "1.0.0");
        let current = create_pkg(&curr_pkg, "test-pkg", "1.0.0");

        let diff = analyzer::get_diff_between(previous, current).unwrap();

        // Should not panic when printing
        printer::print_entry_issues(&diff);
    }

    #[test]
    fn test_print_functions_with_empty_diff() {
        let pkg_json = minimal_pkg_json("test-pkg", "1.0.0");

        let prev_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default function main() {}");

        let curr_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default function main() {}");

        let previous = create_pkg(&prev_pkg, "test-pkg", "1.0.0");
        let current = create_pkg(&curr_pkg, "test-pkg", "1.0.0");

        let diff = analyzer::get_diff_between(previous, current).unwrap();
        let start = Instant::now();

        // All print functions should execute without panic on empty diff
        printer::print_asset_issues(&diff);
        printer::print_entry_issues(&diff);
        let _ = printer::print_exit(&diff, start);
    }
}

// ============================================================================
// Tests for PkgEntryType (used by diff results)
// ============================================================================

mod pkg_entry_type_tests {

    #[test]
    fn test_pkg_entry_type_main_display() {
        use breakpoint::pkg::entries::PkgEntryType;
        let entry_type = PkgEntryType::Main;
        assert_eq!(format!("{}", entry_type), "main");
    }

    #[test]
    fn test_pkg_entry_type_bin_display() {
        use breakpoint::pkg::entries::PkgEntryType;
        let entry_type = PkgEntryType::Bin;
        assert_eq!(format!("{}", entry_type), "bin");
    }

    #[test]
    fn test_pkg_entry_type_browser_display() {
        use breakpoint::pkg::entries::PkgEntryType;
        let entry_type = PkgEntryType::Browser;
        assert_eq!(format!("{}", entry_type), "browser");
    }

    #[test]
    fn test_pkg_entry_type_exports_display() {
        use breakpoint::pkg::entries::PkgEntryType;
        let entry_type = PkgEntryType::Exports;
        assert_eq!(format!("{}", entry_type), "exports");
    }

    #[test]
    fn test_pkg_entry_type_clone() {
        use breakpoint::pkg::entries::PkgEntryType;
        let original = PkgEntryType::Exports;
        let cloned = original.clone();
        assert_eq!(format!("{}", cloned), "exports");
    }

    #[test]
    fn test_pkg_entry_type_debug() {
        use breakpoint::pkg::entries::PkgEntryType;
        let entry_type = PkgEntryType::Main;
        assert_eq!(format!("{:?}", entry_type), "Main");
    }
}

// ============================================================================
// Integration tests combining analyzer and printer
// ============================================================================

mod integration_tests {
    use super::test_helpers::*;
    use super::*;

    #[test]
    fn test_full_diff_workflow_no_changes() {
        let pkg_json = r#"{
            "name": "workflow-test",
            "version": "1.0.0",
            "main": "lib/index.js"
        }"#;

        let prev_pkg = TempPackage::new(pkg_json)
            .with_file("lib/index.js", "export default function main() { return 42; }");

        let curr_pkg = TempPackage::new(pkg_json)
            .with_file("lib/index.js", "export default function main() { return 42; }");

        let previous = create_pkg_from_json(&prev_pkg, pkg_json);
        let current = create_pkg_from_json(&curr_pkg, pkg_json);

        let start = Instant::now();
        let diff = analyzer::get_diff_between(previous, current).unwrap();

        printer::print_asset_issues(&diff);
        printer::print_entry_issues(&diff);
        let exit_code = printer::print_exit(&diff, start);

        assert_eq!(format!("{:?}", exit_code), format!("{:?}", ExitCode::SUCCESS));
    }

    #[test]
    fn test_full_diff_workflow_with_breaking_changes() {
        let pkg_json = r#"{
            "name": "breaking-test",
            "version": "1.0.0",
            "main": "index.js"
        }"#;

        let prev_pkg = TempPackage::new(pkg_json)
            .with_file("index.js", "export default function main() {}")
            .with_file("schema.json", "{}")
            .with_file("assets/logo.png", "png");

        let curr_pkg = TempPackage::new(pkg_json)
            .with_file("index.js", "export default function main() {}");

        let previous = create_pkg_from_json(&prev_pkg, pkg_json);
        let current = create_pkg_from_json(&curr_pkg, pkg_json);

        let start = Instant::now();
        let diff = analyzer::get_diff_between(previous, current).unwrap();

        // Should have removed assets
        assert!(!diff.removed_assets.is_empty());

        printer::print_asset_issues(&diff);
        printer::print_entry_issues(&diff);
        let exit_code = printer::print_exit(&diff, start);

        assert_eq!(format!("{:?}", exit_code), format!("{:?}", ExitCode::FAILURE));
    }

    #[test]
    fn test_diff_with_nested_directory_structure() {
        let pkg_json = minimal_pkg_json("nested-test", "1.0.0");

        let prev_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default 1")
            .with_file("assets/images/logo.png", "png")
            .with_file("assets/images/icon.svg", "svg")
            .with_file("data/config/settings.json", "{}");

        let curr_pkg = TempPackage::new(&pkg_json)
            .with_file("index.js", "export default 1")
            .with_file("assets/images/logo.png", "png");
        // Removed: icon.svg and settings.json

        let previous = create_pkg(&prev_pkg, "nested-test", "1.0.0");
        let current = create_pkg(&curr_pkg, "nested-test", "1.0.0");

        let diff = analyzer::get_diff_between(previous, current).unwrap();

        // Should detect removed nested files
        assert!(diff.removed_assets.len() >= 1);
    }
}
