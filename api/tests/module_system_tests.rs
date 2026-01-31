//! Module system breaking change detection tests.
//!
//! Tests for detecting breaking changes in package.json exports field,
//! module types, and entry points.

use std::collections::HashMap;

use interlint_format::{
    ExportsField, ExportsValue, PackageChangeCategory, PackageChangeSeverity, PackageConfig,
    PackageConfigChangeType, PackageJsonChange,
};

// =============================================================================
// EXPORTS SUBPATH DETECTION TESTS
// =============================================================================

mod exports_subpath_tests {
    use super::*;

    /// Helper to create conditional exports
    fn create_conditional_exports(
        entries: Vec<(&str, ExportsValue)>,
    ) -> ExportsField {
        let map: HashMap<String, ExportsValue> = entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ExportsField::Conditional(map)
    }

    #[test]
    fn test_exports_subpath_removal_is_breaking() {
        // Old: has ./utils subpath
        let old_exports = create_conditional_exports(vec![
            (".", ExportsValue::path("./index.js")),
            ("./utils", ExportsValue::path("./utils.js")),
            ("./helpers", ExportsValue::path("./helpers.js")),
        ]);

        // New: ./utils subpath removed
        let new_exports = create_conditional_exports(vec![
            (".", ExportsValue::path("./index.js")),
            ("./helpers", ExportsValue::path("./helpers.js")),
        ]);

        // Detect the removed subpath
        let old_subpaths: std::collections::HashSet<_> =
            old_exports.subpaths().into_iter().collect();
        let new_subpaths: std::collections::HashSet<_> =
            new_exports.subpaths().into_iter().collect();

        let removed: Vec<_> = old_subpaths.difference(&new_subpaths).collect();

        assert_eq!(removed.len(), 1);
        assert!(removed.contains(&&"./utils"));

        // This should be detected as a breaking change
        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::EntryPoints,
            "Export subpath './utils' was removed",
            "exports[\"./utils\"]",
        )
        .with_old_value("./utils.js");

        assert!(change.is_breaking());
    }

    #[test]
    fn test_exports_subpath_addition_is_not_breaking() {
        let old_exports = create_conditional_exports(vec![(
            ".",
            ExportsValue::path("./index.js"),
        )]);

        let new_exports = create_conditional_exports(vec![
            (".", ExportsValue::path("./index.js")),
            ("./utils", ExportsValue::path("./utils.js")),
        ]);

        let old_subpaths: std::collections::HashSet<_> =
            old_exports.subpaths().into_iter().collect();
        let new_subpaths: std::collections::HashSet<_> =
            new_exports.subpaths().into_iter().collect();

        let added: Vec<_> = new_subpaths.difference(&old_subpaths).collect();

        assert_eq!(added.len(), 1);
        assert!(added.contains(&&"./utils"));

        // Adding a subpath is safe (minor)
        let change = PackageJsonChange::new(
            PackageChangeSeverity::Safe,
            PackageChangeCategory::EntryPoints,
            "Export subpath './utils' was added",
            "exports[\"./utils\"]",
        )
        .with_new_value("./utils.js");

        assert!(!change.is_breaking());
    }

    #[test]
    fn test_exports_root_subpath_removal_is_critical() {
        let old_exports = create_conditional_exports(vec![
            (".", ExportsValue::path("./index.js")),
            ("./utils", ExportsValue::path("./utils.js")),
        ]);

        let new_exports = create_conditional_exports(vec![(
            "./utils",
            ExportsValue::path("./utils.js"),
        )]);

        let old_subpaths: std::collections::HashSet<_> =
            old_exports.subpaths().into_iter().collect();
        let new_subpaths: std::collections::HashSet<_> =
            new_exports.subpaths().into_iter().collect();

        let removed: Vec<_> = old_subpaths.difference(&new_subpaths).collect();

        assert!(removed.contains(&&"."));

        // Removing root export is critical
        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::EntryPoints,
            "Root export '.' was removed - package can no longer be directly imported",
            "exports[\".\"]",
        );

        assert!(change.is_breaking());
    }
}

// =============================================================================
// EXPORTS CONDITION DETECTION TESTS
// =============================================================================

mod exports_condition_tests {
    use super::*;

    fn create_conditional_subpath(
        conditions: Vec<(&str, &str)>,
    ) -> ExportsValue {
        let map: HashMap<String, ExportsValue> = conditions
            .into_iter()
            .map(|(cond, path)| (cond.to_string(), ExportsValue::path(path)))
            .collect();
        ExportsValue::Conditional(map)
    }

    #[test]
    fn test_exports_condition_removal_is_breaking() {
        // Old: has import and require conditions
        let old_conditions = create_conditional_subpath(vec![
            ("import", "./esm/index.js"),
            ("require", "./cjs/index.js"),
            ("types", "./index.d.ts"),
        ]);

        // New: require condition removed
        let new_conditions = create_conditional_subpath(vec![
            ("import", "./esm/index.js"),
            ("types", "./index.d.ts"),
        ]);

        let old_conds: std::collections::HashSet<_> = match &old_conditions {
            ExportsValue::Conditional(map) => map.keys().collect(),
            _ => std::collections::HashSet::new(),
        };

        let new_conds: std::collections::HashSet<_> = match &new_conditions {
            ExportsValue::Conditional(map) => map.keys().collect(),
            _ => std::collections::HashSet::new(),
        };

        let removed: Vec<_> = old_conds.difference(&new_conds).collect();

        assert_eq!(removed.len(), 1);
        assert!(removed.iter().any(|c| c.as_str() == "require"));

        // Removing require condition breaks CommonJS consumers
        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::EntryPoints,
            "Export condition 'require' was removed - CommonJS consumers will fail",
            "exports[\".\"][\"require\"]",
        );

        assert!(change.is_breaking());
    }

    #[test]
    fn test_exports_types_condition_removal_is_breaking() {
        let old_conditions = create_conditional_subpath(vec![
            ("import", "./esm/index.js"),
            ("types", "./index.d.ts"),
        ]);

        let new_conditions = create_conditional_subpath(vec![(
            "import",
            "./esm/index.js",
        )]);

        let old_conds: std::collections::HashSet<_> = match &old_conditions {
            ExportsValue::Conditional(map) => map.keys().collect(),
            _ => std::collections::HashSet::new(),
        };

        let new_conds: std::collections::HashSet<_> = match &new_conditions {
            ExportsValue::Conditional(map) => map.keys().collect(),
            _ => std::collections::HashSet::new(),
        };

        let removed: Vec<_> = old_conds.difference(&new_conds).collect();

        assert!(removed.iter().any(|c| c.as_str() == "types"));

        // Removing types condition breaks TypeScript consumers
        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::TypeScript,
            "Export condition 'types' was removed - TypeScript consumers will lose type definitions",
            "exports[\".\"][\"types\"]",
        );

        assert!(change.is_breaking());
    }

    #[test]
    fn test_exports_condition_addition_is_safe() {
        let old_conditions = create_conditional_subpath(vec![(
            "import",
            "./esm/index.js",
        )]);

        let new_conditions = create_conditional_subpath(vec![
            ("import", "./esm/index.js"),
            ("require", "./cjs/index.js"),
        ]);

        let old_conds: std::collections::HashSet<_> = match &old_conditions {
            ExportsValue::Conditional(map) => map.keys().collect(),
            _ => std::collections::HashSet::new(),
        };

        let new_conds: std::collections::HashSet<_> = match &new_conditions {
            ExportsValue::Conditional(map) => map.keys().collect(),
            _ => std::collections::HashSet::new(),
        };

        let added: Vec<_> = new_conds.difference(&old_conds).collect();

        assert!(added.iter().any(|c| c.as_str() == "require"));

        // Adding require condition expands compatibility
        let change = PackageJsonChange::new(
            PackageChangeSeverity::Safe,
            PackageChangeCategory::EntryPoints,
            "Export condition 'require' was added - CommonJS consumers now supported",
            "exports[\".\"][\"require\"]",
        );

        assert!(!change.is_breaking());
    }

    #[test]
    fn test_exports_condition_reordering_detection() {
        // Test that condition order is preserved in JSON maps
        // Note: In Rust HashMap, order is not guaranteed, but in package.json
        // the first matching condition wins

        let conditions_a = create_conditional_subpath(vec![
            ("node", "./node.js"),
            ("import", "./esm/index.js"),
            ("default", "./cjs/index.js"),
        ]);

        let conditions_b = create_conditional_subpath(vec![
            ("import", "./esm/index.js"),
            ("node", "./node.js"),
            ("default", "./cjs/index.js"),
        ]);

        // Both have same conditions, but order affects resolution
        // This is more relevant for actual JSON parsing order
        match (&conditions_a, &conditions_b) {
            (ExportsValue::Conditional(a), ExportsValue::Conditional(b)) => {
                assert_eq!(a.len(), b.len());
                // In actual detection, we'd check JSON key order
            }
            _ => panic!("Expected conditional exports"),
        }
    }
}

// =============================================================================
// NULL EXPORT DETECTION TESTS
// =============================================================================

mod null_export_tests {
    use super::*;

    #[test]
    fn test_null_export_addition_is_breaking() {
        // Adding null export blocks a previously accessible internal path
        let old_exports: HashMap<String, ExportsValue> = [
            (".".to_string(), ExportsValue::path("./index.js")),
            ("./utils".to_string(), ExportsValue::path("./utils.js")),
        ]
        .into_iter()
        .collect();

        let new_exports: HashMap<String, ExportsValue> = [
            (".".to_string(), ExportsValue::path("./index.js")),
            ("./utils".to_string(), ExportsValue::path("./utils.js")),
            ("./internal/*".to_string(), ExportsValue::Null),
        ]
        .into_iter()
        .collect();

        // Find newly added null exports
        let added_null: Vec<_> = new_exports
            .iter()
            .filter(|(k, v)| {
                v.is_null() && !old_exports.contains_key(*k)
            })
            .map(|(k, _)| k.as_str())
            .collect();

        assert!(added_null.contains(&"./internal/*"));

        // This blocks access to internal modules
        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::EntryPoints,
            "Null export added for './internal/*' - blocks access to internal modules",
            "exports[\"./internal/*\"]",
        )
        .with_new_value("null");

        assert!(change.is_breaking());
    }

    #[test]
    fn test_null_export_to_path_is_safe() {
        // Changing null to a valid path reopens access (safe)
        let old_value = ExportsValue::Null;
        let new_value = ExportsValue::path("./internal/index.js");

        assert!(old_value.is_null());
        assert!(new_value.is_path());

        let change = PackageJsonChange::new(
            PackageChangeSeverity::Safe,
            PackageChangeCategory::EntryPoints,
            "Previously blocked path './internal' is now accessible",
            "exports[\"./internal\"]",
        );

        assert!(!change.is_breaking());
    }
}

// =============================================================================
// BROWSER FIELD DETECTION TESTS
// =============================================================================

mod browser_field_tests {
    use super::*;

    /// Simulates browser field as string (main replacement)
    fn browser_as_string(path: &str) -> BrowserField {
        BrowserField::String(path.to_string())
    }

    /// Simulates browser field as object (module remapping)
    fn browser_as_object(mappings: Vec<(&str, &str)>) -> BrowserField {
        let map: HashMap<String, String> = mappings
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        BrowserField::Object(map)
    }

    #[derive(Debug, Clone)]
    enum BrowserField {
        String(String),
        Object(HashMap<String, String>),
    }

    #[test]
    fn test_browser_string_to_object_is_breaking() {
        let old_browser = browser_as_string("./browser.js");
        let new_browser = browser_as_object(vec![
            ("./index.js", "./browser.js"),
            ("./node-only.js", "false"),
        ]);

        // Structure change from string to object
        let is_type_change = matches!(
            (&old_browser, &new_browser),
            (BrowserField::String(_), BrowserField::Object(_))
        );

        assert!(is_type_change);

        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::EntryPoints,
            "Browser field changed from string to object - affects module resolution",
            "browser",
        )
        .with_old_value("./browser.js")
        .with_new_value("{object}");

        assert!(change.is_breaking());
    }

    #[test]
    fn test_browser_object_module_exclusion() {
        // "false" in browser field means module is excluded in browser builds
        let old_browser = browser_as_object(vec![
            ("./index.js", "./browser.js"),
        ]);

        let new_browser = browser_as_object(vec![
            ("./index.js", "./browser.js"),
            ("./crypto.js", "false"),
        ]);

        // Adding "false" mapping excludes a module in browsers
        let old_mappings = match &old_browser {
            BrowserField::Object(m) => m,
            _ => panic!("Expected object"),
        };
        let new_mappings = match &new_browser {
            BrowserField::Object(m) => m,
            _ => panic!("Expected object"),
        };

        let new_exclusions: Vec<_> = new_mappings
            .iter()
            .filter(|(k, v)| *v == "false" && !old_mappings.contains_key(*k))
            .collect();

        assert!(!new_exclusions.is_empty());

        // Adding browser exclusion can break if code expects the module
        let change = PackageJsonChange::new(
            PackageChangeSeverity::PotentiallyBreaking,
            PackageChangeCategory::EntryPoints,
            "Module './crypto.js' is now excluded in browser builds",
            "browser[\"./crypto.js\"]",
        )
        .with_new_value("false")
        .with_suggestion("Use a browser-compatible polyfill if needed");

        assert!(!change.is_breaking()); // PotentiallyBreaking, not Breaking
    }
}

// =============================================================================
// MODULE TYPE FIELD DETECTION TESTS
// =============================================================================

mod module_type_tests {
    use super::*;

    #[test]
    fn test_module_type_commonjs_to_module_is_breaking() {
        let old_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "1.0.0".to_string(),
            module_type: Some("commonjs".to_string()),
            ..Default::default()
        };

        let new_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "2.0.0".to_string(),
            module_type: Some("module".to_string()),
            ..Default::default()
        };

        let type_changed = old_config.module_type != new_config.module_type;
        assert!(type_changed);

        // This is a major breaking change - all .js files interpreted differently
        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::ModuleType,
            "Package type changed from 'commonjs' to 'module' - all .js files now ESM",
            "type",
        )
        .with_old_value("commonjs")
        .with_new_value("module")
        .with_suggestion("Rename CommonJS files to .cjs or update imports to use ESM syntax");

        assert!(change.is_breaking());
    }

    #[test]
    fn test_module_type_module_to_commonjs_is_breaking() {
        let old_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "1.0.0".to_string(),
            module_type: Some("module".to_string()),
            ..Default::default()
        };

        let new_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "2.0.0".to_string(),
            module_type: Some("commonjs".to_string()),
            ..Default::default()
        };

        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::ModuleType,
            "Package type changed from 'module' to 'commonjs' - all .js files now CJS",
            "type",
        )
        .with_old_value("module")
        .with_new_value("commonjs")
        .with_suggestion("Rename ESM files to .mjs or update imports to use require()");

        assert!(change.is_breaking());
    }

    #[test]
    fn test_module_type_implicit_addition_is_breaking() {
        // No type field = implicit commonjs
        let old_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "1.0.0".to_string(),
            module_type: None,
            ..Default::default()
        };

        let new_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "2.0.0".to_string(),
            module_type: Some("module".to_string()),
            ..Default::default()
        };

        // Adding "type": "module" when it was implicit commonjs is breaking
        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::ModuleType,
            "Package type added as 'module' - was implicit 'commonjs'",
            "type",
        )
        .with_old_value("(implicit commonjs)")
        .with_new_value("module");

        assert!(change.is_breaking());
    }

    #[test]
    fn test_module_type_implicit_removal_is_breaking() {
        let old_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "1.0.0".to_string(),
            module_type: Some("module".to_string()),
            ..Default::default()
        };

        let new_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "2.0.0".to_string(),
            module_type: None,
            ..Default::default()
        };

        // Removing "type": "module" reverts to implicit commonjs
        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::ModuleType,
            "Package type removed - was 'module', now implicit 'commonjs'",
            "type",
        )
        .with_old_value("module")
        .with_new_value("(implicit commonjs)");

        assert!(change.is_breaking());
    }
}

// =============================================================================
// ENTRY POINT (MAIN/MODULE) DETECTION TESTS
// =============================================================================

mod entry_point_tests {
    use super::*;

    #[test]
    fn test_main_field_removal_is_breaking() {
        let old_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "1.0.0".to_string(),
            main: Some("./index.js".to_string()),
            ..Default::default()
        };

        let new_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "2.0.0".to_string(),
            main: None,
            ..Default::default()
        };

        assert!(old_config.main_entry().is_some());
        assert!(new_config.main_entry().is_none());

        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::EntryPoints,
            "Main entry point removed - require('pkg') will fail",
            "main",
        )
        .with_old_value("./index.js");

        assert!(change.is_breaking());
    }

    #[test]
    fn test_main_field_change_is_potentially_breaking() {
        let old_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "1.0.0".to_string(),
            main: Some("./lib/index.js".to_string()),
            ..Default::default()
        };

        let new_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "2.0.0".to_string(),
            main: Some("./dist/index.js".to_string()),
            ..Default::default()
        };

        let changed = old_config.main != new_config.main;
        assert!(changed);

        // Path change might not break if exports point to same API
        let change = PackageJsonChange::new(
            PackageChangeSeverity::PotentiallyBreaking,
            PackageChangeCategory::EntryPoints,
            "Main entry point path changed",
            "main",
        )
        .with_old_value("./lib/index.js")
        .with_new_value("./dist/index.js");

        assert!(!change.is_breaking());
    }

    #[test]
    fn test_module_field_removal_is_breaking() {
        let old_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "1.0.0".to_string(),
            module: Some("./esm/index.js".to_string()),
            ..Default::default()
        };

        let new_config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "2.0.0".to_string(),
            module: None,
            ..Default::default()
        };

        assert!(old_config.module_entry().is_some());
        assert!(new_config.module_entry().is_none());

        // Bundlers rely on module field for ESM entry
        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::EntryPoints,
            "Module entry point removed - bundlers may fail to tree-shake",
            "module",
        )
        .with_old_value("./esm/index.js");

        assert!(change.is_breaking());
    }

    #[test]
    fn test_exports_takes_precedence_over_main() {
        // When exports is present, main is ignored in Node.js 12+
        let config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "1.0.0".to_string(),
            main: Some("./old-main.js".to_string()),
            exports: Some(ExportsField::simple("./new-entry.js")),
            ..Default::default()
        };

        assert!(config.has_exports());
        assert!(config.main_entry().is_some());

        // The exports field takes precedence
        let exports_path = config.exports.as_ref().and_then(|e| e.as_simple());
        assert_eq!(exports_path, Some("./new-entry.js"));
    }
}

// =============================================================================
// INTEGRATION TESTS
// =============================================================================

mod integration_tests {
    use super::*;

    #[test]
    fn test_full_exports_comparison() {
        let old_config = PackageConfig {
            name: "my-library".to_string(),
            version: "1.0.0".to_string(),
            main: Some("./dist/cjs/index.js".to_string()),
            module: Some("./dist/esm/index.js".to_string()),
            exports: Some(ExportsField::Conditional({
                let mut map = HashMap::new();

                // Root export with conditions
                let mut root = HashMap::new();
                root.insert("import".to_string(), ExportsValue::path("./dist/esm/index.js"));
                root.insert("require".to_string(), ExportsValue::path("./dist/cjs/index.js"));
                root.insert("types".to_string(), ExportsValue::path("./dist/types/index.d.ts"));
                map.insert(".".to_string(), ExportsValue::Conditional(root));

                // Utils subpath
                map.insert("./utils".to_string(), ExportsValue::path("./dist/utils.js"));

                // React subpath
                map.insert("./react".to_string(), ExportsValue::path("./dist/react.js"));

                map
            })),
            ..Default::default()
        };

        let new_config = PackageConfig {
            name: "my-library".to_string(),
            version: "2.0.0".to_string(),
            main: Some("./dist/cjs/index.js".to_string()),
            module: Some("./dist/esm/index.js".to_string()),
            exports: Some(ExportsField::Conditional({
                let mut map = HashMap::new();

                // Root export - require condition removed!
                let mut root = HashMap::new();
                root.insert("import".to_string(), ExportsValue::path("./dist/esm/index.js"));
                root.insert("types".to_string(), ExportsValue::path("./dist/types/index.d.ts"));
                map.insert(".".to_string(), ExportsValue::Conditional(root));

                // Utils subpath (still present)
                map.insert("./utils".to_string(), ExportsValue::path("./dist/utils.js"));

                // React subpath REMOVED
                // New subpath added
                map.insert("./vue".to_string(), ExportsValue::path("./dist/vue.js"));

                map
            })),
            ..Default::default()
        };

        // Detect changes
        let mut breaking_changes = Vec::new();

        // Check subpath removals
        if let (Some(ExportsField::Conditional(old)), Some(ExportsField::Conditional(new))) =
            (&old_config.exports, &new_config.exports)
        {
            for (path, _value) in old {
                if !new.contains_key(path) {
                    breaking_changes.push(format!("Subpath {} removed", path));
                }
            }
        }

        assert!(breaking_changes.iter().any(|c| c.contains("./react")));
    }

    #[test]
    fn test_empty_exports_handling() {
        let empty_exports = ExportsField::Conditional(HashMap::new());

        assert!(empty_exports.subpaths().is_empty());
        assert!(!empty_exports.is_simple());
    }

    #[test]
    fn test_nested_conditional_exports() {
        // Test deeply nested condition structure
        // "exports": { ".": { "node": { "import": "./node-esm.js", "require": "./node-cjs.js" } } }
        let mut node_conditions = HashMap::new();
        node_conditions.insert("import".to_string(), ExportsValue::path("./node-esm.js"));
        node_conditions.insert("require".to_string(), ExportsValue::path("./node-cjs.js"));

        let mut root_conditions = HashMap::new();
        root_conditions.insert("node".to_string(), ExportsValue::Conditional(node_conditions));

        let mut exports_map = HashMap::new();
        exports_map.insert(".".to_string(), ExportsValue::Conditional(root_conditions));

        let exports = ExportsField::Conditional(exports_map);

        // Should be able to detect nested structure
        let subpaths = exports.subpaths();
        assert!(subpaths.contains(&"."));
    }

    #[test]
    fn test_package_config_serialization_roundtrip() {
        let config = PackageConfig {
            name: "test-pkg".to_string(),
            version: "1.0.0".to_string(),
            main: Some("./index.js".to_string()),
            module: Some("./esm.js".to_string()),
            module_type: Some("module".to_string()),
            exports: Some(ExportsField::simple("./index.js")),
            types: Some("./index.d.ts".to_string()),
            bin: {
                let mut map = HashMap::new();
                map.insert("my-cli".to_string(), "./bin/cli.js".to_string());
                map
            },
            engines: {
                let mut map = HashMap::new();
                map.insert("node".to_string(), ">=18".to_string());
                map
            },
            ..Default::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        let decoded: PackageConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, decoded);
    }
}
