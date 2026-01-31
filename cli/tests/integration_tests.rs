//! Integration tests for the breakpoint CLI tool.
//!
//! These tests verify the end-to-end behavior of the CLI, including:
//! - Command-line argument parsing
//! - Package loading from filesystem
//! - Error handling for various failure cases
//!
//! Note: Tests that require network access to npm registry are marked with `#[ignore]`
//! and can be run separately with `cargo test -- --ignored`

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Get the path to the test fixtures directory
fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Get the path to ecma fixtures
fn ecma_fixtures_path() -> PathBuf {
    fixtures_path().join("ecma")
}

/// Get the path to package fixtures
fn package_fixtures_path() -> PathBuf {
    fixtures_path().join("packages")
}

/// Create a Command for the breakpoint binary
fn breakpoint_cmd() -> Command {
    Command::cargo_bin("breakpoint").unwrap()
}

// =============================================================================
// CLI Argument Parsing Tests
// =============================================================================

mod cli_args {
    use super::*;

    #[test]
    fn test_no_arguments_shows_error() {
        breakpoint_cmd()
            .assert()
            .failure()
            .stderr(predicate::str::contains("Expected only one argument"));
    }

    #[test]
    fn test_too_many_arguments_shows_error() {
        breakpoint_cmd()
            .arg("arg1")
            .arg("arg2")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Expected only one argument"));
    }

    #[test]
    fn test_nonexistent_path_shows_error() {
        breakpoint_cmd()
            .arg("/nonexistent/path/to/package")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Expected package path to be a directory"));
    }

    #[test]
    fn test_file_path_instead_of_directory_shows_error() {
        let file_path = ecma_fixtures_path().join("simple_exports.js");

        breakpoint_cmd()
            .arg(file_path)
            .assert()
            .failure()
            .stderr(predicate::str::contains("Expected package path to be a directory"));
    }
}

// =============================================================================
// Package Loading Tests
// =============================================================================

mod package_loading {
    use super::*;

    #[test]
    fn test_directory_without_package_json_shows_error() {
        let temp_dir = TempDir::new().unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("package.json"));
    }

    #[test]
    fn test_invalid_package_json_name_field() {
        let pkg_path = package_fixtures_path().join("invalid-pkg-json");

        breakpoint_cmd()
            .arg(&pkg_path)
            .assert()
            .failure()
            .stderr(predicate::str::contains("name").or(predicate::str::contains("string")));
    }

    #[test]
    fn test_missing_entry_file_shows_error() {
        let pkg_path = package_fixtures_path().join("missing-entry");

        breakpoint_cmd()
            .arg(&pkg_path)
            .assert()
            .failure()
            .stderr(predicate::str::contains("does not exist"));
    }

    #[test]
    fn test_valid_package_prints_registry_url() {
        // This test will fail at the registry fetch stage, but we can verify
        // that package loading succeeded by checking for the registry URL message
        let pkg_path = package_fixtures_path().join("valid-pkg");

        breakpoint_cmd()
            .arg(&pkg_path)
            .assert()
            .failure() // Will fail at network request
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_package_with_exports_field() {
        let pkg_path = package_fixtures_path().join("with-exports");

        breakpoint_cmd()
            .arg(&pkg_path)
            .assert()
            .failure() // Will fail at network request
            .stdout(predicate::str::contains("registry"));
    }
}

// =============================================================================
// Custom Registry Tests
// =============================================================================

mod registry_config {
    use super::*;

    #[test]
    fn test_custom_registry_from_npmrc() {
        let temp_dir = TempDir::new().unwrap();

        // Create package.json
        let pkg_json = r#"{
            "name": "test-custom-registry",
            "version": "1.0.0",
            "main": "index.js",
            "files": ["index.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();

        // Create index.js
        let index_js = "export default function() {}";
        fs::write(temp_dir.path().join("index.js"), index_js).unwrap();

        // Create .npmrc with custom registry
        let npmrc = "registry=https://custom.registry.example.com/";
        fs::write(temp_dir.path().join(".npmrc"), npmrc).unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure() // Will fail at network request
            .stdout(predicate::str::contains("custom.registry.example.com"));
    }

    #[test]
    fn test_default_registry_when_no_npmrc() {
        let temp_dir = TempDir::new().unwrap();

        // Create package.json
        let pkg_json = r#"{
            "name": "test-default-registry",
            "version": "1.0.0",
            "main": "index.js",
            "files": ["index.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();

        // Create index.js
        let index_js = "export default function() {}";
        fs::write(temp_dir.path().join("index.js"), index_js).unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure() // Will fail at network request
            .stdout(predicate::str::contains("registry.npmjs.org"));
    }

    #[test]
    fn test_npmrc_without_registry_uses_default() {
        let temp_dir = TempDir::new().unwrap();

        // Create package.json
        let pkg_json = r#"{
            "name": "test-npmrc-no-registry",
            "version": "1.0.0",
            "main": "index.js",
            "files": ["index.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();

        // Create index.js
        let index_js = "export default function() {}";
        fs::write(temp_dir.path().join("index.js"), index_js).unwrap();

        // Create .npmrc without registry line
        let npmrc = "# Just a comment\nsave-exact=true\n";
        fs::write(temp_dir.path().join(".npmrc"), npmrc).unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure() // Will fail at network request
            .stdout(predicate::str::contains("registry.npmjs.org"));
    }
}

// =============================================================================
// Package Entry Point Tests
// =============================================================================

mod entry_points {
    use super::*;

    #[test]
    fn test_main_entry_is_required() {
        let temp_dir = TempDir::new().unwrap();

        // Package without main field uses default index.js
        let pkg_json = r#"{
            "name": "test-no-main",
            "version": "1.0.0",
            "files": ["index.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();

        // No index.js file - should fail
        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("index.js").or(predicate::str::contains("does not exist")));
    }

    #[test]
    fn test_default_index_js_entry() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-default-index",
            "version": "1.0.0",
            "files": ["index.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();

        // Create default index.js
        let index_js = "export const foo = 42;";
        fs::write(temp_dir.path().join("index.js"), index_js).unwrap();

        // Should get past entry validation
        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure() // Will fail at network request
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_custom_main_entry() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-custom-main",
            "version": "1.0.0",
            "main": "src/entry.js",
            "files": ["src/**/*.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();

        // Create src directory and entry
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        let entry_js = "export default function entry() { return 'entry'; }";
        fs::write(temp_dir.path().join("src/entry.js"), entry_js).unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure() // Will fail at network request
            .stdout(predicate::str::contains("registry"));
    }
}

// =============================================================================
// File Extension Tests
// =============================================================================

mod file_extensions {
    use super::*;

    #[test]
    fn test_js_extension_accepted() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-js-ext",
            "version": "1.0.0",
            "main": "index.js",
            "files": ["index.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.js"), "export const x = 1;").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_mjs_extension_accepted() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-mjs-ext",
            "version": "1.0.0",
            "main": "index.mjs",
            "files": ["index.mjs"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.mjs"), "export const x = 1;").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_cjs_extension_accepted() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-cjs-ext",
            "version": "1.0.0",
            "main": "index.cjs",
            "files": ["index.cjs"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(
            temp_dir.path().join("index.cjs"),
            "module.exports = { x: 1 };",
        )
        .unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_ts_extension_accepted() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-ts-ext",
            "version": "1.0.0",
            "main": "index.ts",
            "files": ["index.ts"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(
            temp_dir.path().join("index.ts"),
            "export const x: number = 1;",
        )
        .unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_invalid_extension_rejected() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-invalid-ext",
            "version": "1.0.0",
            "main": "index.txt",
            "files": ["index.txt"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.txt"), "not javascript").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("extension").or(predicate::str::contains("Invalid")));
    }
}

// =============================================================================
// Files Field Tests
// =============================================================================

mod files_field {
    use super::*;

    #[test]
    fn test_entry_must_be_in_files() {
        let temp_dir = TempDir::new().unwrap();

        // Package with files field that doesn't include main entry
        let pkg_json = r#"{
            "name": "test-files-mismatch",
            "version": "1.0.0",
            "main": "index.js",
            "files": ["lib/**/*.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.js"), "export const x = 1;").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("files").or(predicate::str::contains("not in")));
    }

    #[test]
    fn test_empty_files_field_includes_all() {
        let temp_dir = TempDir::new().unwrap();

        // Package without files field should include all files
        let pkg_json = r#"{
            "name": "test-no-files-field",
            "version": "1.0.0",
            "main": "index.js"
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.js"), "export const x = 1;").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }
}

// =============================================================================
// Browser Field Tests
// =============================================================================

mod browser_field {
    use super::*;

    #[test]
    fn test_browser_field_as_string() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-browser-string",
            "version": "1.0.0",
            "main": "index.js",
            "browser": "browser.js",
            "files": ["index.js", "browser.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.js"), "export const x = 1;").unwrap();
        fs::write(temp_dir.path().join("browser.js"), "export const y = 2;").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_browser_field_as_object() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-browser-object",
            "version": "1.0.0",
            "main": "index.js",
            "browser": {
                "server-only": "./browser-shim.js"
            },
            "files": ["index.js", "browser-shim.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.js"), "export const x = 1;").unwrap();
        fs::write(temp_dir.path().join("browser-shim.js"), "export const shim = true;").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_browser_field_exclusion() {
        let temp_dir = TempDir::new().unwrap();

        // Browser field with false value (exclusion) should be skipped
        let pkg_json = r#"{
            "name": "test-browser-exclusion",
            "version": "1.0.0",
            "main": "index.js",
            "browser": {
                "fs": false,
                "path": false
            },
            "files": ["index.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.js"), "export const x = 1;").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }
}

// =============================================================================
// Exports Field Tests
// =============================================================================

mod exports_field {
    use super::*;

    #[test]
    fn test_exports_field_as_string() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-exports-string",
            "version": "1.0.0",
            "main": "index.js",
            "exports": "./dist/index.js",
            "files": ["index.js", "dist/**/*.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.js"), "export const x = 1;").unwrap();
        fs::create_dir(temp_dir.path().join("dist")).unwrap();
        fs::write(temp_dir.path().join("dist/index.js"), "export const y = 2;").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_exports_field_with_subpath() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-exports-subpath",
            "version": "1.0.0",
            "main": "index.js",
            "exports": {
                ".": "./index.js",
                "./utils": "./utils.js"
            },
            "files": ["index.js", "utils.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.js"), "export const x = 1;").unwrap();
        fs::write(temp_dir.path().join("utils.js"), "export function util() {}").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_exports_field_conditional() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-exports-conditional",
            "version": "1.0.0",
            "main": "index.js",
            "exports": {
                ".": {
                    "import": "./index.mjs",
                    "require": "./index.cjs"
                }
            },
            "files": ["index.js", "index.mjs", "index.cjs"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.js"), "export const x = 1;").unwrap();
        fs::write(temp_dir.path().join("index.mjs"), "export const x = 1;").unwrap();
        fs::write(temp_dir.path().join("index.cjs"), "module.exports = { x: 1 };").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }
}

// =============================================================================
// Network/Registry Tests (Ignored by default - require network)
// =============================================================================

mod registry_network {
    use super::*;

    /// Test against a real npm package
    /// Run with: cargo test registry_network -- --ignored
    #[test]
    #[ignore]
    fn test_real_package_fetch() {
        let temp_dir = TempDir::new().unwrap();

        // Create a minimal package that mimics a real npm package
        let pkg_json = r#"{
            "name": "is-odd",
            "version": "999.0.0",
            "main": "index.js",
            "files": ["index.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(
            temp_dir.path().join("index.js"),
            "module.exports = (n) => n % 2 === 1;",
        )
        .unwrap();

        // This should succeed in fetching the package info from npm
        // but may fail on tarball verification since our version doesn't exist
        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure(); // Will fail because version 999.0.0 doesn't exist
    }
}

// =============================================================================
// Edge Cases and Error Handling
// =============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn test_empty_package_json() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(temp_dir.path().join("package.json"), "{}").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("name").or(predicate::str::contains("string")));
    }

    #[test]
    fn test_malformed_json() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(temp_dir.path().join("package.json"), "{ invalid json }").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure();
    }

    #[test]
    fn test_package_name_with_scope() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "@scope/package-name",
            "version": "1.0.0",
            "main": "index.js",
            "files": ["index.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.js"), "export const x = 1;").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_version_without_patch() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-version",
            "version": "1.0",
            "main": "index.js",
            "files": ["index.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(temp_dir.path().join("index.js"), "export const x = 1;").unwrap();

        // Version format is lenient - should still work
        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_deeply_nested_entry() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-deep-entry",
            "version": "1.0.0",
            "main": "src/lib/deep/nested/index.js",
            "files": ["src/**/*.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();

        // Create nested structure
        let nested_dir = temp_dir.path().join("src/lib/deep/nested");
        fs::create_dir_all(&nested_dir).unwrap();
        fs::write(nested_dir.join("index.js"), "export const deep = true;").unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_unicode_in_exports() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-unicode",
            "version": "1.0.0",
            "main": "index.js",
            "files": ["index.js"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(
            temp_dir.path().join("index.js"),
            "export const greeting = '\u{4f60}\u{597d}'; // Hello in Chinese",
        )
        .unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }
}

// =============================================================================
// TypeScript-specific Tests
// =============================================================================

mod typescript {
    use super::*;

    #[test]
    fn test_tsx_extension_accepted() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-tsx",
            "version": "1.0.0",
            "main": "index.tsx",
            "files": ["index.tsx"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(
            temp_dir.path().join("index.tsx"),
            "export const Component = () => <div>Hello</div>;",
        )
        .unwrap();

        // TSX files should be parsed correctly with JSX support enabled
        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_mts_extension_accepted() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-mts",
            "version": "1.0.0",
            "main": "index.mts",
            "files": ["index.mts"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(
            temp_dir.path().join("index.mts"),
            "export const x: number = 1;",
        )
        .unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }

    #[test]
    fn test_cts_extension_accepted() {
        let temp_dir = TempDir::new().unwrap();

        let pkg_json = r#"{
            "name": "test-cts",
            "version": "1.0.0",
            "main": "index.cts",
            "files": ["index.cts"]
        }"#;
        fs::write(temp_dir.path().join("package.json"), pkg_json).unwrap();
        fs::write(
            temp_dir.path().join("index.cts"),
            "const x: number = 1; module.exports = { x };",
        )
        .unwrap();

        breakpoint_cmd()
            .arg(temp_dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::contains("registry"));
    }
}
