//! Integration tests for breaking change detection.
//!
//! These tests validate end-to-end scenarios including:
//! - Full package.json exports field comparison
//! - Real-world breaking change scenarios
//! - Edge cases and complex nested conditions

use std::collections::HashMap;

use interlint_format::{
    ExportsField, ExportsValue, PackageConfig, PackageChangeCategory, PackageChangeSeverity,
    PackageJsonChange, TypeInfo, TypeKind, Severity,
    changes::{
        BreakingChange, CodeChange, AssetChange, Change, ChangeReport, ChangeKind,
        SignatureChange, TypeChange, ExportRemovedChange,
        ReturnTypeChange, ParameterChange, ParameterChangeType, AssetFormatChange,
    },
};

// =============================================================================
// FULL PACKAGE.JSON EXPORTS FIELD COMPARISON TESTS
// =============================================================================

mod full_exports_comparison_tests {
    use super::*;

    fn build_complex_exports() -> ExportsField {
        let mut map = HashMap::new();

        // Root export with full conditions
        let mut root = HashMap::new();
        root.insert("import".to_string(), ExportsValue::path("./dist/esm/index.js"));
        root.insert("require".to_string(), ExportsValue::path("./dist/cjs/index.js"));
        root.insert("types".to_string(), ExportsValue::path("./dist/types/index.d.ts"));
        root.insert("default".to_string(), ExportsValue::path("./dist/cjs/index.js"));
        map.insert(".".to_string(), ExportsValue::Conditional(root));

        // Subpaths
        map.insert("./utils".to_string(), ExportsValue::path("./dist/utils.js"));
        map.insert("./react".to_string(), ExportsValue::path("./dist/react/index.js"));
        map.insert("./vue".to_string(), ExportsValue::path("./dist/vue/index.js"));

        // Conditional subpath
        let mut server = HashMap::new();
        server.insert("node".to_string(), ExportsValue::path("./dist/server.js"));
        server.insert("default".to_string(), ExportsValue::Null);
        map.insert("./server".to_string(), ExportsValue::Conditional(server));

        // Block internal access
        map.insert("./internal/*".to_string(), ExportsValue::Null);

        ExportsField::Conditional(map)
    }

    #[test]
    fn test_full_exports_field_parsing() {
        let exports = build_complex_exports();

        let subpaths = exports.subpaths();

        assert!(subpaths.contains(&"."));
        assert!(subpaths.contains(&"./utils"));
        assert!(subpaths.contains(&"./react"));
        assert!(subpaths.contains(&"./vue"));
        assert!(subpaths.contains(&"./server"));
        assert!(subpaths.contains(&"./internal/*"));
    }

    #[test]
    fn test_exports_field_diff_detects_removals() {
        let old_exports = build_complex_exports();

        // Remove ./vue subpath
        let mut new_map = HashMap::new();
        new_map.insert(".".to_string(), {
            let mut root = HashMap::new();
            root.insert("import".to_string(), ExportsValue::path("./dist/esm/index.js"));
            root.insert("require".to_string(), ExportsValue::path("./dist/cjs/index.js"));
            root.insert("types".to_string(), ExportsValue::path("./dist/types/index.d.ts"));
            ExportsValue::Conditional(root)
        });
        new_map.insert("./utils".to_string(), ExportsValue::path("./dist/utils.js"));
        new_map.insert("./react".to_string(), ExportsValue::path("./dist/react/index.js"));
        // ./vue removed
        new_map.insert("./server".to_string(), {
            let mut server = HashMap::new();
            server.insert("node".to_string(), ExportsValue::path("./dist/server.js"));
            ExportsValue::Conditional(server)
        });

        let new_exports = ExportsField::Conditional(new_map);

        // Detect removed subpaths
        let old_subpaths: std::collections::HashSet<_> = old_exports.subpaths().into_iter().collect();
        let new_subpaths: std::collections::HashSet<_> = new_exports.subpaths().into_iter().collect();

        let removed: Vec<_> = old_subpaths.difference(&new_subpaths).collect();

        assert!(removed.contains(&&"./vue"));
        assert!(removed.contains(&&"./internal/*"));
    }

    #[test]
    fn test_exports_condition_diff() {
        let old_exports = ExportsField::Conditional({
            let mut root = HashMap::new();
            root.insert("import".to_string(), ExportsValue::path("./esm.js"));
            root.insert("require".to_string(), ExportsValue::path("./cjs.js"));
            root.insert("types".to_string(), ExportsValue::path("./types.d.ts"));
            let mut map = HashMap::new();
            map.insert(".".to_string(), ExportsValue::Conditional(root));
            map
        });

        let new_exports = ExportsField::Conditional({
            let mut root = HashMap::new();
            root.insert("import".to_string(), ExportsValue::path("./esm.js"));
            // require removed
            root.insert("types".to_string(), ExportsValue::path("./types.d.ts"));
            let mut map = HashMap::new();
            map.insert(".".to_string(), ExportsValue::Conditional(root));
            map
        });

        // Extract conditions from root export
        let old_conditions = match &old_exports {
            ExportsField::Conditional(map) => match map.get(".") {
                Some(ExportsValue::Conditional(conds)) => {
                    conds.keys().cloned().collect::<std::collections::HashSet<_>>()
                }
                _ => std::collections::HashSet::new(),
            },
            _ => std::collections::HashSet::new(),
        };

        let new_conditions = match &new_exports {
            ExportsField::Conditional(map) => match map.get(".") {
                Some(ExportsValue::Conditional(conds)) => {
                    conds.keys().cloned().collect::<std::collections::HashSet<_>>()
                }
                _ => std::collections::HashSet::new(),
            },
            _ => std::collections::HashSet::new(),
        };

        let removed: Vec<_> = old_conditions.difference(&new_conditions).collect();
        assert!(removed.iter().any(|c| *c == "require"));
    }
}

// =============================================================================
// REAL-WORLD BREAKING CHANGE SCENARIOS
// =============================================================================

mod real_world_scenarios {
    use super::*;

    /// Scenario: Library drops CommonJS support (ESM-only)
    #[test]
    fn test_library_drops_commonjs_support() {
        let old_config = PackageConfig {
            name: "my-lib".to_string(),
            version: "1.0.0".to_string(),
            main: Some("./dist/cjs/index.js".to_string()),
            module: Some("./dist/esm/index.js".to_string()),
            module_type: None, // Implicit CommonJS
            exports: Some(ExportsField::Conditional({
                let mut root = HashMap::new();
                root.insert("import".to_string(), ExportsValue::path("./dist/esm/index.js"));
                root.insert("require".to_string(), ExportsValue::path("./dist/cjs/index.js"));
                let mut map = HashMap::new();
                map.insert(".".to_string(), ExportsValue::Conditional(root));
                map
            })),
            ..Default::default()
        };

        let new_config = PackageConfig {
            name: "my-lib".to_string(),
            version: "2.0.0".to_string(),
            main: None, // Removed
            module: Some("./dist/index.js".to_string()),
            module_type: Some("module".to_string()), // Explicit ESM
            exports: Some(ExportsField::simple("./dist/index.js")),
            ..Default::default()
        };

        let mut changes = Vec::new();

        // Detect main field removal
        if old_config.main.is_some() && new_config.main.is_none() {
            changes.push(PackageJsonChange::new(
                PackageChangeSeverity::Breaking,
                PackageChangeCategory::EntryPoints,
                "Main entry point removed - require('my-lib') will fail",
                "main",
            ));
        }

        // Detect type field change
        if old_config.module_type.is_none() && new_config.module_type == Some("module".to_string()) {
            changes.push(PackageJsonChange::new(
                PackageChangeSeverity::Breaking,
                PackageChangeCategory::ModuleType,
                "Package type changed to 'module' - CJS require() no longer works",
                "type",
            ));
        }

        assert!(!changes.is_empty());
        assert!(changes.iter().all(|c| c.is_breaking()));
    }

    /// Scenario: TypeScript library removes type definitions
    #[test]
    fn test_library_removes_type_definitions() {
        let old_config = PackageConfig {
            name: "typed-lib".to_string(),
            version: "1.0.0".to_string(),
            types: Some("./dist/index.d.ts".to_string()),
            exports: Some(ExportsField::Conditional({
                let mut root = HashMap::new();
                root.insert("import".to_string(), ExportsValue::path("./dist/index.js"));
                root.insert("types".to_string(), ExportsValue::path("./dist/index.d.ts"));
                let mut map = HashMap::new();
                map.insert(".".to_string(), ExportsValue::Conditional(root));
                map
            })),
            ..Default::default()
        };

        let new_config = PackageConfig {
            name: "typed-lib".to_string(),
            version: "2.0.0".to_string(),
            types: None, // Removed
            exports: Some(ExportsField::simple("./dist/index.js")),
            ..Default::default()
        };

        let mut changes = Vec::new();

        if old_config.types.is_some() && new_config.types.is_none() {
            changes.push(PackageJsonChange::new(
                PackageChangeSeverity::Breaking,
                PackageChangeCategory::TypeScript,
                "Type definitions removed - TypeScript users will lose type safety",
                "types",
            ));
        }

        assert!(!changes.is_empty());
        assert!(changes[0].is_breaking());
    }

    /// Scenario: API signature change (breaking function change)
    #[test]
    fn test_api_signature_breaking_change() {
        let mut report = ChangeReport::new("api-lib", "1.0.0", "2.0.0");

        // Function parameters reordered and type changed
        report.add_change(BreakingChange::Code(CodeChange::SignatureChanged(
            SignatureChange {
                name: "fetchUser".to_string(),
                module_path: "src/api.ts".to_string(),
                old_signature: "fetchUser(id: string, options?: Options): User".to_string(),
                new_signature: "fetchUser(options: Options, id: number): Promise<User>".to_string(),
                change_details: vec![
                    "Parameter order changed".to_string(),
                    "ID type changed from string to number".to_string(),
                    "Return type changed to Promise".to_string(),
                    "Options parameter became required".to_string(),
                ],
                parameter_changes: vec![
                    ParameterChange {
                        name: "id".to_string(),
                        change_type: ParameterChangeType::TypeChanged,
                        old_type: Some(TypeInfo::primitive("string")),
                        new_type: Some(TypeInfo::primitive("number")),
                        old_position: Some(0),
                        new_position: Some(1),
                    },
                    ParameterChange {
                        name: "options".to_string(),
                        change_type: ParameterChangeType::BecameRequired,
                        old_type: Some(TypeInfo::reference("Options")),
                        new_type: Some(TypeInfo::reference("Options")),
                        old_position: Some(1),
                        new_position: Some(0),
                    },
                ],
                return_type_change: Some(ReturnTypeChange {
                    old_type: Some(TypeInfo::reference("User")),
                    new_type: Some(TypeInfo {
                        kind: TypeKind::Async,
                        repr: "Promise<User>".to_string(),
                        type_arguments: vec![TypeInfo::reference("User")],
                        ..Default::default()
                    }),
                    became_async: true,
                    was_async: false,
                }),
                migration_hint: Some(
                    "Update call sites: fetchUser(options, parseInt(id)) and add await"
                        .to_string(),
                ),
            },
        )));

        assert!(report.has_breaking_changes());
        assert_eq!(report.recommended_bump(), "major");
        assert_eq!(report.summary.code_count, 1);
        assert_eq!(report.summary.major_count, 1);
    }

    /// Scenario: Type definition narrowed (breaking for consumers)
    #[test]
    fn test_type_definition_narrowed() {
        let mut report = ChangeReport::new("types-lib", "1.0.0", "2.0.0");

        // Union type member removed
        report.add_change(BreakingChange::Code(CodeChange::TypeChanged(TypeChange {
            name: "Status".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Union member 'pending' removed".to_string(),
            old_type: Some(TypeInfo {
                kind: TypeKind::Union,
                repr: "'success' | 'error' | 'pending'".to_string(),
                constituents: vec![
                    TypeInfo::primitive("'success'"),
                    TypeInfo::primitive("'error'"),
                    TypeInfo::primitive("'pending'"),
                ],
                ..Default::default()
            }),
            new_type: Some(TypeInfo {
                kind: TypeKind::Union,
                repr: "'success' | 'error'".to_string(),
                constituents: vec![
                    TypeInfo::primitive("'success'"),
                    TypeInfo::primitive("'error'"),
                ],
                ..Default::default()
            }),
            property_changes: vec![],
            severity: Severity::Major,
            impact: None,
            migration_hint: Some("Remove 'pending' cases from switch statements".to_string()),
        })));

        assert!(report.has_breaking_changes());
    }

    /// Scenario: Export removed from public API
    #[test]
    fn test_export_removed() {
        let mut report = ChangeReport::new("my-lib", "1.0.0", "2.0.0");

        report.add_change(BreakingChange::Code(CodeChange::ExportRemoved(
            ExportRemovedChange {
                name: "deprecatedHelper".to_string(),
                module_path: "src/index.ts".to_string(),
                export_type: Some("function".to_string()),
                replacement: Some("newHelper".to_string()),
                migration_hint: Some(
                    "Replace deprecatedHelper() with newHelper() - same signature"
                        .to_string(),
                ),
            },
        )));

        assert!(report.has_breaking_changes());
        assert_eq!(report.recommended_bump(), "major");

        // Check migration hint is present
        let change = &report.changes[0];
        assert!(change.migration_hint().is_some());
    }

    /// Scenario: Mixed breaking and non-breaking changes
    #[test]
    fn test_mixed_changes_report() {
        let mut report = ChangeReport::new("multi-lib", "1.0.0", "2.0.0");

        // Breaking: export removed
        report.add_change(BreakingChange::Code(CodeChange::ExportRemoved(
            ExportRemovedChange {
                name: "oldFn".to_string(),
                module_path: "src/index.ts".to_string(),
                export_type: Some("function".to_string()),
                replacement: None,
                migration_hint: None,
            },
        )));

        // Breaking: signature changed
        report.add_change(BreakingChange::Code(CodeChange::SignatureChanged(
            SignatureChange::new("process", "src/utils.ts", "old_sig", "new_sig"),
        )));

        // Non-breaking: new export added
        report.add_change(BreakingChange::Code(CodeChange::ExportAdded(
            interlint_format::changes::ExportAddedChange {
                name: "newFn".to_string(),
                module_path: "src/index.ts".to_string(),
                export_type: Some("function".to_string()),
            },
        )));

        // Breaking: asset format changed
        report.add_change(BreakingChange::Asset(AssetChange::FormatChanged(
            AssetFormatChange::new("config.json", "json", "yaml"),
        )));

        assert_eq!(report.summary.total_count, 4);
        assert_eq!(report.summary.major_count, 3);
        assert_eq!(report.summary.minor_count, 1);
        assert_eq!(report.summary.code_count, 3);
        assert_eq!(report.summary.asset_count, 1);
        assert!(report.has_breaking_changes());
        assert_eq!(report.recommended_bump(), "major");
    }
}

// =============================================================================
// EDGE CASE TESTS
// =============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn test_empty_exports_field() {
        let empty = ExportsField::Conditional(HashMap::new());

        assert!(empty.subpaths().is_empty());
        assert!(!empty.is_simple());
        assert!(empty.as_simple().is_none());
    }

    #[test]
    fn test_nested_conditional_exports() {
        // Deeply nested: { ".": { "node": { "import": "...", "require": "..." } } }
        let mut node_conds = HashMap::new();
        node_conds.insert("import".to_string(), ExportsValue::path("./node-esm.js"));
        node_conds.insert("require".to_string(), ExportsValue::path("./node-cjs.js"));

        let mut browser_conds = HashMap::new();
        browser_conds.insert("import".to_string(), ExportsValue::path("./browser.js"));
        browser_conds.insert("default".to_string(), ExportsValue::path("./browser.js"));

        let mut root_conds = HashMap::new();
        root_conds.insert("node".to_string(), ExportsValue::Conditional(node_conds));
        root_conds.insert("browser".to_string(), ExportsValue::Conditional(browser_conds));
        root_conds.insert("default".to_string(), ExportsValue::path("./index.js"));

        let mut map = HashMap::new();
        map.insert(".".to_string(), ExportsValue::Conditional(root_conds));

        let exports = ExportsField::Conditional(map);

        // Should be able to iterate subpaths
        let subpaths = exports.subpaths();
        assert!(subpaths.contains(&"."));
    }

    #[test]
    fn test_wildcard_exports() {
        // Pattern exports: { "./*": "./dist/*.js" }
        let mut map = HashMap::new();
        map.insert(".".to_string(), ExportsValue::path("./index.js"));
        map.insert("./*".to_string(), ExportsValue::path("./dist/*.js"));
        map.insert("./internal/*".to_string(), ExportsValue::Null);

        let exports = ExportsField::Conditional(map);

        let subpaths = exports.subpaths();
        assert!(subpaths.contains(&"./*"));
        assert!(subpaths.contains(&"./internal/*"));
    }

    #[test]
    fn test_exports_with_all_null() {
        // Package that blocks all subpath access
        let mut map = HashMap::new();
        map.insert(".".to_string(), ExportsValue::path("./index.js"));
        map.insert("./*".to_string(), ExportsValue::Null);

        let exports = ExportsField::Conditional(map);

        // Has root but blocks everything else
        let subpaths = exports.subpaths();
        assert!(subpaths.contains(&"."));
        assert!(subpaths.contains(&"./*"));
    }

    #[test]
    fn test_simple_exports_field() {
        let simple = ExportsField::simple("./index.js");

        assert!(simple.is_simple());
        assert_eq!(simple.as_simple(), Some("./index.js"));
    }

    #[test]
    fn test_exports_null_variant() {
        // Test null export for blocking internal access
        let exports = ExportsValue::Null;

        assert!(exports.is_null());
        assert!(!exports.is_path());
    }

    #[test]
    fn test_change_report_with_no_changes() {
        let report = ChangeReport::new("stable-lib", "1.0.0", "1.0.1");

        assert!(!report.has_breaking_changes());
        assert_eq!(report.recommended_bump(), "none");
        assert_eq!(report.summary.total_count, 0);
    }

    #[test]
    fn test_change_report_minor_only() {
        let mut report = ChangeReport::new("feature-lib", "1.0.0", "1.1.0");

        // Only additions
        report.add_change(BreakingChange::Code(CodeChange::ExportAdded(
            interlint_format::changes::ExportAddedChange {
                name: "newFeature".to_string(),
                module_path: "src/index.ts".to_string(),
                export_type: Some("function".to_string()),
            },
        )));

        assert!(!report.has_breaking_changes());
        assert_eq!(report.recommended_bump(), "minor");
    }

    #[test]
    fn test_filtering_changes_by_severity() {
        let mut report = ChangeReport::new("mixed-lib", "1.0.0", "2.0.0");

        // Major change
        report.add_change(BreakingChange::Code(CodeChange::ExportRemoved(
            ExportRemovedChange {
                name: "removed".to_string(),
                module_path: "src/index.ts".to_string(),
                export_type: None,
                replacement: None,
                migration_hint: None,
            },
        )));

        // Minor change
        report.add_change(BreakingChange::Code(CodeChange::ExportAdded(
            interlint_format::changes::ExportAddedChange {
                name: "added".to_string(),
                module_path: "src/index.ts".to_string(),
                export_type: None,
            },
        )));

        let major_changes: Vec<_> = report.changes_by_severity(Severity::Major).collect();
        let minor_changes: Vec<_> = report.changes_by_severity(Severity::Minor).collect();

        assert_eq!(major_changes.len(), 1);
        assert_eq!(minor_changes.len(), 1);
    }

    #[test]
    fn test_filtering_changes_by_kind() {
        let mut report = ChangeReport::new("mixed-lib", "1.0.0", "2.0.0");

        // Code change
        report.add_change(BreakingChange::Code(CodeChange::ExportRemoved(
            ExportRemovedChange {
                name: "fn".to_string(),
                module_path: "src/index.ts".to_string(),
                export_type: None,
                replacement: None,
                migration_hint: None,
            },
        )));

        // Asset change
        report.add_change(BreakingChange::Asset(AssetChange::FormatChanged(
            AssetFormatChange::new("config.json", "json", "yaml"),
        )));

        let code_changes: Vec<_> = report.code_changes().collect();
        let asset_changes: Vec<_> = report.asset_changes().collect();

        assert_eq!(code_changes.len(), 1);
        assert_eq!(asset_changes.len(), 1);
    }
}

// =============================================================================
// SERIALIZATION ROUNDTRIP TESTS
// =============================================================================

mod serialization_tests {
    use super::*;

    #[test]
    fn test_package_config_full_roundtrip() {
        let config = PackageConfig {
            name: "full-package".to_string(),
            version: "2.0.0".to_string(),
            main: Some("./dist/cjs/index.js".to_string()),
            module: Some("./dist/esm/index.js".to_string()),
            module_type: Some("module".to_string()),
            types: Some("./dist/types/index.d.ts".to_string()),
            exports: Some(ExportsField::Conditional({
                let mut root = HashMap::new();
                root.insert("import".to_string(), ExportsValue::path("./dist/esm/index.js"));
                root.insert("require".to_string(), ExportsValue::path("./dist/cjs/index.js"));
                root.insert("types".to_string(), ExportsValue::path("./dist/types/index.d.ts"));
                let mut map = HashMap::new();
                map.insert(".".to_string(), ExportsValue::Conditional(root));
                map.insert("./utils".to_string(), ExportsValue::path("./dist/utils.js"));
                map
            })),
            bin: {
                let mut map = HashMap::new();
                map.insert("my-cli".to_string(), "./bin/cli.js".to_string());
                map
            },
            engines: {
                let mut map = HashMap::new();
                map.insert("node".to_string(), ">=18".to_string());
                map.insert("npm".to_string(), ">=9".to_string());
                map
            },
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let decoded: PackageConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, decoded);
    }

    #[test]
    fn test_change_report_full_roundtrip() {
        let mut report = ChangeReport::new("test-package", "1.0.0", "2.0.0");

        report.add_change(BreakingChange::Code(CodeChange::SignatureChanged(
            SignatureChange {
                name: "processData".to_string(),
                module_path: "src/utils.ts".to_string(),
                old_signature: "processData(data: string): void".to_string(),
                new_signature: "processData(data: Buffer): Promise<void>".to_string(),
                change_details: vec![
                    "Parameter type changed".to_string(),
                    "Return type became Promise".to_string(),
                ],
                parameter_changes: vec![ParameterChange {
                    name: "data".to_string(),
                    change_type: ParameterChangeType::TypeChanged,
                    old_type: Some(TypeInfo::primitive("string")),
                    new_type: Some(TypeInfo::reference("Buffer")),
                    old_position: Some(0),
                    new_position: Some(0),
                }],
                return_type_change: Some(ReturnTypeChange {
                    old_type: Some(TypeInfo::void()),
                    new_type: Some(TypeInfo {
                        kind: TypeKind::Async,
                        repr: "Promise<void>".to_string(),
                        type_arguments: vec![TypeInfo::void()],
                        ..Default::default()
                    }),
                    became_async: true,
                    was_async: false,
                }),
                migration_hint: Some("Add await to all call sites".to_string()),
            },
        )));

        report.add_change(BreakingChange::Asset(AssetChange::FormatChanged(
            AssetFormatChange::new("config.json", "json", "yaml")
                .with_hint("Convert JSON to YAML format"),
        )));

        let json = serde_json::to_string_pretty(&report).unwrap();
        let decoded: ChangeReport = serde_json::from_str(&json).unwrap();

        assert_eq!(report, decoded);
        assert_eq!(decoded.summary.total_count, 2);
        assert_eq!(decoded.summary.major_count, 2);
    }

    #[test]
    fn test_exports_value_variants_roundtrip() {
        // Test all ExportsValue variants
        let variants = vec![
            ExportsValue::path("./index.js"),
            ExportsValue::Null,
            ExportsValue::Conditional({
                let mut map = HashMap::new();
                map.insert("import".to_string(), ExportsValue::path("./esm.js"));
                map.insert("require".to_string(), ExportsValue::path("./cjs.js"));
                map
            }),
        ];

        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let decoded: ExportsValue = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, decoded);
        }
    }

    #[test]
    fn test_package_json_change_roundtrip() {
        let change = PackageJsonChange::new(
            PackageChangeSeverity::Breaking,
            PackageChangeCategory::EntryPoints,
            "Export subpath removed",
            "exports[\"./utils\"]",
        )
        .with_old_value("./utils.js")
        .with_suggestion("Use ./helpers instead");

        let json = serde_json::to_string(&change).unwrap();
        let decoded: PackageJsonChange = serde_json::from_str(&json).unwrap();

        assert_eq!(change, decoded);
        assert!(decoded.is_breaking());
    }
}

// =============================================================================
// CHANGE TRAIT IMPLEMENTATION TESTS
// =============================================================================

mod change_trait_tests {
    use super::*;

    #[test]
    fn test_breaking_change_trait_methods() {
        let change = BreakingChange::Code(CodeChange::ExportRemoved(ExportRemovedChange {
            name: "helper".to_string(),
            module_path: "src/utils.ts".to_string(),
            export_type: Some("function".to_string()),
            replacement: Some("newHelper".to_string()),
            migration_hint: Some("Use newHelper instead".to_string()),
        }));

        assert_eq!(change.severity(), Severity::Major);
        assert_eq!(change.kind(), ChangeKind::Code);
        assert!(change.description().contains("helper"));
        assert_eq!(change.location(), "src/utils.ts");
        assert_eq!(change.migration_hint(), Some("Use newHelper instead"));
        assert!(change.is_code());
        assert!(!change.is_asset());
    }

    #[test]
    fn test_asset_change_trait_methods() {
        let change = BreakingChange::Asset(AssetChange::FormatChanged(AssetFormatChange {
            path: "config/settings.json".to_string(),
            old_format: "json".to_string(),
            new_format: "yaml".to_string(),
            migration_hint: Some("Convert to YAML".to_string()),
        }));

        assert_eq!(change.severity(), Severity::Major);
        assert_eq!(change.kind(), ChangeKind::Asset);
        assert!(change.description().contains("config/settings.json"));
        assert_eq!(change.location(), "config/settings.json");
        assert_eq!(change.migration_hint(), Some("Convert to YAML"));
        assert!(change.is_asset());
        assert!(!change.is_code());
    }
}
