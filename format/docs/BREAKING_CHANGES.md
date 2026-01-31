# Breaking Change Type System

This document describes the breaking change detection types provided by the `interlint-format` crate. The system distinguishes between two primary categories of breaking changes.

## Overview

Breaking changes are categorized into two primary domains:

1. **Code Changes** (`CodeChange`) - Changes to exported code signatures, types, and behaviors
2. **Asset Changes** (`AssetChange`) - Changes to package structure and non-code assets

This distinction helps consumers understand whether a change affects:
- Their code that imports and uses the library's API
- Their build/deployment that depends on package assets (CSS, images, configs)

## Module Organization

The crate provides two modules for breaking change detection:

| Module | Status | Use Case |
|--------|--------|----------|
| `changes` | **Recommended** | New API with `ChangeReport`, `ChangeSummary`, migration hints |
| `breaking_changes` | Legacy | Original builder-based API |

### Quick Start with `changes` Module

```rust
use interlint_format::{
    ChangeReport, BreakingChange, AssetChange, CodeChange,
    AssetFormatChange, SignatureChange, Change, Severity,
};

// Create a change report
let mut report = ChangeReport::new("my-package", "1.0.0", "2.0.0");

// Add an asset format change with migration hint
report.add_change(BreakingChange::Asset(AssetChange::FormatChanged(
    AssetFormatChange::new("config.json", "json", "yaml")
        .with_hint("Convert JSON to YAML using: yq -p json config.json > config.yaml")
)));

// Add a code signature change
report.add_change(BreakingChange::Code(CodeChange::SignatureChanged(
    SignatureChange::new(
        "processData",
        "src/utils.ts",
        "processData(data: string): void",
        "processData(data: Buffer): Promise<void>",
    )
    .with_detail("Parameter type changed from string to Buffer")
    .with_detail("Return type changed to Promise<void>")
    .with_hint("Update call sites to await the Promise")
)));

// Analyze the report
println!("Breaking changes: {}", report.has_breaking_changes());
println!("Recommended bump: {}", report.recommended_bump());
println!("Total changes: {}", report.summary.total_count);
println!("Asset changes: {}", report.summary.asset_count);
println!("Code changes: {}", report.summary.code_count);
```

## Severity Levels

Each breaking change has an associated severity level. The `changes` module uses semver-aligned severity:

| Severity | Semver Bump | Description |
|----------|-------------|-------------|
| `Info` | none | Informational, no version change needed |
| `Patch` | patch | Backwards-compatible bug fix |
| `Minor` | minor | Backwards-compatible addition |
| `Major` | major | Incompatible/breaking change |

```rust
use interlint_format::Severity;

let severity = Severity::Major;

// Check if this is a breaking change
assert!(severity.is_breaking()); // true for Major only

// Get the recommended semver bump
assert_eq!(severity.semver_bump(), "major");

// Severity implements Ord for comparisons
assert!(Severity::Major > Severity::Minor);
assert!(Severity::Minor > Severity::Patch);
assert!(Severity::Patch > Severity::Info);
```

### Legacy Severity (breaking_changes module)

The legacy `breaking_changes` module uses a different severity scale:

| Severity | Level | Description |
|----------|-------|-------------|
| `Minor` | 1 | Easy to migrate, low impact |
| `Moderate` | 2 | Requires code changes but straightforward |
| `Major` | 3 | Significant migration effort required |
| `Critical` | 4 | Fundamental incompatibility |

```rust
use interlint_format::LegacySeverity;

// Check if a severity is at least Major
if severity.is_at_least(&LegacySeverity::Major) {
    println!("This is a severe breaking change!");
}

// Get numeric level for comparison
let level = severity.as_level(); // Returns 1-4
```

## ChangeReport and ChangeSummary

The `ChangeReport` type aggregates all breaking changes between two versions and provides analysis methods.

### ChangeReport

```rust
use interlint_format::{ChangeReport, BreakingChange, AssetChange, CodeChange, Severity};

// Create a new report
let mut report = ChangeReport::new("my-package", "1.0.0", "2.0.0");

// Add changes
report.add_change(/* ... */);

// Query the report
let asset_changes: Vec<_> = report.asset_changes().collect();
let code_changes: Vec<_> = report.code_changes().collect();
let major_changes: Vec<_> = report.changes_by_severity(Severity::Major).collect();

// Check for breaking changes
if report.has_breaking_changes() {
    println!("Breaking changes detected!");
    println!("Recommended version bump: {}", report.recommended_bump());
}
```

### ChangeSummary

The `ChangeSummary` provides statistics about the changes:

```rust
use interlint_format::ChangeSummary;

// ChangeSummary is automatically updated when adding changes to a report
let summary = report.summary;

println!("Total changes: {}", summary.total_count);
println!("Asset changes: {}", summary.asset_count);
println!("Code changes: {}", summary.code_count);
println!("Major (breaking): {}", summary.major_count);
println!("Minor (additions): {}", summary.minor_count);
println!("Patch (fixes): {}", summary.patch_count);
println!("Info (notes): {}", summary.info_count);
```

### Serialization

Both `ChangeReport` and all change types serialize to JSON:

```rust
use interlint_format::ChangeReport;

let report = ChangeReport::new("pkg", "1.0.0", "2.0.0");
let json = serde_json::to_string_pretty(&report)?;
let decoded: ChangeReport = serde_json::from_str(&json)?;
```

## The Change Trait

All change types implement the `Change` trait for polymorphic handling:

```rust
pub trait Change {
    /// Returns the severity of this change.
    fn severity(&self) -> Severity;

    /// Returns a human-readable description of the change.
    fn description(&self) -> String;

    /// Returns the location of the change (path or identifier).
    fn location(&self) -> &str;

    /// Returns the kind of change (asset or code).
    fn kind(&self) -> ChangeKind;

    /// Returns an optional migration hint for addressing this change.
    fn migration_hint(&self) -> Option<&str>;
}
```

### Using migration_hint()

The `migration_hint()` method provides actionable guidance for consumers:

```rust
use interlint_format::{BreakingChange, AssetChange, AssetFormatChange, Change};

let change = BreakingChange::Asset(AssetChange::FormatChanged(
    AssetFormatChange::new("config.json", "json", "yaml")
        .with_hint("Run: yq -p json config.json > config.yaml")
));

if let Some(hint) = change.migration_hint() {
    println!("Migration hint: {}", hint);
}
```

## Code Breaking Changes (changes module)

These changes affect the programmatic API that consumers import and use.

### CodeChange Variants

| Variant | Severity | Description |
|---------|----------|-------------|
| `SignatureChanged` | Major | Function/method signature changed |
| `TypeChanged` | Configurable | Type definition changed |
| `BehaviorChanged` | Configurable | Runtime behavior changed |
| `DependencyVersionChanged` | Configurable | Dependency version changed |
| `ExportRemoved` | Major | Export removed from public API |
| `ExportAdded` | Minor | New export added (informational) |
| `VisibilityChanged` | Configurable | Visibility level changed |
| `ModuleChanged` | Configurable | Module structure changed |
| `GenericConstraintChanged` | Configurable | Generic type constraints changed |
| `DefaultValueChanged` | Configurable | Default value changed |

### ExportRemoved

Represents a removed export from the public API:

```rust
use interlint_format::{CodeChange, ExportRemovedChange};

let change = CodeChange::ExportRemoved(ExportRemovedChange {
    name: "deprecatedFunction".to_string(),
    module_path: "src/utils.ts".to_string(),
    export_type: Some("function".to_string()),
    replacement: Some("newFunction".to_string()),
    migration_hint: Some("Replace deprecatedFunction() with newFunction()".to_string()),
});
```

### VisibilityChanged

Represents a change in visibility level:

```rust
use interlint_format::{CodeChange, VisibilityChange, VisibilityLevel, Severity};

let change = CodeChange::VisibilityChanged(VisibilityChange {
    name: "internalHelper".to_string(),
    module_path: "src/helpers.ts".to_string(),
    old_visibility: VisibilityLevel::Public,
    new_visibility: VisibilityLevel::Private,
    severity: Severity::Major,
    migration_hint: Some("This function is now private. Use publicHelper() instead.".to_string()),
});
```

### GenericConstraintChanged

Represents tighter or changed generic type constraints:

```rust
use interlint_format::{CodeChange, GenericConstraintChange, Severity};
use interlint_format::TypeInfo;

let change = CodeChange::GenericConstraintChanged(GenericConstraintChange {
    name: "Container".to_string(),
    module_path: "src/container.ts".to_string(),
    type_parameter: "T".to_string(),
    old_constraint: None,
    new_constraint: Some(TypeInfo::primitive("Serializable")),
    severity: Severity::Major,
    migration_hint: Some("Type parameter T must now implement Serializable".to_string()),
});
```

### DefaultValueChanged

Represents a change in default parameter or property values:

```rust
use interlint_format::{CodeChange, DefaultValueChange, Severity};

let change = CodeChange::DefaultValueChanged(DefaultValueChange {
    name: "timeout".to_string(),
    module_path: "src/config.ts".to_string(),
    parent_name: Some("fetchOptions".to_string()),
    old_default: Some("5000".to_string()),
    new_default: Some("30000".to_string()),
    severity: Severity::Minor,
    migration_hint: Some("Default timeout increased from 5s to 30s".to_string()),
});
```

### ModuleChanged

Represents structural changes to modules:

```rust
use interlint_format::{CodeChange, ModuleChange, MovedExport, Severity};

let change = CodeChange::ModuleChanged(ModuleChange {
    module_path: "src/index.ts".to_string(),
    change_summary: "Reorganized exports into submodules".to_string(),
    old_structure: Some("All exports in index.ts".to_string()),
    new_structure: Some("Exports split into utils.ts and core.ts".to_string()),
    moved_exports: vec![
        MovedExport {
            name: "formatDate".to_string(),
            old_module: "src/index.ts".to_string(),
            new_module: "src/utils.ts".to_string(),
        },
    ],
    severity: Severity::Major,
    migration_hint: Some("Update imports: import { formatDate } from 'pkg/utils'".to_string()),
});
```

## Asset Breaking Changes (changes module)

These changes affect non-code files distributed with the package.

### AssetChange Variants

| Variant | Severity | Description |
|---------|----------|-------------|
| `FormatChanged` | Major | File format changed |
| `DependencyChanged` | Configurable | Asset dependency changed |
| `PathChanged` | Configurable | Asset path/name changed |
| `MetadataChanged` | Configurable | Asset metadata changed |
| `Removed` | Major | Asset file removed |
| `Added` | Minor | New asset added (informational) |
| `SchemaChanged` | Configurable | JSON/XML schema changed |
| `EncodingChanged` | Major | File encoding changed |

### AssetRemoved

Represents a removed asset file:

```rust
use interlint_format::{AssetChange, AssetRemovedChange};

let change = AssetChange::Removed(AssetRemovedChange {
    path: "assets/legacy-styles.css".to_string(),
    asset_type: Some("css".to_string()),
    migration_hint: Some("Use assets/modern-styles.css instead".to_string()),
});
```

### AssetSchemaChanged

Represents changes to JSON/XML schema structure:

```rust
use interlint_format::{
    AssetChange, AssetSchemaChange, SchemaFieldChange, SchemaFieldChangeType, Severity
};

let change = AssetChange::SchemaChanged(AssetSchemaChange {
    path: "config/schema.json".to_string(),
    change_summary: "Added required 'apiVersion' field".to_string(),
    field_changes: vec![
        SchemaFieldChange {
            field_path: "apiVersion".to_string(),
            change_type: SchemaFieldChangeType::Added,
            old_info: None,
            new_info: Some("string (required)".to_string()),
        },
        SchemaFieldChange {
            field_path: "legacyMode".to_string(),
            change_type: SchemaFieldChangeType::Removed,
            old_info: Some("boolean".to_string()),
            new_info: None,
        },
    ],
    severity: Severity::Major,
    migration_hint: Some("Add 'apiVersion: \"v2\"' to your config files".to_string()),
});
```

### AssetEncodingChanged

Represents file encoding changes:

```rust
use interlint_format::{AssetChange, AssetEncodingChange};

let change = AssetChange::EncodingChanged(AssetEncodingChange {
    path: "data/translations.json".to_string(),
    old_encoding: "UTF-8".to_string(),
    new_encoding: "UTF-16".to_string(),
    migration_hint: Some("Re-save files as UTF-16 or convert with iconv".to_string()),
});
```

## Library/Code Breaking Changes (Legacy API)

These changes affect the programmatic API that consumers import and use.

### API Signature Changes

Represents changes to function or method signatures.

```rust
use interlint_format::{
    LibraryBreakingChange, ChangeLocation, Severity, ApiSignatureChangeBuilder
};

// Using the builder pattern
let change = ApiSignatureChangeBuilder::new()
    .name("greet")
    .location(ChangeLocation::with_line("src/lib.rs", 42))
    .old_signature("fn greet() -> String")
    .new_signature("fn greet(name: &str) -> String")
    .description("Added required name parameter")
    .severity(Severity::Major)
    .build();

// Or construct directly
let change = LibraryBreakingChange::ApiSignatureChange {
    name: "process_data".to_string(),
    location: ChangeLocation::new("src/processor.ts"),
    old_signature: Some("fn process_data(input: &str) -> Result<Data>".to_string()),
    new_signature: "fn process_data(input: &str, opts: Options) -> Result<Data>".to_string(),
    description: "Added required options parameter".to_string(),
    severity: Severity::Major,
};
```

**When to use:**
- Function parameter added, removed, or reordered
- Return type changed
- Async/sync modifier changed
- Generic parameters changed

### Type Changes

Represents changes to type definitions (structs, enums, interfaces).

```rust
use interlint_format::{
    LibraryBreakingChange, ChangeLocation, Severity, TypeChangeKind, TypeChangeBuilder
};

let change = TypeChangeBuilder::new()
    .name("Config")
    .location(ChangeLocation::new("src/config.rs"))
    .change_kind(TypeChangeKind::StructureChanged)
    .old_definition("struct Config { name: String }")
    .new_definition("struct Config { name: String, version: u32 }")
    .description("Added required version field")
    .severity(Severity::Major)
    .build();
```

**TypeChangeKind variants:**

| Kind | Description |
|------|-------------|
| `Removed` | Type was deleted |
| `Renamed` | Type name changed |
| `StructureChanged` | Fields added, removed, or modified |
| `GenericsChanged` | Generic type parameters changed |
| `ConstraintsChanged` | Type bounds/constraints changed |
| `InheritanceChanged` | Base class or interface implementations changed |

### Behavior Changes

Represents changes to runtime behavior that don't affect signatures.

```rust
use interlint_format::{
    LibraryBreakingChange, ChangeLocation, Severity, BehaviorChangeBuilder
};

let change = BehaviorChangeBuilder::new()
    .name("fetch_data")
    .location(ChangeLocation::new("src/api.rs"))
    .old_behavior("Returns empty array on error")
    .new_behavior("Throws exception on error")
    .affects_error_handling(true)
    .affects_return_values(true)
    .description("Changed error handling behavior")
    .severity(Severity::Critical)
    .build();
```

**Behavior flags:**
- `affects_error_handling` - Error throwing/catching behavior changed
- `affects_return_values` - Different values returned for same inputs
- `affects_side_effects` - I/O, logging, or other side effects changed

### Dependency Version Changes

Represents changes to dependency version requirements.

```rust
use interlint_format::{LibraryBreakingChange, Severity};

let change = LibraryBreakingChange::DependencyVersionChange {
    dependency_name: "serde".to_string(),
    old_version: Some("1.0".to_string()),
    new_version: "2.0".to_string(),
    is_major_bump: true,
    description: "Upgraded serde to v2".to_string(),
    severity: Severity::Major,
};
```

## Package/Asset Breaking Changes

These changes affect non-code files distributed with the package.

### Asset Format Changes

Represents changes to the format of asset files.

```rust
use interlint_format::{
    PackageBreakingChange, Severity, AssetFormatChangeBuilder
};

let change = AssetFormatChangeBuilder::new()
    .asset_path("styles/main.css")
    .old_format("css3")
    .new_format("css4")
    .description("Upgraded to CSS4 syntax, requires updated PostCSS")
    .severity(Severity::Major)
    .build();
```

**When to use:**
- CSS syntax version changed
- JSON schema version updated
- Image format changed (PNG to WebP)
- Configuration file format changed

### Asset Path Changes

Represents moved, renamed, or removed assets.

```rust
use interlint_format::{PackageBreakingChange, Severity};

// Asset renamed
let change = PackageBreakingChange::AssetPathChange {
    old_path: "assets/logo.png".to_string(),
    new_path: Some("images/logo.png".to_string()),
    is_removal: false,
    is_rename: true,
    description: "Moved logo to images directory".to_string(),
    severity: Severity::Minor,
};

// Asset removed
let change = PackageBreakingChange::AssetPathChange {
    old_path: "styles/deprecated.css".to_string(),
    new_path: None,
    is_removal: true,
    is_rename: false,
    description: "Removed deprecated stylesheet".to_string(),
    severity: Severity::Major,
};
```

### Asset Dependency Changes

Represents changes to references between assets.

```rust
use interlint_format::{PackageBreakingChange, Severity};

let change = PackageBreakingChange::AssetDependencyChange {
    asset_path: "styles/main.css".to_string(),
    removed_dependencies: vec!["fonts/old-font.woff".to_string()],
    added_dependencies: vec!["fonts/new-font.woff2".to_string()],
    modified_dependencies: vec![],
    description: "Updated font references".to_string(),
    severity: Severity::Moderate,
};
```

### Asset Metadata Changes

Represents changes to asset metadata or configuration.

```rust
use interlint_format::{PackageBreakingChange, Severity};

let change = PackageBreakingChange::AssetMetadataChange {
    asset_path: "config/default.json".to_string(),
    field_name: "apiEndpoint".to_string(),
    old_value: Some("https://api.v1.example.com".to_string()),
    new_value: Some("https://api.v2.example.com".to_string()),
    description: "Updated API endpoint to v2".to_string(),
    severity: Severity::Major,
};
```

## The BreakingChange Trait (Legacy)

Both `LibraryBreakingChange` and `PackageBreakingChange` implement the legacy `BreakingChange` trait:

```rust
pub trait BreakingChange {
    /// Get the severity of this breaking change.
    fn severity(&self) -> &Severity;

    /// Get a human-readable description of the change.
    fn description(&self) -> &str;

    /// Get the category of breaking change as a string.
    fn category(&self) -> &'static str;

    /// Check if this is a major or critical breaking change.
    fn is_severe(&self) -> bool;
}
```

This allows polymorphic handling of any breaking change:

```rust
use interlint_format::{
    LegacyBreakingChange, LibraryBreakingChange, PackageBreakingChange, LegacySeverity
};

fn process_change(change: &dyn LegacyBreakingChange) {
    println!("Category: {}", change.category());
    println!("Description: {}", change.description());
    println!("Severity: {:?}", change.severity());

    if change.is_severe() {
        println!("WARNING: This is a severe breaking change!");
    }
}
```

> **Note**: For new code, prefer the `Change` trait from the `changes` module which includes
> `migration_hint()` support and semver-aligned severity levels.

## Location Information

The `ChangeLocation` struct provides precise location information:

```rust
use interlint_format::ChangeLocation;

// Just a file path
let loc = ChangeLocation::new("src/lib.rs");

// File path with line number
let loc = ChangeLocation::with_line("src/lib.rs", 42);

// File path with line and column
let loc = ChangeLocation::with_position("src/lib.rs", 42, 15);
```

## Distinguishing Code vs Asset Changes

When processing a list of breaking changes:

```rust
use interlint_format::{LibraryBreakingChange, PackageBreakingChange, BreakingChange};

enum AnyBreakingChange {
    Library(LibraryBreakingChange),
    Package(PackageBreakingChange),
}

fn categorize_changes(changes: &[AnyBreakingChange]) -> (Vec<&LibraryBreakingChange>, Vec<&PackageBreakingChange>) {
    let mut library_changes = Vec::new();
    let mut package_changes = Vec::new();

    for change in changes {
        match change {
            AnyBreakingChange::Library(c) => library_changes.push(c),
            AnyBreakingChange::Package(c) => package_changes.push(c),
        }
    }

    (library_changes, package_changes)
}
```

## Semver Implications

### Using ChangeReport (Recommended)

The `ChangeReport.recommended_bump()` method automatically determines the semver bump:

```rust
use interlint_format::ChangeReport;

let report = ChangeReport::new("pkg", "1.0.0", "2.0.0");
// ... add changes ...

match report.recommended_bump() {
    "major" => println!("Bump to next major version"),
    "minor" => println!("Bump to next minor version"),
    "patch" => println!("Bump to next patch version"),
    "none" => println!("No version change needed"),
    _ => unreachable!(),
}
```

### Severity to Semver Mapping

The `changes` module uses semver-aligned severity:

| Severity | Semver Bump | Version Change |
|----------|-------------|----------------|
| `Major` | major | X.0.0 |
| `Minor` | minor | x.Y.0 |
| `Patch` | patch | x.y.Z |
| `Info` | none | (no change) |

```rust
use interlint_format::Severity;

let severity = Severity::Major;
assert_eq!(severity.semver_bump(), "major");
```

### Legacy Severity Mapping

The `breaking_changes` module maps differently:

| Highest Severity | Suggested Bump |
|------------------|----------------|
| Critical | Major (X.0.0) |
| Major | Major (X.0.0) |
| Moderate | Minor (x.Y.0) |
| Minor | Patch (x.y.Z) |

## Best Practices

### For Library Authors

1. **Avoid Critical changes** between minor versions
2. **Deprecate before removing** - Add deprecation warnings before removing exports
3. **Document asset changes** - Note when CSS/image paths change
4. **Use builders** - The builder pattern prevents invalid change objects

### For Consumers

1. **Pin major versions** - Use `^1.0.0` to avoid major breaking changes
2. **Review changelogs** - Check for asset changes if you import CSS/images
3. **Check severity** - Use `is_severe()` to filter important changes
4. **Categorize by type** - Handle library and package changes differently

## See Also

- [Binary Format Documentation](../README.md) - ILMJ binary format specification
- [`changes` module](../src/changes.rs) - **Recommended**: New API with ChangeReport, migration hints
- [`breaking_changes` module](../src/breaking_changes.rs) - Legacy builder-based API
- [`module`](../src/module.rs) - Asset and Module types
- [`signature`](../src/signature.rs) - Signature types affected by library changes

## Migration from Legacy API

To migrate from `breaking_changes` to `changes` module:

```rust
// Before (legacy)
use interlint_format::{
    LibraryBreakingChange, PackageBreakingChange,
    ApiSignatureChangeBuilder, LegacySeverity,
};

let change = ApiSignatureChangeBuilder::new()
    .name("myFunc")
    .location(ChangeLocation::new("src/lib.rs"))
    .new_signature("fn myFunc(x: i32)")
    .description("Added parameter")
    .severity(LegacySeverity::Major)
    .build();

// After (recommended)
use interlint_format::{
    ChangeReport, BreakingChange, CodeChange, SignatureChange, Severity,
};

let mut report = ChangeReport::new("my-pkg", "1.0.0", "2.0.0");
report.add_change(BreakingChange::Code(CodeChange::SignatureChanged(
    SignatureChange::new(
        "myFunc",
        "src/lib.rs",
        "fn myFunc()",
        "fn myFunc(x: i32)",
    )
    .with_detail("Added parameter x: i32")
    .with_hint("Update all call sites to pass the new parameter")
)));

// Now you have statistics and semver recommendations
println!("Recommended bump: {}", report.recommended_bump());
```
