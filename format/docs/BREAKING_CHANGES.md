# Breaking Change Type System

This document describes the breaking change detection types provided by the `interlint-format` crate. The system distinguishes between two primary categories of breaking changes.

## Overview

Breaking changes are categorized into two primary domains:

1. **Library/Code Breaking Changes** (`LibraryBreakingChange`) - Changes to exported code signatures, types, and behaviors
2. **Package/Asset Breaking Changes** (`PackageBreakingChange`) - Changes to package structure and non-code assets

This distinction helps consumers understand whether a change affects:
- Their code that imports and uses the library's API
- Their build/deployment that depends on package assets (CSS, images, configs)

## Severity Levels

Each breaking change has an associated severity level:

| Severity | Level | Description |
|----------|-------|-------------|
| `Minor` | 1 | Easy to migrate, low impact |
| `Moderate` | 2 | Requires code changes but straightforward |
| `Major` | 3 | Significant migration effort required |
| `Critical` | 4 | Fundamental incompatibility |

```rust
use interlint_format::Severity;

// Check if a severity is at least Major
if severity.is_at_least(&Severity::Major) {
    println!("This is a severe breaking change!");
}

// Get numeric level for comparison
let level = severity.as_level(); // Returns 1-4
```

## Library/Code Breaking Changes

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

## The BreakingChange Trait

Both `LibraryBreakingChange` and `PackageBreakingChange` implement the `BreakingChange` trait:

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
use interlint_format::{BreakingChange, LibraryBreakingChange, PackageBreakingChange, Severity};

fn process_change(change: &dyn BreakingChange) {
    println!("Category: {}", change.category());
    println!("Description: {}", change.description());
    println!("Severity: {:?}", change.severity());

    if change.is_severe() {
        println!("WARNING: This is a severe breaking change!");
    }
}
```

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

The breaking change system maps to semantic versioning:

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
- [`breaking_changes` module](../src/breaking_changes.rs) - Full implementation
- [`module`](../src/module.rs) - Asset and Module types
- [`signature`](../src/signature.rs) - Signature types affected by library changes
