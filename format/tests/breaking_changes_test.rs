//! Integration tests for the breaking_changes module.
//!
//! These tests verify the functionality of breaking change types including:
//! - Severity levels and comparisons
//! - PackageBreakingChange variants
//! - LibraryBreakingChange variants
//! - Builder patterns
//! - JSON serialization with correct tags
//! - Edge cases and integration scenarios

use interlint_format::{
    ApiSignatureChangeBuilder, AssetFormatChangeBuilder, BehaviorChangeBuilder,
    BehaviorChangeData, BreakingChangeTrait, ChangeLocation, LegacySeverity as Severity,
    LibraryBreakingChange, PackageBreakingChange, TypeChangeBuilder, TypeChangeKind,
};

// ============================================================================
// Severity Tests
// ============================================================================

mod severity_tests {
    use super::*;

    #[test]
    fn test_severity_default_is_moderate() {
        let severity = Severity::default();
        assert_eq!(severity, Severity::Moderate);
    }

    #[test]
    fn test_severity_as_level_ordering() {
        assert_eq!(Severity::Minor.as_level(), 1);
        assert_eq!(Severity::Moderate.as_level(), 2);
        assert_eq!(Severity::Major.as_level(), 3);
        assert_eq!(Severity::Critical.as_level(), 4);
    }

    #[test]
    fn test_severity_is_at_least_same_level() {
        assert!(Severity::Minor.is_at_least(&Severity::Minor));
        assert!(Severity::Moderate.is_at_least(&Severity::Moderate));
        assert!(Severity::Major.is_at_least(&Severity::Major));
        assert!(Severity::Critical.is_at_least(&Severity::Critical));
    }

    #[test]
    fn test_severity_is_at_least_higher_level() {
        assert!(Severity::Critical.is_at_least(&Severity::Major));
        assert!(Severity::Critical.is_at_least(&Severity::Moderate));
        assert!(Severity::Critical.is_at_least(&Severity::Minor));
        assert!(Severity::Major.is_at_least(&Severity::Moderate));
        assert!(Severity::Major.is_at_least(&Severity::Minor));
        assert!(Severity::Moderate.is_at_least(&Severity::Minor));
    }

    #[test]
    fn test_severity_is_at_least_lower_level() {
        assert!(!Severity::Minor.is_at_least(&Severity::Moderate));
        assert!(!Severity::Minor.is_at_least(&Severity::Major));
        assert!(!Severity::Minor.is_at_least(&Severity::Critical));
        assert!(!Severity::Moderate.is_at_least(&Severity::Major));
        assert!(!Severity::Moderate.is_at_least(&Severity::Critical));
        assert!(!Severity::Major.is_at_least(&Severity::Critical));
    }

    #[test]
    fn test_severity_clone_and_copy() {
        let severity = Severity::Major;
        let cloned = severity.clone();
        let copied = severity;

        assert_eq!(severity, cloned);
        assert_eq!(severity, copied);
    }

    #[test]
    fn test_severity_hash_equality() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(Severity::Minor);
        set.insert(Severity::Minor); // Duplicate

        assert_eq!(set.len(), 1);
        assert!(set.contains(&Severity::Minor));
    }

    #[test]
    fn test_severity_serialization_roundtrip() {
        for severity in [
            Severity::Minor,
            Severity::Moderate,
            Severity::Major,
            Severity::Critical,
        ] {
            let json = serde_json::to_string(&severity).expect("serialization should succeed");
            let deserialized: Severity =
                serde_json::from_str(&json).expect("deserialization should succeed");
            assert_eq!(severity, deserialized);
        }
    }

    #[test]
    fn test_severity_json_snake_case() {
        assert_eq!(
            serde_json::to_string(&Severity::Minor).unwrap(),
            "\"minor\""
        );
        assert_eq!(
            serde_json::to_string(&Severity::Moderate).unwrap(),
            "\"moderate\""
        );
        assert_eq!(
            serde_json::to_string(&Severity::Major).unwrap(),
            "\"major\""
        );
        assert_eq!(
            serde_json::to_string(&Severity::Critical).unwrap(),
            "\"critical\""
        );
    }
}

// ============================================================================
// ChangeLocation Tests
// ============================================================================

mod change_location_tests {
    use super::*;

    #[test]
    fn test_change_location_new_path_only() {
        let loc = ChangeLocation::new("src/lib.rs");

        assert_eq!(loc.path, "src/lib.rs");
        assert!(loc.line.is_none());
        assert!(loc.column.is_none());
    }

    #[test]
    fn test_change_location_with_line() {
        let loc = ChangeLocation::with_line("src/main.rs", 42);

        assert_eq!(loc.path, "src/main.rs");
        assert_eq!(loc.line, Some(42));
        assert!(loc.column.is_none());
    }

    #[test]
    fn test_change_location_with_position() {
        let loc = ChangeLocation::with_position("src/utils.rs", 100, 25);

        assert_eq!(loc.path, "src/utils.rs");
        assert_eq!(loc.line, Some(100));
        assert_eq!(loc.column, Some(25));
    }

    #[test]
    fn test_change_location_into_string() {
        // Test that Into<String> works for path parameter
        let path = String::from("src/module.rs");
        let loc = ChangeLocation::new(path);

        assert_eq!(loc.path, "src/module.rs");
    }

    #[test]
    fn test_change_location_serialization_omits_none() {
        let loc = ChangeLocation::new("src/lib.rs");
        let json = serde_json::to_string(&loc).unwrap();

        // Should not contain "line" or "column" when None
        assert!(!json.contains("line"));
        assert!(!json.contains("column"));
        assert!(json.contains("path"));
    }

    #[test]
    fn test_change_location_serialization_includes_values() {
        let loc = ChangeLocation::with_position("src/lib.rs", 10, 5);
        let json = serde_json::to_string(&loc).unwrap();

        assert!(json.contains("\"line\":10"));
        assert!(json.contains("\"column\":5"));
    }

    #[test]
    fn test_change_location_roundtrip() {
        let locations = vec![
            ChangeLocation::new("path/to/file.rs"),
            ChangeLocation::with_line("another/file.ts", 1),
            ChangeLocation::with_position("deep/nested/file.py", 999, 100),
        ];

        for loc in locations {
            let json = serde_json::to_string(&loc).unwrap();
            let deserialized: ChangeLocation = serde_json::from_str(&json).unwrap();
            assert_eq!(loc, deserialized);
        }
    }

    #[test]
    fn test_change_location_unicode_path() {
        let loc = ChangeLocation::new("src/\u{4e2d}\u{6587}/module.rs"); // Chinese characters

        assert_eq!(loc.path, "src/\u{4e2d}\u{6587}/module.rs");

        let json = serde_json::to_string(&loc).unwrap();
        let deserialized: ChangeLocation = serde_json::from_str(&json).unwrap();
        assert_eq!(loc, deserialized);
    }

    #[test]
    fn test_change_location_special_characters_in_path() {
        let paths = vec![
            "src/file with spaces.rs",
            "src/file-with-dashes.rs",
            "src/file_with_underscores.rs",
            "src/file.multiple.dots.rs",
            "src/@scoped/package.ts",
            "src/[bracket]/file.rs",
        ];

        for path in paths {
            let loc = ChangeLocation::new(path);
            let json = serde_json::to_string(&loc).unwrap();
            let deserialized: ChangeLocation = serde_json::from_str(&json).unwrap();
            assert_eq!(loc.path, deserialized.path);
        }
    }
}

// ============================================================================
// PackageBreakingChange Tests
// ============================================================================

mod package_breaking_change_tests {
    use super::*;

    #[test]
    fn test_asset_format_change_basic() {
        let change = PackageBreakingChange::AssetFormatChange {
            asset_path: "styles/main.css".to_string(),
            old_format: Some("css3".to_string()),
            new_format: "css4".to_string(),
            description: "Upgraded CSS format".to_string(),
            severity: Severity::Major,
        };

        assert_eq!(change.severity(), &Severity::Major);
        assert_eq!(change.description(), "Upgraded CSS format");
        assert_eq!(change.category(), "asset_format");
        assert!(change.is_severe());
        assert_eq!(change.asset_path(), "styles/main.css");
    }

    #[test]
    fn test_asset_format_change_without_old_format() {
        let change = PackageBreakingChange::AssetFormatChange {
            asset_path: "config/schema.json".to_string(),
            old_format: None,
            new_format: "json-schema-draft-07".to_string(),
            description: "New schema format".to_string(),
            severity: Severity::Minor,
        };

        assert_eq!(change.severity(), &Severity::Minor);
        assert!(!change.is_severe());
    }

    #[test]
    fn test_asset_dependency_change_with_all_types() {
        let change = PackageBreakingChange::AssetDependencyChange {
            asset_path: "styles/main.css".to_string(),
            removed_dependencies: vec!["reset.css".to_string(), "normalize.css".to_string()],
            added_dependencies: vec!["modern-reset.css".to_string()],
            modified_dependencies: vec!["variables.css".to_string()],
            description: "Refactored CSS dependencies".to_string(),
            severity: Severity::Moderate,
        };

        assert_eq!(change.severity(), &Severity::Moderate);
        assert_eq!(change.category(), "asset_dependency");
        assert_eq!(change.asset_path(), "styles/main.css");
    }

    #[test]
    fn test_asset_dependency_change_empty_lists() {
        let change = PackageBreakingChange::AssetDependencyChange {
            asset_path: "index.html".to_string(),
            removed_dependencies: vec![],
            added_dependencies: vec![],
            modified_dependencies: vec![],
            description: "No dependency changes".to_string(),
            severity: Severity::Minor,
        };

        let json = serde_json::to_string(&change).unwrap();
        // Empty vecs should be omitted due to skip_serializing_if
        assert!(!json.contains("removed_dependencies"));
        assert!(!json.contains("added_dependencies"));
        assert!(!json.contains("modified_dependencies"));
    }

    #[test]
    fn test_asset_path_change_move() {
        let change = PackageBreakingChange::AssetPathChange {
            old_path: "assets/logo.png".to_string(),
            new_path: Some("images/logo.png".to_string()),
            is_removal: false,
            is_rename: false,
            description: "Moved logo to images directory".to_string(),
            severity: Severity::Minor,
        };

        assert_eq!(change.category(), "asset_path");
        assert_eq!(change.asset_path(), "assets/logo.png");
    }

    #[test]
    fn test_asset_path_change_rename() {
        let change = PackageBreakingChange::AssetPathChange {
            old_path: "config/settings.json".to_string(),
            new_path: Some("config/config.json".to_string()),
            is_removal: false,
            is_rename: true,
            description: "Renamed settings to config".to_string(),
            severity: Severity::Minor,
        };

        assert!(!change.is_severe());
    }

    #[test]
    fn test_asset_path_change_removal() {
        let change = PackageBreakingChange::AssetPathChange {
            old_path: "deprecated/old-file.txt".to_string(),
            new_path: None,
            is_removal: true,
            is_rename: false,
            description: "Removed deprecated file".to_string(),
            severity: Severity::Major,
        };

        assert!(change.is_severe());
    }

    #[test]
    fn test_asset_metadata_change() {
        let change = PackageBreakingChange::AssetMetadataChange {
            asset_path: "package.json".to_string(),
            field_name: "main".to_string(),
            old_value: Some("dist/index.js".to_string()),
            new_value: Some("dist/index.mjs".to_string()),
            description: "Changed main entry point to ESM".to_string(),
            severity: Severity::Major,
        };

        assert_eq!(change.category(), "asset_metadata");
        assert_eq!(change.asset_path(), "package.json");
    }

    #[test]
    fn test_asset_metadata_change_field_added() {
        let change = PackageBreakingChange::AssetMetadataChange {
            asset_path: "tsconfig.json".to_string(),
            field_name: "strict".to_string(),
            old_value: None,
            new_value: Some("true".to_string()),
            description: "Added strict mode".to_string(),
            severity: Severity::Moderate,
        };

        let json = serde_json::to_string(&change).unwrap();
        assert!(!json.contains("old_value"));
        assert!(json.contains("new_value"));
    }

    #[test]
    fn test_package_change_json_tag_format() {
        let change = PackageBreakingChange::AssetFormatChange {
            asset_path: "test.css".to_string(),
            old_format: None,
            new_format: "css4".to_string(),
            description: "Test".to_string(),
            severity: Severity::Minor,
        };

        let json = serde_json::to_string(&change).unwrap();
        assert!(json.contains("\"kind\":\"asset_format_change\""));
    }

    #[test]
    fn test_package_change_all_variants_json_tags() {
        let variants: Vec<PackageBreakingChange> = vec![
            PackageBreakingChange::AssetFormatChange {
                asset_path: "a".to_string(),
                old_format: None,
                new_format: "new".to_string(),
                description: "d".to_string(),
                severity: Severity::Minor,
            },
            PackageBreakingChange::AssetDependencyChange {
                asset_path: "a".to_string(),
                removed_dependencies: vec![],
                added_dependencies: vec![],
                modified_dependencies: vec![],
                description: "d".to_string(),
                severity: Severity::Minor,
            },
            PackageBreakingChange::AssetPathChange {
                old_path: "a".to_string(),
                new_path: None,
                is_removal: false,
                is_rename: false,
                description: "d".to_string(),
                severity: Severity::Minor,
            },
            PackageBreakingChange::AssetMetadataChange {
                asset_path: "a".to_string(),
                field_name: "f".to_string(),
                old_value: None,
                new_value: None,
                description: "d".to_string(),
                severity: Severity::Minor,
            },
        ];

        let expected_tags = vec![
            "asset_format_change",
            "asset_dependency_change",
            "asset_path_change",
            "asset_metadata_change",
        ];

        for (change, expected_tag) in variants.iter().zip(expected_tags.iter()) {
            let json = serde_json::to_string(&change).unwrap();
            assert!(
                json.contains(&format!("\"kind\":\"{}\"", expected_tag)),
                "Expected tag {} not found in {}",
                expected_tag,
                json
            );
        }
    }

    #[test]
    fn test_package_change_serialization_roundtrip_all_variants() {
        let variants = vec![
            PackageBreakingChange::AssetFormatChange {
                asset_path: "styles/app.css".to_string(),
                old_format: Some("css3".to_string()),
                new_format: "css4".to_string(),
                description: "Upgraded CSS".to_string(),
                severity: Severity::Major,
            },
            PackageBreakingChange::AssetDependencyChange {
                asset_path: "main.html".to_string(),
                removed_dependencies: vec!["old.js".to_string()],
                added_dependencies: vec!["new.js".to_string()],
                modified_dependencies: vec!["shared.js".to_string()],
                description: "Updated deps".to_string(),
                severity: Severity::Moderate,
            },
            PackageBreakingChange::AssetPathChange {
                old_path: "assets/icon.svg".to_string(),
                new_path: Some("icons/main.svg".to_string()),
                is_removal: false,
                is_rename: true,
                description: "Moved icon".to_string(),
                severity: Severity::Minor,
            },
            PackageBreakingChange::AssetMetadataChange {
                asset_path: "manifest.json".to_string(),
                field_name: "version".to_string(),
                old_value: Some("1.0.0".to_string()),
                new_value: Some("2.0.0".to_string()),
                description: "Version bump".to_string(),
                severity: Severity::Critical,
            },
        ];

        for change in variants {
            let json = serde_json::to_string(&change).unwrap();
            let deserialized: PackageBreakingChange = serde_json::from_str(&json).unwrap();
            assert_eq!(change, deserialized);
        }
    }
}

// ============================================================================
// LibraryBreakingChange Tests
// ============================================================================

mod library_breaking_change_tests {
    use super::*;

    #[test]
    fn test_api_signature_change_basic() {
        let change = LibraryBreakingChange::ApiSignatureChange {
            name: "process_data".to_string(),
            location: ChangeLocation::with_line("src/processor.rs", 42),
            old_signature: Some("fn process_data(input: &str) -> Result<Data>".to_string()),
            new_signature: "fn process_data(input: &str, opts: Options) -> Result<Data>"
                .to_string(),
            description: "Added required options parameter".to_string(),
            severity: Severity::Major,
        };

        assert_eq!(change.severity(), &Severity::Major);
        assert_eq!(change.description(), "Added required options parameter");
        assert_eq!(change.category(), "api_signature");
        assert!(change.is_severe());
        assert_eq!(change.name(), "process_data");

        let loc = change.location().unwrap();
        assert_eq!(loc.path, "src/processor.rs");
        assert_eq!(loc.line, Some(42));
    }

    #[test]
    fn test_api_signature_change_without_old_signature() {
        let change = LibraryBreakingChange::ApiSignatureChange {
            name: "new_function".to_string(),
            location: ChangeLocation::new("src/lib.rs"),
            old_signature: None,
            new_signature: "fn new_function() -> ()".to_string(),
            description: "New function added".to_string(),
            severity: Severity::Minor,
        };

        assert!(!change.is_severe());
    }

    #[test]
    fn test_type_change_all_kinds() {
        let kinds = vec![
            TypeChangeKind::Removed,
            TypeChangeKind::Renamed,
            TypeChangeKind::StructureChanged,
            TypeChangeKind::GenericsChanged,
            TypeChangeKind::ConstraintsChanged,
            TypeChangeKind::InheritanceChanged,
        ];

        for kind in kinds {
            let change = LibraryBreakingChange::TypeChange {
                name: "MyType".to_string(),
                location: ChangeLocation::new("src/types.rs"),
                change_kind: kind,
                old_definition: Some("struct MyType {}".to_string()),
                new_definition: Some("struct MyType { field: u32 }".to_string()),
                description: format!("Type change: {:?}", kind),
                severity: Severity::Moderate,
            };

            assert_eq!(change.category(), "type_change");
            assert_eq!(change.name(), "MyType");
        }
    }

    #[test]
    fn test_type_change_kind_serialization() {
        let kinds_and_json = vec![
            (TypeChangeKind::Removed, "\"removed\""),
            (TypeChangeKind::Renamed, "\"renamed\""),
            (TypeChangeKind::StructureChanged, "\"structure_changed\""),
            (TypeChangeKind::GenericsChanged, "\"generics_changed\""),
            (
                TypeChangeKind::ConstraintsChanged,
                "\"constraints_changed\"",
            ),
            (
                TypeChangeKind::InheritanceChanged,
                "\"inheritance_changed\"",
            ),
        ];

        for (kind, expected_json) in kinds_and_json {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, expected_json);

            let deserialized: TypeChangeKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, deserialized);
        }
    }

    #[test]
    fn test_behavior_change_all_flags() {
        let change = LibraryBreakingChange::BehaviorChange(Box::new(BehaviorChangeData {
            name: "fetch_data".to_string(),
            location: ChangeLocation::with_position("src/api.rs", 100, 5),
            old_behavior: Some("Returns empty array on error".to_string()),
            new_behavior: "Throws exception on error".to_string(),
            affects_error_handling: true,
            affects_return_values: true,
            affects_side_effects: true,
            description: "Changed error handling strategy".to_string(),
            severity: Severity::Critical,
        }));

        assert_eq!(change.severity(), &Severity::Critical);
        assert_eq!(change.category(), "behavior");
        assert!(change.is_severe());
    }

    #[test]
    fn test_behavior_change_no_flags() {
        let change = LibraryBreakingChange::BehaviorChange(Box::new(BehaviorChangeData {
            name: "compute".to_string(),
            location: ChangeLocation::new("src/math.rs"),
            old_behavior: None,
            new_behavior: "Uses optimized algorithm".to_string(),
            affects_error_handling: false,
            affects_return_values: false,
            affects_side_effects: false,
            description: "Internal optimization".to_string(),
            severity: Severity::Minor,
        }));

        assert!(!change.is_severe());
    }

    #[test]
    fn test_dependency_version_change_major_bump() {
        let change = LibraryBreakingChange::DependencyVersionChange {
            dependency_name: "serde".to_string(),
            old_version: Some("1.0.0".to_string()),
            new_version: "2.0.0".to_string(),
            is_major_bump: true,
            description: "Upgraded serde to v2".to_string(),
            severity: Severity::Major,
        };

        assert_eq!(change.category(), "dependency_version");
        assert_eq!(change.name(), "serde");
        assert!(change.location().is_none()); // No location for dependency changes
        assert!(change.is_severe());
    }

    #[test]
    fn test_dependency_version_change_minor_bump() {
        let change = LibraryBreakingChange::DependencyVersionChange {
            dependency_name: "tokio".to_string(),
            old_version: Some("1.0.0".to_string()),
            new_version: "1.1.0".to_string(),
            is_major_bump: false,
            description: "Upgraded tokio".to_string(),
            severity: Severity::Minor,
        };

        assert!(!change.is_severe());
    }

    #[test]
    fn test_library_change_json_tag_format() {
        let change = LibraryBreakingChange::ApiSignatureChange {
            name: "test".to_string(),
            location: ChangeLocation::new("test.rs"),
            old_signature: None,
            new_signature: "fn test()".to_string(),
            description: "Test".to_string(),
            severity: Severity::Minor,
        };

        let json = serde_json::to_string(&change).unwrap();
        assert!(json.contains("\"kind\":\"api_signature_change\""));
    }

    #[test]
    fn test_library_change_all_variants_json_tags() {
        let variants: Vec<(LibraryBreakingChange, &str)> = vec![
            (
                LibraryBreakingChange::ApiSignatureChange {
                    name: "f".to_string(),
                    location: ChangeLocation::new("a.rs"),
                    old_signature: None,
                    new_signature: "fn f()".to_string(),
                    description: "d".to_string(),
                    severity: Severity::Minor,
                },
                "api_signature_change",
            ),
            (
                LibraryBreakingChange::TypeChange {
                    name: "T".to_string(),
                    location: ChangeLocation::new("a.rs"),
                    change_kind: TypeChangeKind::Removed,
                    old_definition: None,
                    new_definition: None,
                    description: "d".to_string(),
                    severity: Severity::Minor,
                },
                "type_change",
            ),
            (
                LibraryBreakingChange::BehaviorChange(Box::new(BehaviorChangeData {
                    name: "b".to_string(),
                    location: ChangeLocation::new("a.rs"),
                    old_behavior: None,
                    new_behavior: "new".to_string(),
                    affects_error_handling: false,
                    affects_return_values: false,
                    affects_side_effects: false,
                    description: "d".to_string(),
                    severity: Severity::Minor,
                })),
                "behavior_change",
            ),
            (
                LibraryBreakingChange::DependencyVersionChange {
                    dependency_name: "dep".to_string(),
                    old_version: None,
                    new_version: "1.0.0".to_string(),
                    is_major_bump: false,
                    description: "d".to_string(),
                    severity: Severity::Minor,
                },
                "dependency_version_change",
            ),
        ];

        for (change, expected_tag) in variants {
            let json = serde_json::to_string(&change).unwrap();
            assert!(
                json.contains(&format!("\"kind\":\"{}\"", expected_tag)),
                "Expected tag {} not found in {}",
                expected_tag,
                json
            );
        }
    }

    #[test]
    fn test_library_change_serialization_roundtrip_all_variants() {
        let variants = vec![
            LibraryBreakingChange::ApiSignatureChange {
                name: "process".to_string(),
                location: ChangeLocation::with_position("src/lib.rs", 10, 5),
                old_signature: Some("fn process() -> ()".to_string()),
                new_signature: "fn process(data: &str) -> Result<()>".to_string(),
                description: "Added parameter and changed return type".to_string(),
                severity: Severity::Major,
            },
            LibraryBreakingChange::TypeChange {
                name: "Config".to_string(),
                location: ChangeLocation::new("src/config.rs"),
                change_kind: TypeChangeKind::StructureChanged,
                old_definition: Some("struct Config { name: String }".to_string()),
                new_definition: Some("struct Config { name: String, version: u32 }".to_string()),
                description: "Added version field".to_string(),
                severity: Severity::Moderate,
            },
            LibraryBreakingChange::BehaviorChange(Box::new(BehaviorChangeData {
                name: "validate".to_string(),
                location: ChangeLocation::with_line("src/validation.rs", 55),
                old_behavior: Some("Silently ignores invalid input".to_string()),
                new_behavior: "Throws ValidationError for invalid input".to_string(),
                affects_error_handling: true,
                affects_return_values: false,
                affects_side_effects: false,
                description: "Now validates input strictly".to_string(),
                severity: Severity::Critical,
            })),
            LibraryBreakingChange::DependencyVersionChange {
                dependency_name: "regex".to_string(),
                old_version: Some("1.5.0".to_string()),
                new_version: "2.0.0".to_string(),
                is_major_bump: true,
                description: "Upgraded regex crate".to_string(),
                severity: Severity::Major,
            },
        ];

        for change in variants {
            let json = serde_json::to_string(&change).unwrap();
            let deserialized: LibraryBreakingChange = serde_json::from_str(&json).unwrap();
            assert_eq!(change, deserialized);
        }
    }
}

// ============================================================================
// Builder Tests
// ============================================================================

mod builder_tests {
    use super::*;

    #[test]
    fn test_asset_format_change_builder_all_fields() {
        let change = AssetFormatChangeBuilder::new()
            .asset_path("styles/app.css")
            .old_format("css3")
            .new_format("css4")
            .description("Upgraded to CSS4 syntax")
            .severity(Severity::Major)
            .build();

        if let PackageBreakingChange::AssetFormatChange {
            asset_path,
            old_format,
            new_format,
            description,
            severity,
        } = change
        {
            assert_eq!(asset_path, "styles/app.css");
            assert_eq!(old_format, Some("css3".to_string()));
            assert_eq!(new_format, "css4");
            assert_eq!(description, "Upgraded to CSS4 syntax");
            assert_eq!(severity, Severity::Major);
        } else {
            panic!("Expected AssetFormatChange variant");
        }
    }

    #[test]
    fn test_asset_format_change_builder_minimal() {
        let change = AssetFormatChangeBuilder::new()
            .asset_path("data.json")
            .new_format("json5")
            .description("Changed to JSON5 format")
            .build();

        if let PackageBreakingChange::AssetFormatChange {
            old_format,
            severity,
            ..
        } = change
        {
            assert!(old_format.is_none());
            assert_eq!(severity, Severity::Moderate); // Default
        } else {
            panic!("Expected AssetFormatChange variant");
        }
    }

    #[test]
    fn test_asset_format_change_builder_default() {
        let builder = AssetFormatChangeBuilder::default();
        // Verify it's the same as new()
        let builder2 = AssetFormatChangeBuilder::new();

        // Both should have default severity
        let change1 = builder
            .asset_path("a")
            .new_format("b")
            .description("c")
            .build();

        let change2 = builder2
            .asset_path("a")
            .new_format("b")
            .description("c")
            .build();

        assert_eq!(change1, change2);
    }

    #[test]
    #[should_panic(expected = "asset_path is required")]
    fn test_asset_format_change_builder_missing_path() {
        AssetFormatChangeBuilder::new()
            .new_format("format")
            .description("desc")
            .build();
    }

    #[test]
    #[should_panic(expected = "new_format is required")]
    fn test_asset_format_change_builder_missing_format() {
        AssetFormatChangeBuilder::new()
            .asset_path("path")
            .description("desc")
            .build();
    }

    #[test]
    #[should_panic(expected = "description is required")]
    fn test_asset_format_change_builder_missing_description() {
        AssetFormatChangeBuilder::new()
            .asset_path("path")
            .new_format("format")
            .build();
    }

    #[test]
    fn test_api_signature_change_builder_all_fields() {
        let change = ApiSignatureChangeBuilder::new()
            .name("greet")
            .location(ChangeLocation::with_line("src/lib.rs", 42))
            .old_signature("fn greet() -> String")
            .new_signature("fn greet(name: &str) -> String")
            .description("Added name parameter")
            .severity(Severity::Major)
            .build();

        if let LibraryBreakingChange::ApiSignatureChange {
            name,
            location,
            old_signature,
            new_signature,
            description,
            severity,
        } = change
        {
            assert_eq!(name, "greet");
            assert_eq!(location.path, "src/lib.rs");
            assert_eq!(location.line, Some(42));
            assert_eq!(old_signature, Some("fn greet() -> String".to_string()));
            assert_eq!(new_signature, "fn greet(name: &str) -> String");
            assert_eq!(description, "Added name parameter");
            assert_eq!(severity, Severity::Major);
        } else {
            panic!("Expected ApiSignatureChange variant");
        }
    }

    #[test]
    fn test_api_signature_change_builder_minimal() {
        let change = ApiSignatureChangeBuilder::new()
            .name("init")
            .location(ChangeLocation::new("src/main.rs"))
            .new_signature("fn init() -> ()")
            .description("Initialization function")
            .build();

        if let LibraryBreakingChange::ApiSignatureChange {
            old_signature,
            severity,
            ..
        } = change
        {
            assert!(old_signature.is_none());
            assert_eq!(severity, Severity::Moderate); // Default
        } else {
            panic!("Expected ApiSignatureChange variant");
        }
    }

    #[test]
    #[should_panic(expected = "name is required")]
    fn test_api_signature_change_builder_missing_name() {
        ApiSignatureChangeBuilder::new()
            .location(ChangeLocation::new("a.rs"))
            .new_signature("fn f()")
            .description("desc")
            .build();
    }

    #[test]
    #[should_panic(expected = "location is required")]
    fn test_api_signature_change_builder_missing_location() {
        ApiSignatureChangeBuilder::new()
            .name("func")
            .new_signature("fn f()")
            .description("desc")
            .build();
    }

    #[test]
    fn test_type_change_builder_all_fields() {
        let change = TypeChangeBuilder::new()
            .name("Config")
            .location(ChangeLocation::with_position("src/config.rs", 10, 1))
            .change_kind(TypeChangeKind::StructureChanged)
            .old_definition("struct Config { name: String }")
            .new_definition("struct Config { name: String, version: u32 }")
            .description("Added version field")
            .severity(Severity::Moderate)
            .build();

        if let LibraryBreakingChange::TypeChange {
            name,
            location,
            change_kind,
            old_definition,
            new_definition,
            description,
            severity,
        } = change
        {
            assert_eq!(name, "Config");
            assert_eq!(location.path, "src/config.rs");
            assert_eq!(location.line, Some(10));
            assert_eq!(location.column, Some(1));
            assert_eq!(change_kind, TypeChangeKind::StructureChanged);
            assert_eq!(
                old_definition,
                Some("struct Config { name: String }".to_string())
            );
            assert_eq!(
                new_definition,
                Some("struct Config { name: String, version: u32 }".to_string())
            );
            assert_eq!(description, "Added version field");
            assert_eq!(severity, Severity::Moderate);
        } else {
            panic!("Expected TypeChange variant");
        }
    }

    #[test]
    fn test_type_change_builder_minimal() {
        let change = TypeChangeBuilder::new()
            .name("MyType")
            .location(ChangeLocation::new("src/types.rs"))
            .change_kind(TypeChangeKind::Removed)
            .description("Type was removed")
            .build();

        if let LibraryBreakingChange::TypeChange {
            old_definition,
            new_definition,
            ..
        } = change
        {
            assert!(old_definition.is_none());
            assert!(new_definition.is_none());
        } else {
            panic!("Expected TypeChange variant");
        }
    }

    #[test]
    #[should_panic(expected = "change_kind is required")]
    fn test_type_change_builder_missing_kind() {
        TypeChangeBuilder::new()
            .name("T")
            .location(ChangeLocation::new("a.rs"))
            .description("desc")
            .build();
    }

    #[test]
    fn test_behavior_change_builder_all_fields() {
        let change = BehaviorChangeBuilder::new()
            .name("fetch")
            .location(ChangeLocation::new("src/api.rs"))
            .old_behavior("Returns null on error")
            .new_behavior("Throws exception on error")
            .affects_error_handling(true)
            .affects_return_values(true)
            .affects_side_effects(true)
            .description("Changed error handling")
            .severity(Severity::Critical)
            .build();

        if let LibraryBreakingChange::BehaviorChange(data) = change {
            assert_eq!(data.name, "fetch");
            assert!(data.affects_error_handling);
            assert!(data.affects_return_values);
            assert!(data.affects_side_effects);
            assert_eq!(data.severity, Severity::Critical);
        } else {
            panic!("Expected BehaviorChange variant");
        }
    }

    #[test]
    fn test_behavior_change_builder_minimal() {
        let change = BehaviorChangeBuilder::new()
            .name("compute")
            .location(ChangeLocation::new("src/math.rs"))
            .new_behavior("Uses optimized algorithm")
            .description("Performance improvement")
            .build();

        if let LibraryBreakingChange::BehaviorChange(data) = change {
            assert!(data.old_behavior.is_none());
            assert!(!data.affects_error_handling);
            assert!(!data.affects_return_values);
            assert!(!data.affects_side_effects);
        } else {
            panic!("Expected BehaviorChange variant");
        }
    }

    #[test]
    #[should_panic(expected = "new_behavior is required")]
    fn test_behavior_change_builder_missing_behavior() {
        BehaviorChangeBuilder::new()
            .name("func")
            .location(ChangeLocation::new("a.rs"))
            .description("desc")
            .build();
    }

    #[test]
    fn test_builder_clone() {
        let builder = ApiSignatureChangeBuilder::new()
            .name("test")
            .location(ChangeLocation::new("test.rs"))
            .severity(Severity::Major);

        let builder_clone = builder.clone();

        let change1 = builder
            .new_signature("fn test() -> u32")
            .description("First")
            .build();

        let change2 = builder_clone
            .new_signature("fn test() -> i32")
            .description("Second")
            .build();

        // Both should have the same name and severity, but different signatures
        assert_eq!(change1.name(), change2.name());
        assert_eq!(change1.severity(), change2.severity());
        assert_ne!(change1.description(), change2.description());
    }
}

// ============================================================================
// Edge Cases
// ============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn test_empty_strings() {
        let change = PackageBreakingChange::AssetFormatChange {
            asset_path: "".to_string(),
            old_format: Some("".to_string()),
            new_format: "".to_string(),
            description: "".to_string(),
            severity: Severity::Minor,
        };

        let json = serde_json::to_string(&change).unwrap();
        let deserialized: PackageBreakingChange = serde_json::from_str(&json).unwrap();
        assert_eq!(change, deserialized);
    }

    #[test]
    fn test_unicode_in_descriptions() {
        let change = LibraryBreakingChange::ApiSignatureChange {
            name: "\u{5916}\u{90e8}_function".to_string(), // Japanese characters
            location: ChangeLocation::new("src/\u{4e2d}\u{6587}.rs"), // Chinese characters
            old_signature: Some("fn \u{03b1}\u{03b2}\u{03b3}()".to_string()), // Greek letters
            new_signature: "fn \u{03b1}\u{03b2}\u{03b3}\u{03b4}()".to_string(),
            description: "Added \u{0394} parameter - \u{65e5}\u{672c}\u{8a9e} description"
                .to_string(),
            severity: Severity::Moderate,
        };

        let json = serde_json::to_string(&change).unwrap();
        let deserialized: LibraryBreakingChange = serde_json::from_str(&json).unwrap();
        assert_eq!(change, deserialized);
    }

    #[test]
    fn test_special_characters_in_paths() {
        let special_paths = vec![
            "src/file with spaces.rs",
            "src/file\twith\ttabs.rs",
            "src/file\"with\"quotes.rs",
            "src/file\\with\\backslashes.rs",
            "src/file/with/many/nested/directories/deep.rs",
            "@scope/package/index.ts",
            "[computed]/path.ts",
        ];

        for path in special_paths {
            let change = LibraryBreakingChange::ApiSignatureChange {
                name: "test".to_string(),
                location: ChangeLocation::new(path),
                old_signature: None,
                new_signature: "fn test()".to_string(),
                description: format!("Test with path: {}", path),
                severity: Severity::Minor,
            };

            let json = serde_json::to_string(&change).unwrap();
            let deserialized: LibraryBreakingChange = serde_json::from_str(&json).unwrap();
            assert_eq!(change, deserialized);
        }
    }

    #[test]
    fn test_very_long_strings() {
        let long_string = "a".repeat(10000);

        let change = PackageBreakingChange::AssetMetadataChange {
            asset_path: long_string.clone(),
            field_name: long_string.clone(),
            old_value: Some(long_string.clone()),
            new_value: Some(long_string.clone()),
            description: long_string,
            severity: Severity::Minor,
        };

        let json = serde_json::to_string(&change).unwrap();
        let deserialized: PackageBreakingChange = serde_json::from_str(&json).unwrap();
        assert_eq!(change, deserialized);
    }

    #[test]
    fn test_newlines_in_signatures() {
        let multiline_sig = r#"fn complex_function<T, U>(
    param1: T,
    param2: U,
    param3: Option<String>,
) -> Result<(), Error>
where
    T: Debug + Clone,
    U: Send + Sync"#;

        let change = LibraryBreakingChange::ApiSignatureChange {
            name: "complex_function".to_string(),
            location: ChangeLocation::new("src/lib.rs"),
            old_signature: None,
            new_signature: multiline_sig.to_string(),
            description: "Complex function with multiline signature".to_string(),
            severity: Severity::Moderate,
        };

        let json = serde_json::to_string(&change).unwrap();
        let deserialized: LibraryBreakingChange = serde_json::from_str(&json).unwrap();
        assert_eq!(change, deserialized);
    }

    #[test]
    fn test_json_special_characters() {
        let change = PackageBreakingChange::AssetFormatChange {
            asset_path: "path/with\"quotes\".json".to_string(),
            old_format: Some("format\nwith\nnewlines".to_string()),
            new_format: "format\twith\ttabs".to_string(),
            description: "Description with \\ backslash and \"quotes\"".to_string(),
            severity: Severity::Minor,
        };

        let json = serde_json::to_string(&change).unwrap();
        let deserialized: PackageBreakingChange = serde_json::from_str(&json).unwrap();
        assert_eq!(change, deserialized);
    }

    #[test]
    fn test_boundary_line_numbers() {
        // Test with boundary values for line and column
        let locations = vec![
            ChangeLocation::with_line("a.rs", 0),
            ChangeLocation::with_line("a.rs", 1),
            ChangeLocation::with_line("a.rs", u32::MAX),
            ChangeLocation::with_position("a.rs", 0, 0),
            ChangeLocation::with_position("a.rs", u32::MAX, u32::MAX),
        ];

        for loc in locations {
            let json = serde_json::to_string(&loc).unwrap();
            let deserialized: ChangeLocation = serde_json::from_str(&json).unwrap();
            assert_eq!(loc, deserialized);
        }
    }

    #[test]
    fn test_all_severity_levels_in_changes() {
        let severities = vec![
            Severity::Minor,
            Severity::Moderate,
            Severity::Major,
            Severity::Critical,
        ];

        for severity in severities {
            let change = PackageBreakingChange::AssetFormatChange {
                asset_path: "test.css".to_string(),
                old_format: None,
                new_format: "new".to_string(),
                description: format!("Test with {:?} severity", severity),
                severity,
            };

            assert_eq!(change.severity(), &severity);
            assert_eq!(change.is_severe(), severity.is_at_least(&Severity::Major));
        }
    }
}

// ============================================================================
// Integration Scenarios
// ============================================================================

mod integration_scenarios {
    use super::*;

    /// Simulates building a complete change report from multiple changes
    #[test]
    fn test_building_complete_change_report() {
        let package_changes: Vec<Box<dyn BreakingChangeTrait>> = vec![
            Box::new(PackageBreakingChange::AssetFormatChange {
                asset_path: "styles/main.css".to_string(),
                old_format: Some("css3".to_string()),
                new_format: "css4".to_string(),
                description: "Upgraded CSS format".to_string(),
                severity: Severity::Major,
            }),
            Box::new(PackageBreakingChange::AssetPathChange {
                old_path: "assets/logo.png".to_string(),
                new_path: Some("images/logo.png".to_string()),
                is_removal: false,
                is_rename: false,
                description: "Moved logo".to_string(),
                severity: Severity::Minor,
            }),
        ];

        let library_changes: Vec<Box<dyn BreakingChangeTrait>> = vec![
            Box::new(LibraryBreakingChange::ApiSignatureChange {
                name: "process".to_string(),
                location: ChangeLocation::new("src/lib.rs"),
                old_signature: Some("fn process()".to_string()),
                new_signature: "fn process(opts: Options)".to_string(),
                description: "Added options parameter".to_string(),
                severity: Severity::Major,
            }),
            Box::new(LibraryBreakingChange::TypeChange {
                name: "Config".to_string(),
                location: ChangeLocation::new("src/config.rs"),
                change_kind: TypeChangeKind::StructureChanged,
                old_definition: Some("struct Config {}".to_string()),
                new_definition: Some("struct Config { version: u32 }".to_string()),
                description: "Added version field".to_string(),
                severity: Severity::Moderate,
            }),
        ];

        // Count severe changes
        let severe_package_changes = package_changes.iter().filter(|c| c.is_severe()).count();
        let severe_library_changes = library_changes.iter().filter(|c| c.is_severe()).count();

        assert_eq!(severe_package_changes, 1);
        assert_eq!(severe_library_changes, 1);

        // All changes
        let all_changes: Vec<&dyn BreakingChangeTrait> = package_changes
            .iter()
            .map(|c| c.as_ref())
            .chain(library_changes.iter().map(|c| c.as_ref()))
            .collect();

        assert_eq!(all_changes.len(), 4);

        // Find most severe
        let max_severity = all_changes
            .iter()
            .map(|c| c.severity().as_level())
            .max()
            .unwrap();

        assert_eq!(max_severity, Severity::Major.as_level());
    }

    /// Test filtering changes by severity
    #[test]
    fn test_filtering_changes_by_severity() {
        let changes = vec![
            PackageBreakingChange::AssetFormatChange {
                asset_path: "a.css".to_string(),
                old_format: None,
                new_format: "new".to_string(),
                description: "Minor change".to_string(),
                severity: Severity::Minor,
            },
            PackageBreakingChange::AssetFormatChange {
                asset_path: "b.css".to_string(),
                old_format: None,
                new_format: "new".to_string(),
                description: "Moderate change".to_string(),
                severity: Severity::Moderate,
            },
            PackageBreakingChange::AssetFormatChange {
                asset_path: "c.css".to_string(),
                old_format: None,
                new_format: "new".to_string(),
                description: "Major change".to_string(),
                severity: Severity::Major,
            },
            PackageBreakingChange::AssetFormatChange {
                asset_path: "d.css".to_string(),
                old_format: None,
                new_format: "new".to_string(),
                description: "Critical change".to_string(),
                severity: Severity::Critical,
            },
        ];

        // Filter by minimum severity
        let major_or_higher: Vec<_> = changes
            .iter()
            .filter(|c| c.severity().is_at_least(&Severity::Major))
            .collect();

        assert_eq!(major_or_higher.len(), 2);

        let moderate_or_higher: Vec<_> = changes
            .iter()
            .filter(|c| c.severity().is_at_least(&Severity::Moderate))
            .collect();

        assert_eq!(moderate_or_higher.len(), 3);

        let all: Vec<_> = changes
            .iter()
            .filter(|c| c.severity().is_at_least(&Severity::Minor))
            .collect();

        assert_eq!(all.len(), 4);
    }

    /// Test comparing severity levels for version bump suggestion
    #[test]
    fn test_suggested_version_bump() {
        fn suggest_bump(changes: &[impl BreakingChangeTrait]) -> &'static str {
            let max_severity = changes
                .iter()
                .map(|c| c.severity().as_level())
                .max()
                .unwrap_or(0);

            match max_severity {
                4 => "major", // Critical
                3 => "major", // Major
                2 => "minor", // Moderate
                1 => "patch", // Minor
                _ => "none",
            }
        }

        let no_changes: Vec<PackageBreakingChange> = vec![];
        assert_eq!(suggest_bump(&no_changes), "none");

        let minor_changes = vec![PackageBreakingChange::AssetPathChange {
            old_path: "a.txt".to_string(),
            new_path: Some("b.txt".to_string()),
            is_removal: false,
            is_rename: true,
            description: "Renamed".to_string(),
            severity: Severity::Minor,
        }];
        assert_eq!(suggest_bump(&minor_changes), "patch");

        let moderate_changes = vec![PackageBreakingChange::AssetFormatChange {
            asset_path: "a.json".to_string(),
            old_format: None,
            new_format: "json5".to_string(),
            description: "Format change".to_string(),
            severity: Severity::Moderate,
        }];
        assert_eq!(suggest_bump(&moderate_changes), "minor");

        let major_changes = vec![PackageBreakingChange::AssetPathChange {
            old_path: "critical.config".to_string(),
            new_path: None,
            is_removal: true,
            is_rename: false,
            description: "Removed critical config".to_string(),
            severity: Severity::Major,
        }];
        assert_eq!(suggest_bump(&major_changes), "major");

        let critical_changes = vec![PackageBreakingChange::AssetMetadataChange {
            asset_path: "package.json".to_string(),
            field_name: "main".to_string(),
            old_value: Some("index.js".to_string()),
            new_value: None,
            description: "Removed entry point".to_string(),
            severity: Severity::Critical,
        }];
        assert_eq!(suggest_bump(&critical_changes), "major");
    }

    /// Test grouping changes by category
    #[test]
    fn test_grouping_by_category() {
        let changes: Vec<Box<dyn BreakingChangeTrait>> = vec![
            Box::new(PackageBreakingChange::AssetFormatChange {
                asset_path: "a.css".to_string(),
                old_format: None,
                new_format: "new".to_string(),
                description: "1".to_string(),
                severity: Severity::Minor,
            }),
            Box::new(PackageBreakingChange::AssetPathChange {
                old_path: "b.txt".to_string(),
                new_path: None,
                is_removal: true,
                is_rename: false,
                description: "2".to_string(),
                severity: Severity::Major,
            }),
            Box::new(LibraryBreakingChange::ApiSignatureChange {
                name: "f".to_string(),
                location: ChangeLocation::new("a.rs"),
                old_signature: None,
                new_signature: "fn f()".to_string(),
                description: "3".to_string(),
                severity: Severity::Moderate,
            }),
            Box::new(LibraryBreakingChange::ApiSignatureChange {
                name: "g".to_string(),
                location: ChangeLocation::new("b.rs"),
                old_signature: None,
                new_signature: "fn g()".to_string(),
                description: "4".to_string(),
                severity: Severity::Minor,
            }),
        ];

        use std::collections::HashMap;
        let mut by_category: HashMap<&'static str, Vec<&dyn BreakingChangeTrait>> = HashMap::new();

        for change in &changes {
            by_category
                .entry(change.category())
                .or_default()
                .push(change.as_ref());
        }

        assert_eq!(by_category.get("asset_format").map(|v| v.len()), Some(1));
        assert_eq!(by_category.get("asset_path").map(|v| v.len()), Some(1));
        assert_eq!(by_category.get("api_signature").map(|v| v.len()), Some(2));
    }

    /// Test creating changes using builders in a workflow
    #[test]
    fn test_builder_workflow() {
        // Simulate a change detection workflow
        let detected_api_changes = vec![
            (
                "greet",
                "src/lib.rs",
                10,
                "fn greet()",
                "fn greet(name: &str)",
            ),
            (
                "process",
                "src/processor.rs",
                25,
                "fn process()",
                "fn process(data: Data)",
            ),
        ];

        let changes: Vec<LibraryBreakingChange> = detected_api_changes
            .into_iter()
            .map(|(name, path, line, old_sig, new_sig)| {
                ApiSignatureChangeBuilder::new()
                    .name(name)
                    .location(ChangeLocation::with_line(path, line))
                    .old_signature(old_sig)
                    .new_signature(new_sig)
                    .description(format!("Signature of {} changed", name))
                    .severity(Severity::Major)
                    .build()
            })
            .collect();

        assert_eq!(changes.len(), 2);

        for change in &changes {
            assert!(change.is_severe());
            assert_eq!(change.category(), "api_signature");
        }
    }

    /// Test mixed package and library changes together
    #[test]
    fn test_mixed_changes_serialization() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct ChangeReport {
            package_changes: Vec<PackageBreakingChange>,
            library_changes: Vec<LibraryBreakingChange>,
        }

        let report = ChangeReport {
            package_changes: vec![
                PackageBreakingChange::AssetFormatChange {
                    asset_path: "styles/app.css".to_string(),
                    old_format: Some("css3".to_string()),
                    new_format: "css4".to_string(),
                    description: "CSS upgrade".to_string(),
                    severity: Severity::Major,
                },
                PackageBreakingChange::AssetPathChange {
                    old_path: "old/path.txt".to_string(),
                    new_path: Some("new/path.txt".to_string()),
                    is_removal: false,
                    is_rename: false,
                    description: "Moved file".to_string(),
                    severity: Severity::Minor,
                },
            ],
            library_changes: vec![
                LibraryBreakingChange::ApiSignatureChange {
                    name: "init".to_string(),
                    location: ChangeLocation::with_line("src/lib.rs", 1),
                    old_signature: None,
                    new_signature: "fn init()".to_string(),
                    description: "New init function".to_string(),
                    severity: Severity::Minor,
                },
                LibraryBreakingChange::BehaviorChange(Box::new(BehaviorChangeData {
                    name: "validate".to_string(),
                    location: ChangeLocation::new("src/validation.rs"),
                    old_behavior: Some("Silent failure".to_string()),
                    new_behavior: "Throws error".to_string(),
                    affects_error_handling: true,
                    affects_return_values: false,
                    affects_side_effects: false,
                    description: "Now validates strictly".to_string(),
                    severity: Severity::Critical,
                })),
            ],
        };

        let json = serde_json::to_string_pretty(&report).unwrap();
        let deserialized: ChangeReport = serde_json::from_str(&json).unwrap();

        assert_eq!(report, deserialized);
        assert_eq!(report.package_changes.len(), 2);
        assert_eq!(report.library_changes.len(), 2);
    }
}

// ============================================================================
// Trait Object Tests
// ============================================================================

mod trait_object_tests {
    use super::*;

    #[test]
    fn test_breaking_change_trait_with_package_change() {
        let change = PackageBreakingChange::AssetFormatChange {
            asset_path: "test.css".to_string(),
            old_format: None,
            new_format: "new".to_string(),
            description: "Test change".to_string(),
            severity: Severity::Major,
        };

        // Use as trait object
        let trait_obj: &dyn BreakingChangeTrait = &change;

        assert_eq!(trait_obj.severity(), &Severity::Major);
        assert_eq!(trait_obj.description(), "Test change");
        assert_eq!(trait_obj.category(), "asset_format");
        assert!(trait_obj.is_severe());
    }

    #[test]
    fn test_breaking_change_trait_with_library_change() {
        let change = LibraryBreakingChange::TypeChange {
            name: "MyType".to_string(),
            location: ChangeLocation::new("src/types.rs"),
            change_kind: TypeChangeKind::Removed,
            old_definition: Some("struct MyType {}".to_string()),
            new_definition: None,
            description: "Type removed".to_string(),
            severity: Severity::Critical,
        };

        let trait_obj: &dyn BreakingChangeTrait = &change;

        assert_eq!(trait_obj.severity(), &Severity::Critical);
        assert_eq!(trait_obj.description(), "Type removed");
        assert_eq!(trait_obj.category(), "type_change");
        assert!(trait_obj.is_severe());
    }

    #[test]
    fn test_heterogeneous_collection() {
        let changes: Vec<Box<dyn BreakingChangeTrait>> = vec![
            Box::new(PackageBreakingChange::AssetFormatChange {
                asset_path: "a.css".to_string(),
                old_format: None,
                new_format: "new".to_string(),
                description: "Package change".to_string(),
                severity: Severity::Minor,
            }),
            Box::new(LibraryBreakingChange::ApiSignatureChange {
                name: "func".to_string(),
                location: ChangeLocation::new("lib.rs"),
                old_signature: None,
                new_signature: "fn func()".to_string(),
                description: "Library change".to_string(),
                severity: Severity::Moderate,
            }),
        ];

        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].category(), "asset_format");
        assert_eq!(changes[1].category(), "api_signature");
    }

    #[test]
    fn test_is_severe_trait_method() {
        // Test that is_severe() correctly identifies Major and Critical

        let minor_change = PackageBreakingChange::AssetPathChange {
            old_path: "a.txt".to_string(),
            new_path: Some("b.txt".to_string()),
            is_removal: false,
            is_rename: true,
            description: "Minor".to_string(),
            severity: Severity::Minor,
        };
        assert!(!minor_change.is_severe());

        let moderate_change = PackageBreakingChange::AssetPathChange {
            old_path: "a.txt".to_string(),
            new_path: Some("b.txt".to_string()),
            is_removal: false,
            is_rename: true,
            description: "Moderate".to_string(),
            severity: Severity::Moderate,
        };
        assert!(!moderate_change.is_severe());

        let major_change = PackageBreakingChange::AssetPathChange {
            old_path: "a.txt".to_string(),
            new_path: None,
            is_removal: true,
            is_rename: false,
            description: "Major".to_string(),
            severity: Severity::Major,
        };
        assert!(major_change.is_severe());

        let critical_change = PackageBreakingChange::AssetPathChange {
            old_path: "critical.txt".to_string(),
            new_path: None,
            is_removal: true,
            is_rename: false,
            description: "Critical".to_string(),
            severity: Severity::Critical,
        };
        assert!(critical_change.is_severe());
    }
}
