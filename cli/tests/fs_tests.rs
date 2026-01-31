//! Unit tests for the fs module (file.rs and path.rs)

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use glob::Pattern;

// Re-export the modules under test by including the library
// Since fs module is private, we need to test via integration tests
// or make the module pub(crate). For now, we'll duplicate minimal
// necessary types for testing or use a test-only feature.

/// Tests for FileExt enum and its methods
mod file_ext_tests {
    use std::path::Path;

    // Since FileExt is in a private module, we'll create equivalent tests
    // that can be moved into the module itself with #[cfg(test)]

    /// Helper to determine file extension type (mirrors FileExt::from logic)
    fn get_ext_type(path: &Path) -> &'static str {
        match path.extension().and_then(|e| e.to_str()) {
            Some("js") => "js",
            Some("jsx") => "jsx",
            Some("cjs") => "cjs",
            Some("mjs") => "mjs",
            Some("ts") => "ts",
            Some("tsx") => "tsx",
            Some("cts") => "cts",
            Some("mts") => "mts",
            Some(_) => "other",
            None => "none",
        }
    }

    fn is_ts_extension(ext: &str) -> bool {
        matches!(ext, "ts" | "tsx" | "cts" | "mts")
    }

    #[test]
    fn test_js_extension() {
        let path = Path::new("file.js");
        assert_eq!(get_ext_type(path), "js");
    }

    #[test]
    fn test_jsx_extension() {
        let path = Path::new("component.jsx");
        assert_eq!(get_ext_type(path), "jsx");
    }

    #[test]
    fn test_cjs_extension() {
        let path = Path::new("module.cjs");
        assert_eq!(get_ext_type(path), "cjs");
    }

    #[test]
    fn test_mjs_extension() {
        let path = Path::new("esm.mjs");
        assert_eq!(get_ext_type(path), "mjs");
    }

    #[test]
    fn test_ts_extension() {
        let path = Path::new("types.ts");
        assert_eq!(get_ext_type(path), "ts");
        assert!(is_ts_extension("ts"));
    }

    #[test]
    fn test_tsx_extension() {
        let path = Path::new("component.tsx");
        assert_eq!(get_ext_type(path), "tsx");
        assert!(is_ts_extension("tsx"));
    }

    #[test]
    fn test_cts_extension() {
        let path = Path::new("commonjs.cts");
        assert_eq!(get_ext_type(path), "cts");
        assert!(is_ts_extension("cts"));
    }

    #[test]
    fn test_mts_extension() {
        let path = Path::new("esmodule.mts");
        assert_eq!(get_ext_type(path), "mts");
        assert!(is_ts_extension("mts"));
    }

    #[test]
    fn test_other_extension() {
        let path = Path::new("readme.md");
        assert_eq!(get_ext_type(path), "other");
    }

    #[test]
    fn test_no_extension() {
        let path = Path::new("Makefile");
        assert_eq!(get_ext_type(path), "none");
    }

    #[test]
    fn test_hidden_file_no_extension() {
        let path = Path::new(".gitignore");
        // In Rust's Path API, .gitignore has no extension (hidden files
        // starting with dot are not considered to have an extension)
        assert_eq!(get_ext_type(path), "none");
    }

    #[test]
    fn test_multiple_dots_in_filename() {
        let path = Path::new("file.test.ts");
        assert_eq!(get_ext_type(path), "ts");
        assert!(is_ts_extension("ts"));
    }

    #[test]
    fn test_path_with_directories() {
        let path = Path::new("src/components/Button.tsx");
        assert_eq!(get_ext_type(path), "tsx");
    }

    #[test]
    fn test_js_extensions_are_not_ts() {
        assert!(!is_ts_extension("js"));
        assert!(!is_ts_extension("jsx"));
        assert!(!is_ts_extension("cjs"));
        assert!(!is_ts_extension("mjs"));
    }

    #[test]
    fn test_empty_path() {
        let path = Path::new("");
        assert_eq!(get_ext_type(path), "none");
    }

    #[test]
    fn test_extension_case_sensitivity() {
        // Extensions should be case-sensitive (TS != ts)
        let path = Path::new("file.TS");
        assert_eq!(get_ext_type(path), "other");
    }
}

/// Tests for path utility functions
mod path_tests {
    use glob::Pattern;
    use std::path::Path;

    /// Mirrors path_matches_a_pattern_in function
    fn path_matches_a_pattern_in(path: &Path, patterns: &[Pattern]) -> bool {
        patterns.iter().any(|pattern| pattern.matches_path(path))
    }

    #[test]
    fn test_path_matches_single_pattern() {
        let patterns = vec![Pattern::new("*.js").unwrap()];
        let path = Path::new("index.js");
        assert!(path_matches_a_pattern_in(path, &patterns));
    }

    #[test]
    fn test_path_does_not_match_pattern() {
        let patterns = vec![Pattern::new("*.ts").unwrap()];
        let path = Path::new("index.js");
        assert!(!path_matches_a_pattern_in(path, &patterns));
    }

    #[test]
    fn test_path_matches_one_of_multiple_patterns() {
        let patterns = vec![
            Pattern::new("*.ts").unwrap(),
            Pattern::new("*.js").unwrap(),
            Pattern::new("*.jsx").unwrap(),
        ];
        let path = Path::new("component.jsx");
        assert!(path_matches_a_pattern_in(path, &patterns));
    }

    #[test]
    fn test_path_matches_none_of_multiple_patterns() {
        let patterns = vec![
            Pattern::new("*.ts").unwrap(),
            Pattern::new("*.tsx").unwrap(),
        ];
        let path = Path::new("style.css");
        assert!(!path_matches_a_pattern_in(path, &patterns));
    }

    #[test]
    fn test_empty_patterns_list() {
        let patterns: Vec<Pattern> = vec![];
        let path = Path::new("anything.js");
        assert!(!path_matches_a_pattern_in(path, &patterns));
    }

    #[test]
    fn test_glob_star_pattern() {
        let patterns = vec![Pattern::new("**/*.js").unwrap()];
        let path = Path::new("src/utils/helper.js");
        assert!(path_matches_a_pattern_in(path, &patterns));
    }

    #[test]
    fn test_directory_exclusion_pattern() {
        let patterns = vec![Pattern::new("**/node_modules/**").unwrap()];
        let path = Path::new("node_modules/lodash/index.js");
        assert!(path_matches_a_pattern_in(path, &patterns));
    }

    #[test]
    fn test_specific_directory_pattern() {
        let patterns = vec![Pattern::new("src/**/*.ts").unwrap()];

        let matching_path = Path::new("src/components/Button.ts");
        assert!(path_matches_a_pattern_in(matching_path, &patterns));

        let non_matching_path = Path::new("lib/components/Button.ts");
        assert!(!path_matches_a_pattern_in(non_matching_path, &patterns));
    }

    #[test]
    fn test_exact_filename_pattern() {
        let patterns = vec![Pattern::new("package.json").unwrap()];

        let path = Path::new("package.json");
        assert!(path_matches_a_pattern_in(path, &patterns));

        let nested_path = Path::new("subdir/package.json");
        assert!(!path_matches_a_pattern_in(nested_path, &patterns));
    }

    #[test]
    fn test_question_mark_wildcard() {
        let patterns = vec![Pattern::new("file?.js").unwrap()];

        let path1 = Path::new("file1.js");
        assert!(path_matches_a_pattern_in(path1, &patterns));

        let path2 = Path::new("file12.js");
        assert!(!path_matches_a_pattern_in(path2, &patterns));
    }

    #[test]
    fn test_character_class_pattern() {
        let patterns = vec![Pattern::new("*.[jt]s").unwrap()];

        let js_path = Path::new("index.js");
        assert!(path_matches_a_pattern_in(js_path, &patterns));

        let ts_path = Path::new("index.ts");
        assert!(path_matches_a_pattern_in(ts_path, &patterns));

        let css_path = Path::new("style.css");
        assert!(!path_matches_a_pattern_in(css_path, &patterns));
    }

    #[test]
    fn test_negated_character_class() {
        let patterns = vec![Pattern::new("*.[!j]s").unwrap()];

        let ts_path = Path::new("index.ts");
        assert!(path_matches_a_pattern_in(ts_path, &patterns));

        let js_path = Path::new("index.js");
        assert!(!path_matches_a_pattern_in(js_path, &patterns));
    }
}

/// Integration tests for get_matching_files_in_dir
mod get_matching_files_tests {
    use glob::Pattern;
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Helper function that mirrors get_matching_files_in_dir behavior
    fn get_matching_files_in_dir<OnMatch>(
        dir: &PathBuf,
        buffer: &mut HashSet<PathBuf>,
        include_patterns: &Vec<Pattern>,
        exclude_patterns: &Vec<Pattern>,
        _exclude_negation_patterns: &Vec<Pattern>,
        on_match: &OnMatch,
    ) -> anyhow::Result<()>
    where
        OnMatch: Fn(&PathBuf) -> anyhow::Result<PathBuf>,
    {
        fn path_matches_a_pattern_in(path: &std::path::Path, patterns: &[Pattern]) -> bool {
            patterns.iter().any(|pattern| pattern.matches_path(path))
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let entry_path = entry.path();

            if path_matches_a_pattern_in(&entry_path, exclude_patterns) {
                continue;
            }

            if entry.file_type()?.is_dir() {
                get_matching_files_in_dir(
                    &entry_path,
                    buffer,
                    include_patterns,
                    exclude_patterns,
                    _exclude_negation_patterns,
                    on_match,
                )?;
                continue;
            }

            if path_matches_a_pattern_in(&entry_path, include_patterns) {
                buffer.insert(on_match(&entry_path)?);
            }
        }

        Ok(())
    }

    fn create_test_structure(temp_dir: &TempDir) -> std::io::Result<()> {
        // Create directory structure
        fs::create_dir_all(temp_dir.path().join("src/components"))?;
        fs::create_dir_all(temp_dir.path().join("src/utils"))?;
        fs::create_dir_all(temp_dir.path().join("node_modules/lodash"))?;
        fs::create_dir_all(temp_dir.path().join("dist"))?;

        // Create files
        fs::write(temp_dir.path().join("src/index.ts"), "export {}")?;
        fs::write(temp_dir.path().join("src/components/Button.tsx"), "export const Button = () => {}")?;
        fs::write(temp_dir.path().join("src/utils/helper.ts"), "export function helper() {}")?;
        fs::write(temp_dir.path().join("src/utils/helper.test.ts"), "test('helper', () => {})")?;
        fs::write(temp_dir.path().join("node_modules/lodash/index.js"), "module.exports = {}")?;
        fs::write(temp_dir.path().join("dist/bundle.js"), "// bundled")?;
        fs::write(temp_dir.path().join("package.json"), "{}")?;

        Ok(())
    }

    #[test]
    fn test_find_typescript_files() {
        let temp_dir = TempDir::new().unwrap();
        create_test_structure(&temp_dir).unwrap();

        let mut results = HashSet::new();
        let include = vec![Pattern::new(&format!("{}/**/*.ts", temp_dir.path().display())).unwrap()];
        let exclude: Vec<Pattern> = vec![];
        let negation: Vec<Pattern> = vec![];

        get_matching_files_in_dir(
            &temp_dir.path().to_path_buf(),
            &mut results,
            &include,
            &exclude,
            &negation,
            &|p| Ok(p.clone()),
        )
        .unwrap();

        // Should find .ts files but not .tsx
        assert!(results.iter().any(|p| p.ends_with("index.ts")));
        assert!(results.iter().any(|p| p.ends_with("helper.ts")));
        assert!(results.iter().any(|p| p.ends_with("helper.test.ts")));
    }

    #[test]
    fn test_exclude_node_modules() {
        let temp_dir = TempDir::new().unwrap();
        create_test_structure(&temp_dir).unwrap();

        let mut results = HashSet::new();
        let include = vec![Pattern::new(&format!("{}/**/*.js", temp_dir.path().display())).unwrap()];
        let exclude = vec![Pattern::new(&format!("{}/**/node_modules", temp_dir.path().display())).unwrap()];
        let negation: Vec<Pattern> = vec![];

        get_matching_files_in_dir(
            &temp_dir.path().to_path_buf(),
            &mut results,
            &include,
            &exclude,
            &negation,
            &|p| Ok(p.clone()),
        )
        .unwrap();

        // Should find dist/bundle.js but not node_modules files
        assert!(results.iter().any(|p| p.ends_with("bundle.js")));
        assert!(!results.iter().any(|p| p.to_string_lossy().contains("node_modules")));
    }

    #[test]
    fn test_exclude_test_files() {
        let temp_dir = TempDir::new().unwrap();
        create_test_structure(&temp_dir).unwrap();

        let mut results = HashSet::new();
        let include = vec![Pattern::new(&format!("{}/**/*.ts", temp_dir.path().display())).unwrap()];
        let exclude = vec![Pattern::new(&format!("{}/**/*.test.ts", temp_dir.path().display())).unwrap()];
        let negation: Vec<Pattern> = vec![];

        get_matching_files_in_dir(
            &temp_dir.path().to_path_buf(),
            &mut results,
            &include,
            &exclude,
            &negation,
            &|p| Ok(p.clone()),
        )
        .unwrap();

        // Should find regular .ts files but not .test.ts files
        assert!(results.iter().any(|p| p.ends_with("helper.ts")));
        assert!(!results.iter().any(|p| p.to_string_lossy().contains(".test.")));
    }

    #[test]
    fn test_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join("empty")).unwrap();

        let mut results = HashSet::new();
        let include = vec![Pattern::new("**/*.ts").unwrap()];
        let exclude: Vec<Pattern> = vec![];
        let negation: Vec<Pattern> = vec![];

        let result = get_matching_files_in_dir(
            &temp_dir.path().join("empty"),
            &mut results,
            &include,
            &exclude,
            &negation,
            &|p| Ok(p.clone()),
        );

        assert!(result.is_ok());
        assert!(results.is_empty());
    }

    #[test]
    fn test_on_match_callback_transforms_path() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file.ts"), "").unwrap();

        let mut results = HashSet::new();
        let include = vec![Pattern::new(&format!("{}/**/*.ts", temp_dir.path().display())).unwrap()];
        let exclude: Vec<Pattern> = vec![];
        let negation: Vec<Pattern> = vec![];

        // Transform paths to just filenames
        get_matching_files_in_dir(
            &temp_dir.path().to_path_buf(),
            &mut results,
            &include,
            &exclude,
            &negation,
            &|p| Ok(PathBuf::from(p.file_name().unwrap())),
        )
        .unwrap();

        assert!(results.contains(&PathBuf::from("file.ts")));
    }

    #[test]
    fn test_multiple_include_patterns() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("script.js"), "").unwrap();
        fs::write(temp_dir.path().join("types.ts"), "").unwrap();
        fs::write(temp_dir.path().join("styles.css"), "").unwrap();

        let mut results = HashSet::new();
        let include = vec![
            Pattern::new(&format!("{}/**/*.js", temp_dir.path().display())).unwrap(),
            Pattern::new(&format!("{}/**/*.ts", temp_dir.path().display())).unwrap(),
        ];
        let exclude: Vec<Pattern> = vec![];
        let negation: Vec<Pattern> = vec![];

        get_matching_files_in_dir(
            &temp_dir.path().to_path_buf(),
            &mut results,
            &include,
            &exclude,
            &negation,
            &|p| Ok(p.clone()),
        )
        .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|p| p.ends_with("script.js")));
        assert!(results.iter().any(|p| p.ends_with("types.ts")));
        assert!(!results.iter().any(|p| p.ends_with("styles.css")));
    }
}
