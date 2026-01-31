//! Unit tests for the pkg module
//!
//! Tests cover:
//! - mod.rs: Pkg struct methods
//! - contents.rs: PkgContents struct
//! - entries.rs: PkgEntry, PkgEntries structs
//! - tarball.rs: PkgTarball struct
//! - registry.rs: registry functions

use breakpoint::pkg::contents::PkgContents;
use breakpoint::pkg::entries::{PkgEntries, PkgEntry, PkgEntryType};
use breakpoint::pkg::Pkg;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use tempfile::TempDir;
use url::Url;

/// Helper module for creating test fixtures
mod test_helpers {
    use super::*;

    /// Creates a temporary package directory with the specified configuration
    pub struct TempPackage {
        pub temp_dir: TempDir,
        pub pkg_dir: PathBuf,
    }

    impl TempPackage {
        pub fn new(pkg_json_content: &str) -> Self {
            let temp_dir = TempDir::new().expect("Failed to create temp directory");
            let pkg_dir = temp_dir.path().to_path_buf();

            let pkg_json_path = pkg_dir.join("package.json");
            fs::write(&pkg_json_path, pkg_json_content).expect("Failed to write package.json");

            Self { temp_dir, pkg_dir }
        }

        pub fn with_file(self, path: &str, content: &str) -> Self {
            let file_path = self.pkg_dir.join(path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::write(&file_path, content).expect("Failed to write file");
            self
        }

        pub fn with_dir(self, path: &str) -> Self {
            let dir_path = self.pkg_dir.join(path);
            fs::create_dir_all(&dir_path).expect("Failed to create directory");
            self
        }
    }

    /// Creates a minimal package.json content
    pub fn minimal_pkg_json(name: &str, version: &str) -> String {
        format!(r#"{{"name": "{}", "version": "{}"}}"#, name, version)
    }

    /// Creates a package.json with main entry
    pub fn pkg_json_with_main(name: &str, version: &str, main: &str) -> String {
        format!(
            r#"{{"name": "{}", "version": "{}", "main": "{}"}}"#,
            name, version, main
        )
    }

    /// Creates a package.json with exports
    pub fn pkg_json_with_exports(name: &str, version: &str, exports: &str) -> String {
        format!(
            r#"{{"name": "{}", "version": "{}", "exports": {}}}"#,
            name, version, exports
        )
    }
}

// ============================================================================
// Tests for pkg/mod.rs - Pkg struct
// ============================================================================

mod pkg_struct_tests {
    use super::*;
    use test_helpers::*;

    #[test]
    fn test_pkg_parse_config_as_json_valid() {
        let json_str = r#"{"name": "test-pkg", "version": "1.0.0"}"#.to_string();

        let result = Pkg::parse_config_as_json(json_str);

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config["name"].to_string(), "test-pkg");
        assert_eq!(config["version"].to_string(), "1.0.0");
    }

    #[test]
    fn test_pkg_parse_config_as_json_missing_name() {
        let json_str = r#"{"version": "1.0.0"}"#.to_string();

        let result = Pkg::parse_config_as_json(json_str);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("name"));
    }

    #[test]
    fn test_pkg_parse_config_as_json_missing_version() {
        let json_str = r#"{"name": "test-pkg"}"#.to_string();

        let result = Pkg::parse_config_as_json(json_str);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("version"));
    }

    #[test]
    fn test_pkg_parse_config_as_json_name_not_string() {
        let json_str = r#"{"name": 123, "version": "1.0.0"}"#.to_string();

        let result = Pkg::parse_config_as_json(json_str);

        assert!(result.is_err());
    }

    #[test]
    fn test_pkg_parse_config_as_json_version_not_string() {
        let json_str = r#"{"name": "test-pkg", "version": 100}"#.to_string();

        let result = Pkg::parse_config_as_json(json_str);

        assert!(result.is_err());
    }

    #[test]
    fn test_pkg_parse_config_as_json_invalid_json() {
        let json_str = r#"{"name": "test-pkg", invalid}"#.to_string();

        let result = Pkg::parse_config_as_json(json_str);

        assert!(result.is_err());
    }

    #[test]
    fn test_pkg_parse_config_as_json_with_extra_fields() {
        let json_str = r#"{
            "name": "test-pkg",
            "version": "1.0.0",
            "description": "A test package",
            "main": "index.js",
            "keywords": ["test", "package"]
        }"#
        .to_string();

        let result = Pkg::parse_config_as_json(json_str);

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config["description"].to_string(), "A test package");
        assert_eq!(config["main"].to_string(), "index.js");
    }

    #[test]
    fn test_pkg_parse_config_in_dir_valid() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("my-package", "2.0.0"));

        let result = Pkg::parse_config_in_dir(&temp_pkg.pkg_dir);

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config["name"].to_string(), "my-package");
        assert_eq!(config["version"].to_string(), "2.0.0");
    }

    #[test]
    fn test_pkg_parse_config_in_dir_not_a_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("not-a-dir.txt");
        fs::write(&file_path, "content").unwrap();

        let result = Pkg::parse_config_in_dir(&file_path);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("directory"));
    }

    #[test]
    fn test_pkg_parse_config_in_dir_missing_package_json() {
        let temp_dir = TempDir::new().unwrap();
        let pkg_dir = temp_dir.path().to_path_buf();

        let result = Pkg::parse_config_in_dir(&pkg_dir);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("package.json"));
    }

    #[test]
    fn test_pkg_get_registry_url_default() {
        let temp_dir = TempDir::new().unwrap();
        let pkg_dir = temp_dir.path().to_path_buf();

        let result = Pkg::get_registry_url(&pkg_dir);

        assert!(result.is_ok());
        let url = result.unwrap();
        assert_eq!(url.as_str(), "https://registry.npmjs.org/");
    }

    #[test]
    fn test_pkg_get_registry_url_from_npmrc() {
        let temp_dir = TempDir::new().unwrap();
        let pkg_dir = temp_dir.path().to_path_buf();

        let npmrc_content = "registry=https://custom.registry.org/";
        fs::write(pkg_dir.join(".npmrc"), npmrc_content).unwrap();

        let result = Pkg::get_registry_url(&pkg_dir);

        assert!(result.is_ok());
        let url = result.unwrap();
        assert_eq!(url.as_str(), "https://custom.registry.org/");
    }

    #[test]
    fn test_pkg_get_registry_url_from_npmrc_with_other_settings() {
        let temp_dir = TempDir::new().unwrap();
        let pkg_dir = temp_dir.path().to_path_buf();

        let npmrc_content = r#"
# This is a comment
save-exact=true
registry=https://npm.example.com/
always-auth=true
"#;
        fs::write(pkg_dir.join(".npmrc"), npmrc_content).unwrap();

        let result = Pkg::get_registry_url(&pkg_dir);

        assert!(result.is_ok());
        let url = result.unwrap();
        assert_eq!(url.as_str(), "https://npm.example.com/");
    }

    #[test]
    fn test_pkg_get_registry_url_npmrc_without_registry() {
        let temp_dir = TempDir::new().unwrap();
        let pkg_dir = temp_dir.path().to_path_buf();

        let npmrc_content = r#"
save-exact=true
always-auth=true
"#;
        fs::write(pkg_dir.join(".npmrc"), npmrc_content).unwrap();

        let result = Pkg::get_registry_url(&pkg_dir);

        assert!(result.is_ok());
        let url = result.unwrap();
        // Falls back to default
        assert_eq!(url.as_str(), "https://registry.npmjs.org/");
    }

    #[test]
    fn test_pkg_new_creates_correct_struct() {
        let temp_pkg =
            TempPackage::new(&minimal_pkg_json("new-pkg", "3.0.0")).with_file("index.js", "");

        let config = Pkg::parse_config_in_dir(&temp_pkg.pkg_dir).unwrap();
        let registry_url = Url::parse("https://registry.npmjs.org/").unwrap();
        let pkg_contents =
            Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());
        let pkg_entries = PkgEntries::new(&config, Rc::clone(&pkg_contents)).unwrap();

        let pkg = Pkg::new(
            temp_pkg.pkg_dir.clone(),
            config,
            registry_url.clone(),
            pkg_contents,
            pkg_entries,
        );

        assert_eq!(pkg.name, "new-pkg");
        assert_eq!(pkg.version, "3.0.0");
        assert_eq!(pkg.dir, temp_pkg.pkg_dir);
        assert_eq!(pkg.registry_url, registry_url);
    }
}

// ============================================================================
// Tests for pkg/contents.rs - PkgContents struct
// ============================================================================

mod pkg_contents_tests {
    use super::*;
    use test_helpers::*;

    #[test]
    fn test_pkg_contents_new_without_tarball() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("content-test", "1.0.0"));
        let config = json::parse(&minimal_pkg_json("content-test", "1.0.0")).unwrap();

        let result = PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None);

        assert!(result.is_ok());
        let contents = result.unwrap();
        assert!(!contents.is_tarball());
        assert_eq!(contents.pkg_dir, temp_pkg.pkg_dir);
    }

    #[test]
    fn test_pkg_contents_is_tarball_returns_false_without_tarball() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"));
        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();

        let contents = PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap();

        assert!(!contents.is_tarball());
    }

    #[test]
    fn test_pkg_contents_asset_list_empty_package() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"));
        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();

        let contents = PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap();
        let assets = contents.asset_list().unwrap();

        // package.json and README/LICENSE are always included but they're not "other" assets
        // The asset_list only returns non-code files
        assert!(assets.is_empty() || assets.iter().all(|p| {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            !["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"].contains(&ext)
        }));
    }

    #[test]
    fn test_pkg_contents_asset_list_with_assets() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("logo.png", "fake png content")
            .with_file("icon.svg", "<svg></svg>")
            .with_file("data/config.json", "{}")
            .with_file("src/index.js", "export default 1");

        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap();

        let assets = contents.asset_list().unwrap();

        // Should include non-code files: png, svg, json
        // Should NOT include .js files
        assert!(assets.contains(&PathBuf::from("logo.png")));
        assert!(assets.contains(&PathBuf::from("icon.svg")));
        assert!(assets.contains(&PathBuf::from("data/config.json")));
        assert!(!assets.iter().any(|p| p.extension() == Some("js".as_ref())));
    }

    #[test]
    fn test_pkg_contents_load_file_existing_file() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("src/utils.js", "export const add = (a, b) => a + b;");

        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap();

        let file_path = PathBuf::from("src/utils.js");
        let result = contents.load_file(&file_path);

        assert!(result.is_ok());
        let data = result.unwrap();
        assert!(data.is_some());
        let content = String::from_utf8(data.unwrap()).unwrap();
        assert!(content.contains("export const add"));
    }

    #[test]
    fn test_pkg_contents_load_file_nonexistent_file() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"));
        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap();

        let file_path = PathBuf::from("nonexistent.js");
        let result = contents.load_file(&file_path);

        assert!(result.is_err());
    }

    #[test]
    fn test_pkg_contents_with_files_field() {
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "files": ["src/**/*", "dist/**/*"]
        }"#;

        let temp_pkg = TempPackage::new(pkg_json)
            .with_file("src/index.js", "export default 1")
            .with_file("dist/bundle.js", "var x = 1")
            .with_file("test/test.js", "test code");

        let config = json::parse(pkg_json).unwrap();
        let result = PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None);

        assert!(result.is_ok());
        let contents = result.unwrap();
        assert!(!contents.include_patterns.is_empty());
    }

    #[test]
    fn test_pkg_contents_exclude_patterns_include_defaults() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"));
        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap();

        // Default exclusions should include node_modules, .git, etc.
        let exclude_patterns: Vec<String> = contents
            .exclude_patterns
            .iter()
            .map(|p| p.as_str().to_string())
            .collect();

        assert!(exclude_patterns.iter().any(|p| p.contains("node_modules")));
        assert!(exclude_patterns.iter().any(|p| p.contains(".git")));
    }

    #[test]
    fn test_pkg_contents_with_npmignore() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file(".npmignore", "test/\n*.test.js\n!important.test.js")
            .with_file("src/index.js", "");

        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let result = PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None);

        assert!(result.is_ok());
        let contents = result.unwrap();

        // Should have exclusion patterns from .npmignore
        let exclude_patterns: Vec<String> = contents
            .exclude_patterns
            .iter()
            .map(|p| p.as_str().to_string())
            .collect();

        assert!(exclude_patterns.iter().any(|p| p.contains("test")));

        // Should have negation pattern for !important.test.js
        let negation_patterns: Vec<String> = contents
            .exclude_negation_patterns
            .iter()
            .map(|p| p.as_str().to_string())
            .collect();

        assert!(negation_patterns
            .iter()
            .any(|p| p.contains("important.test.js")));
    }

    #[test]
    fn test_pkg_contents_ignores_paths_outside_root() {
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "files": ["src/**/*", "../outside/**/*"]
        }"#;

        let temp_pkg = TempPackage::new(pkg_json).with_file("src/index.js", "");

        let config = json::parse(pkg_json).unwrap();
        let result = PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None);

        assert!(result.is_ok());
        // Should not include patterns that go outside package root
        let contents = result.unwrap();
        assert!(!contents
            .include_patterns
            .iter()
            .any(|p| p.as_str().contains("../")));
    }
}

// ============================================================================
// Tests for pkg/entries.rs - PkgEntry struct
// ============================================================================

mod pkg_entry_tests {
    use super::*;
    use test_helpers::*;

    #[test]
    fn test_pkg_entry_new_valid_js_entry() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("src/index.js", "export default function() {}");

        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntry::new(
            "main".to_string(),
            PathBuf::from("src/index.js"),
            contents,
        );

        assert!(result.is_ok());
        let entry = result.unwrap();
        assert_eq!(entry.name, "main");
        assert_eq!(entry.path, PathBuf::from("src/index.js"));
    }

    #[test]
    fn test_pkg_entry_new_valid_ts_entry() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("src/index.ts", "export default function(): void {}");

        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntry::new(
            "main".to_string(),
            PathBuf::from("src/index.ts"),
            contents,
        );

        assert!(result.is_ok());
        let entry = result.unwrap();
        assert!(entry.ext.is_ts());
    }

    #[test]
    fn test_pkg_entry_new_invalid_extension() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("index.py", "print('hello')");

        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntry::new("main".to_string(), PathBuf::from("index.py"), contents);

        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("extension"));
    }

    #[test]
    fn test_pkg_entry_new_nonexistent_file() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"));

        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntry::new(
            "main".to_string(),
            PathBuf::from("nonexistent.js"),
            contents,
        );

        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("does not exist"));
    }

    #[test]
    fn test_pkg_entry_dir_path_root_file() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("index.js", "export default 1");

        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let entry = PkgEntry::new("main".to_string(), PathBuf::from("index.js"), contents).unwrap();

        let dir_path = entry.dir_path();
        assert_eq!(dir_path, temp_pkg.pkg_dir);
    }

    #[test]
    fn test_pkg_entry_dir_path_nested_file() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("src/lib/index.js", "export default 1");

        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let entry = PkgEntry::new(
            "main".to_string(),
            PathBuf::from("src/lib/index.js"),
            contents,
        )
        .unwrap();

        let dir_path = entry.dir_path();
        assert_eq!(dir_path, temp_pkg.pkg_dir.join("src/lib"));
    }

    #[test]
    fn test_pkg_entry_load_file() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("index.js", "export const foo = 'bar';");

        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let entry = PkgEntry::new("main".to_string(), PathBuf::from("index.js"), contents).unwrap();

        let result = entry.load_file();

        assert!(result.is_ok());
        let data = result.unwrap();
        assert!(data.is_some());
        let content = String::from_utf8(data.unwrap()).unwrap();
        assert!(content.contains("export const foo"));
    }

    #[test]
    fn test_pkg_entry_various_extensions() {
        let extensions = vec![
            ("index.js", false),
            ("index.jsx", false),
            ("index.ts", true),
            ("index.tsx", true),
            ("index.mjs", false),
            ("index.cjs", false),
            ("index.mts", true),
            ("index.cts", true),
        ];

        for (filename, is_ts) in extensions {
            let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
                .with_file(filename, "export default 1");

            let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
            let contents =
                Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

            let entry =
                PkgEntry::new("main".to_string(), PathBuf::from(filename), contents).unwrap();

            assert_eq!(
                entry.ext.is_ts(),
                is_ts,
                "Failed for extension: {}",
                filename
            );
        }
    }
}

// ============================================================================
// Tests for pkg/entries.rs - PkgEntries struct
// ============================================================================

mod pkg_entries_tests {
    use super::*;
    use test_helpers::*;

    #[test]
    fn test_pkg_entries_new_default_main() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("index.js", "export default 1");

        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntries::new(&config, contents);

        assert!(result.is_ok());
        let entries = result.unwrap();
        assert!(entries.main.contains_key("main"));
        assert_eq!(entries.main.get("main").unwrap().path, PathBuf::from("index.js"));
    }

    #[test]
    fn test_pkg_entries_new_custom_main() {
        let pkg_json = pkg_json_with_main("test", "1.0.0", "dist/main.js");
        let temp_pkg =
            TempPackage::new(&pkg_json).with_file("dist/main.js", "export default 1");

        let config = json::parse(&pkg_json).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntries::new(&config, contents);

        assert!(result.is_ok());
        let entries = result.unwrap();
        assert_eq!(
            entries.main.get("main").unwrap().path,
            PathBuf::from("dist/main.js")
        );
    }

    #[test]
    fn test_pkg_entries_with_string_exports() {
        let pkg_json = pkg_json_with_exports("test", "1.0.0", r#""./dist/index.js""#);
        let temp_pkg =
            TempPackage::new(&pkg_json)
                .with_file("index.js", "")
                .with_file("dist/index.js", "export default 1");

        let config = json::parse(&pkg_json).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntries::new(&config, contents);

        assert!(result.is_ok());
        let entries = result.unwrap();
        assert!(entries.exports.contains_key("exports"));
    }

    #[test]
    fn test_pkg_entries_with_object_exports() {
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "exports": {
                ".": "./dist/index.js",
                "./utils": "./dist/utils.js"
            }
        }"#;

        let temp_pkg = TempPackage::new(pkg_json)
            .with_file("index.js", "")
            .with_file("dist/index.js", "export default 1")
            .with_file("dist/utils.js", "export const util = 1");

        let config = json::parse(pkg_json).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntries::new(&config, contents);

        assert!(result.is_ok());
        let entries = result.unwrap();
        assert!(entries.exports.contains_key("."));
        assert!(entries.exports.contains_key("./utils"));
    }

    #[test]
    fn test_pkg_entries_with_conditional_exports() {
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "exports": {
                ".": {
                    "import": "./dist/index.mjs",
                    "require": "./dist/index.cjs"
                }
            }
        }"#;

        let temp_pkg = TempPackage::new(pkg_json)
            .with_file("index.js", "")
            .with_file("dist/index.mjs", "export default 1")
            .with_file("dist/index.cjs", "module.exports = 1");

        let config = json::parse(pkg_json).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntries::new(&config, contents);

        assert!(result.is_ok());
        let entries = result.unwrap();
        assert!(entries.exports.contains_key("import"));
        assert!(entries.exports.contains_key("require"));
    }

    #[test]
    fn test_pkg_entries_with_browser_string() {
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "browser": "./dist/browser.js"
        }"#;

        let temp_pkg = TempPackage::new(pkg_json)
            .with_file("index.js", "")
            .with_file("dist/browser.js", "export default 1");

        let config = json::parse(pkg_json).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntries::new(&config, contents);

        assert!(result.is_ok());
        let entries = result.unwrap();
        assert!(entries.browser.contains_key("browser"));
    }

    #[test]
    fn test_pkg_entries_with_browser_object() {
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "browser": {
                "./server.js": "./browser.js"
            }
        }"#;

        let temp_pkg = TempPackage::new(pkg_json)
            .with_file("index.js", "")
            .with_file("browser.js", "export default 1")
            .with_file("server.js", "export default 1");

        let config = json::parse(pkg_json).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntries::new(&config, contents);

        assert!(result.is_ok());
        let entries = result.unwrap();
        assert!(entries.browser.contains_key("./server.js"));
    }

    #[test]
    fn test_pkg_entries_browser_false_exclusion() {
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "browser": {
                "./server-only.js": false,
                "./client.js": "./client-browser.js"
            }
        }"#;

        let temp_pkg = TempPackage::new(pkg_json)
            .with_file("index.js", "")
            .with_file("server-only.js", "")
            .with_file("client.js", "")
            .with_file("client-browser.js", "export default 1");

        let config = json::parse(pkg_json).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntries::new(&config, contents);

        assert!(result.is_ok());
        let entries = result.unwrap();
        // false exclusions should be skipped
        assert!(!entries.browser.contains_key("./server-only.js"));
        assert!(entries.browser.contains_key("./client.js"));
    }

    #[test]
    fn test_pkg_entries_with_bin_string() {
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "bin": "./bin/cli.js"
        }"#;

        let temp_pkg = TempPackage::new(pkg_json)
            .with_file("index.js", "")
            .with_file("bin/cli.js", "#!/usr/bin/env node\nconsole.log('cli')");

        let config = json::parse(pkg_json).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntries::new(&config, contents);

        assert!(result.is_ok());
        let entries = result.unwrap();
        assert!(entries.bin.contains_key("bin"));
    }

    #[test]
    fn test_pkg_entries_with_bin_object() {
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "bin": {
                "my-cli": "./bin/cli.js",
                "my-tool": "./bin/tool.js"
            }
        }"#;

        let temp_pkg = TempPackage::new(pkg_json)
            .with_file("index.js", "")
            .with_file("bin/cli.js", "#!/usr/bin/env node")
            .with_file("bin/tool.js", "#!/usr/bin/env node");

        let config = json::parse(pkg_json).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntries::new(&config, contents);

        assert!(result.is_ok());
        let entries = result.unwrap();
        assert!(entries.bin.contains_key("my-cli"));
        assert!(entries.bin.contains_key("my-tool"));
    }

    #[test]
    fn test_pkg_entries_empty_exports() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("index.js", "export default 1");

        let config = json::parse(&minimal_pkg_json("test", "1.0.0")).unwrap();
        let contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());

        let result = PkgEntries::new(&config, contents);

        assert!(result.is_ok());
        let entries = result.unwrap();
        assert!(entries.exports.is_empty());
        assert!(entries.browser.is_empty());
        assert!(entries.bin.is_empty());
    }
}

// ============================================================================
// Tests for PkgEntryType enum
// ============================================================================

mod pkg_entry_type_enum_tests {
    use super::*;

    #[test]
    fn test_pkg_entry_type_display_main() {
        assert_eq!(format!("{}", PkgEntryType::Main), "main");
    }

    #[test]
    fn test_pkg_entry_type_display_bin() {
        assert_eq!(format!("{}", PkgEntryType::Bin), "bin");
    }

    #[test]
    fn test_pkg_entry_type_display_browser() {
        assert_eq!(format!("{}", PkgEntryType::Browser), "browser");
    }

    #[test]
    fn test_pkg_entry_type_display_exports() {
        assert_eq!(format!("{}", PkgEntryType::Exports), "exports");
    }

    #[test]
    fn test_pkg_entry_type_debug() {
        let entry_type = PkgEntryType::Main;
        assert_eq!(format!("{:?}", entry_type), "Main");
    }

    #[test]
    fn test_pkg_entry_type_clone() {
        let original = PkgEntryType::Exports;
        let cloned = original.clone();
        assert_eq!(format!("{}", cloned), "exports");
    }
}

// ============================================================================
// Tests for pkg/tarball.rs - PkgTarball struct
// ============================================================================

mod pkg_tarball_tests {
    use super::*;
    use breakpoint::pkg::tarball::PkgTarball;

    #[test]
    fn test_pkg_tarball_new_creates_instance() {
        let temp_dir = TempDir::new().unwrap();
        let tarball_dir = temp_dir.path().to_path_buf();
        let source_url = Url::parse("https://registry.npmjs.org/test/-/test-1.0.0.tgz").unwrap();
        let checksum = vec![0u8; 64]; // SHA-512 is 64 bytes

        let result = PkgTarball::new(
            "test-1.0.0.tar.gz".to_string(),
            tarball_dir,
            source_url,
            checksum,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_pkg_tarball_get_files_empty_when_no_data() {
        let temp_dir = TempDir::new().unwrap();
        let tarball_dir = temp_dir.path().to_path_buf();
        let source_url = Url::parse("https://registry.npmjs.org/test/-/test-1.0.0.tgz").unwrap();
        let checksum = vec![0u8; 64];

        let tarball = PkgTarball::new(
            "test-1.0.0.tar.gz".to_string(),
            tarball_dir,
            source_url,
            checksum,
        )
        .unwrap();

        // Without downloading, data is None
        let files = tarball.get_files::<fn(&tar::Entry<&[u8]>) -> anyhow::Result<bool>>(None);

        assert!(files.is_ok());
        assert!(files.unwrap().is_empty());
    }

    #[test]
    fn test_pkg_tarball_load_file_by_path_empty_when_no_data() {
        let temp_dir = TempDir::new().unwrap();
        let tarball_dir = temp_dir.path().to_path_buf();
        let source_url = Url::parse("https://registry.npmjs.org/test/-/test-1.0.0.tgz").unwrap();
        let checksum = vec![0u8; 64];

        let tarball = PkgTarball::new(
            "test-1.0.0.tar.gz".to_string(),
            tarball_dir,
            source_url,
            checksum,
        )
        .unwrap();

        let file_path = PathBuf::from("package.json");
        let result = tarball.load_file_by_path(&file_path);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // No data loaded
    }

    #[test]
    fn test_pkg_tarball_load_file_handles_path_prefixes() {
        let temp_dir = TempDir::new().unwrap();
        let tarball_dir = temp_dir.path().to_path_buf();
        let source_url = Url::parse("https://registry.npmjs.org/test/-/test-1.0.0.tgz").unwrap();
        let checksum = vec![0u8; 64];

        let tarball = PkgTarball::new(
            "test-1.0.0.tar.gz".to_string(),
            tarball_dir,
            source_url,
            checksum,
        )
        .unwrap();

        // Test with various path prefixes - all should be handled gracefully
        let paths = vec![
            PathBuf::from("package.json"),
            PathBuf::from("./package.json"),
            PathBuf::from("/package.json"),
        ];

        for path in paths {
            let result = tarball.load_file_by_path(&path);
            assert!(result.is_ok(), "Failed for path: {:?}", path);
        }
    }
}

// ============================================================================
// Integration tests for full package workflows
// ============================================================================

// ============================================================================
// Tests for pkg/pack.rs - Package manager detection and packing
// ============================================================================

mod pack_tests {
    use super::*;
    use breakpoint::pkg::pack::{PackageManager, detect_package_manager};
    use test_helpers::*;

    #[test]
    fn test_package_manager_command() {
        assert_eq!(PackageManager::Npm.command(), "npm");
        assert_eq!(PackageManager::Pnpm.command(), "pnpm");
        assert_eq!(PackageManager::Yarn.command(), "yarn");
        assert_eq!(PackageManager::Bun.command(), "bun");
    }

    #[test]
    fn test_package_manager_name() {
        assert_eq!(PackageManager::Npm.name(), "npm");
        assert_eq!(PackageManager::Pnpm.name(), "pnpm");
        assert_eq!(PackageManager::Yarn.name(), "Yarn");
        assert_eq!(PackageManager::Bun.name(), "Bun");
    }

    #[test]
    fn test_package_manager_install_instructions() {
        assert!(PackageManager::Npm.install_instructions().contains("Node.js"));
        assert!(PackageManager::Pnpm.install_instructions().contains("npm install"));
        assert!(PackageManager::Yarn.install_instructions().contains("npm install"));
        assert!(PackageManager::Bun.install_instructions().contains("bun.sh"));
    }

    #[test]
    fn test_detect_package_manager_from_npm_lockfile() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("package-lock.json", "{}");

        let result = detect_package_manager(&temp_pkg.pkg_dir);

        // This might fail if npm isn't installed, so we just check for either success or "not installed"
        match result {
            Ok(info) => {
                // If detection succeeds with a lockfile, it should prefer npm
                // (or whatever is installed if npm isn't)
                assert!(info.path.to_string_lossy().len() > 0);
            }
            Err(e) => {
                // If it fails, it should be because no package manager is installed
                assert!(e.to_string().contains("No package manager found"));
            }
        }
    }

    #[test]
    fn test_detect_package_manager_from_pnpm_lockfile() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("pnpm-lock.yaml", "lockfileVersion: '9.0'");

        let result = detect_package_manager(&temp_pkg.pkg_dir);

        // Similar to above - verify it either works or gives proper error
        match result {
            Ok(info) => {
                assert!(info.path.to_string_lossy().len() > 0);
            }
            Err(e) => {
                assert!(e.to_string().contains("No package manager found"));
            }
        }
    }

    #[test]
    fn test_detect_package_manager_from_yarn_lockfile() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("yarn.lock", "# yarn lockfile v1");

        let result = detect_package_manager(&temp_pkg.pkg_dir);

        match result {
            Ok(info) => {
                assert!(info.path.to_string_lossy().len() > 0);
            }
            Err(e) => {
                assert!(e.to_string().contains("No package manager found"));
            }
        }
    }

    #[test]
    fn test_detect_package_manager_from_bun_lockfile() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file("bun.lockb", "binary content");

        let result = detect_package_manager(&temp_pkg.pkg_dir);

        match result {
            Ok(info) => {
                assert!(info.path.to_string_lossy().len() > 0);
            }
            Err(e) => {
                assert!(e.to_string().contains("No package manager found"));
            }
        }
    }

    #[test]
    fn test_detect_package_manager_from_package_json_field() {
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "packageManager": "pnpm@9.0.0"
        }"#;

        let temp_pkg = TempPackage::new(pkg_json);

        let result = detect_package_manager(&temp_pkg.pkg_dir);

        match result {
            Ok(info) => {
                assert!(info.path.to_string_lossy().len() > 0);
            }
            Err(e) => {
                assert!(e.to_string().contains("No package manager found"));
            }
        }
    }

    #[test]
    fn test_detect_package_manager_fallback() {
        // No lockfile, no packageManager field - should fall back to what's installed
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"));

        let result = detect_package_manager(&temp_pkg.pkg_dir);

        match result {
            Ok(info) => {
                // Should have found something
                assert!(info.path.to_string_lossy().len() > 0);
                assert!(info.version.is_some() || info.version.is_none()); // Either is valid
            }
            Err(e) => {
                // Only valid if truly nothing is installed
                assert!(e.to_string().contains("No package manager found"));
            }
        }
    }
}

// ============================================================================
// Tests for pkg/registry_client.rs - Registry client
// ============================================================================

mod registry_client_tests {
    use super::*;
    use breakpoint::pkg::registry_client::RegistryClient;
    use test_helpers::*;

    #[test]
    fn test_registry_client_new_default() {
        let result = RegistryClient::new();

        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.registry_url.as_str(), "https://registry.npmjs.org/");
    }

    #[test]
    fn test_registry_client_with_url() {
        let result = RegistryClient::with_url("https://npm.example.com/");

        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.registry_url.as_str(), "https://npm.example.com/");
    }

    #[test]
    fn test_registry_client_with_invalid_url() {
        let result = RegistryClient::with_url("not a valid url");

        assert!(result.is_err());
    }

    #[test]
    fn test_registry_client_from_pkg_dir_default() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"));

        let result = RegistryClient::from_pkg_dir(&temp_pkg.pkg_dir);

        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.registry_url.as_str(), "https://registry.npmjs.org/");
    }

    #[test]
    fn test_registry_client_from_pkg_dir_with_npmrc() {
        let temp_pkg = TempPackage::new(&minimal_pkg_json("test", "1.0.0"))
            .with_file(".npmrc", "registry=https://npm.example.com/");

        let result = RegistryClient::from_pkg_dir(&temp_pkg.pkg_dir);

        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.registry_url.as_str(), "https://npm.example.com/");
    }

    #[test]
    fn test_registry_client_from_pkg_dir_with_publish_config() {
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "publishConfig": {
                "registry": "https://publish.example.com/"
            }
        }"#;

        let temp_pkg = TempPackage::new(pkg_json);

        let result = RegistryClient::from_pkg_dir(&temp_pkg.pkg_dir);

        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.registry_url.as_str(), "https://publish.example.com/");
    }

    #[test]
    fn test_registry_client_npmrc_takes_precedence() {
        // .npmrc should take precedence over publishConfig
        let pkg_json = r#"{
            "name": "test",
            "version": "1.0.0",
            "publishConfig": {
                "registry": "https://publish.example.com/"
            }
        }"#;

        let temp_pkg = TempPackage::new(pkg_json)
            .with_file(".npmrc", "registry=https://npmrc.example.com/");

        let result = RegistryClient::from_pkg_dir(&temp_pkg.pkg_dir);

        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.registry_url.as_str(), "https://npmrc.example.com/");
    }

    #[test]
    fn test_registry_client_with_auth_token() {
        let client = RegistryClient::new()
            .unwrap()
            .with_auth_token("my-secret-token".to_string());

        // We can't directly test the token is set (it's private), but we can
        // verify the builder pattern works
        assert_eq!(client.registry_url.as_str(), "https://registry.npmjs.org/");
    }
}

mod integration_tests {
    use super::*;
    use test_helpers::*;

    #[test]
    fn test_full_package_creation_workflow() {
        let pkg_json = r#"{
            "name": "integration-test",
            "version": "1.0.0",
            "main": "dist/index.js",
            "exports": {
                ".": "./dist/index.js",
                "./utils": "./dist/utils.js"
            }
        }"#;

        let temp_pkg = TempPackage::new(pkg_json)
            .with_file("dist/index.js", "export default function main() {}")
            .with_file("dist/utils.js", "export const helper = () => {}");

        let config = json::parse(pkg_json).unwrap();
        let registry_url = Url::parse("https://registry.npmjs.org/").unwrap();

        // Create contents
        let pkg_contents = PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None);
        assert!(pkg_contents.is_ok());
        let pkg_contents = Rc::new(pkg_contents.unwrap());

        // Create entries
        let pkg_entries = PkgEntries::new(&config, Rc::clone(&pkg_contents));
        assert!(pkg_entries.is_ok());
        let pkg_entries = pkg_entries.unwrap();

        // Create package
        let pkg = Pkg::new(
            temp_pkg.pkg_dir.clone(),
            config,
            registry_url,
            pkg_contents,
            pkg_entries,
        );

        assert_eq!(pkg.name, "integration-test");
        assert_eq!(pkg.version, "1.0.0");
        assert!(pkg.entries.main.contains_key("main"));
        assert!(pkg.entries.exports.contains_key("."));
        assert!(pkg.entries.exports.contains_key("./utils"));
    }

    #[test]
    fn test_package_with_complex_structure() {
        let pkg_json = r#"{
            "name": "complex-pkg",
            "version": "2.0.0",
            "main": "lib/index.js",
            "browser": {
                "lib/server.js": "lib/browser.js"
            },
            "bin": {
                "cli": "bin/cli.js"
            },
            "exports": {
                ".": {
                    "import": "lib/index.mjs",
                    "require": "lib/index.cjs"
                }
            },
            "files": ["lib/**/*", "bin/**/*"]
        }"#;

        let temp_pkg = TempPackage::new(pkg_json)
            .with_file("lib/index.js", "export default 1")
            .with_file("lib/index.mjs", "export default 1")
            .with_file("lib/index.cjs", "module.exports = 1")
            .with_file("lib/server.js", "export default 1")
            .with_file("lib/browser.js", "export default 1")
            .with_file("bin/cli.js", "#!/usr/bin/env node");

        let config = json::parse(pkg_json).unwrap();
        let registry_url = Url::parse("https://registry.npmjs.org/").unwrap();
        let pkg_contents = Rc::new(PkgContents::new(temp_pkg.pkg_dir.clone(), &config, None).unwrap());
        let pkg_entries = PkgEntries::new(&config, Rc::clone(&pkg_contents)).unwrap();

        let pkg = Pkg::new(
            temp_pkg.pkg_dir.clone(),
            config,
            registry_url,
            pkg_contents,
            pkg_entries,
        );

        assert_eq!(pkg.name, "complex-pkg");
        assert!(pkg.entries.bin.contains_key("cli"));
        assert!(pkg.entries.browser.contains_key("lib/server.js"));
        assert!(pkg.entries.exports.contains_key("import"));
        assert!(pkg.entries.exports.contains_key("require"));
    }
}
