//! Breaking change detection types for packages and libraries.
//!
//! This module provides data structures for representing two distinct categories
//! of breaking changes:
//!
//! - **Package breaking changes** ([`PackageBreakingChange`]): Changes to assets,
//!   configurations, and non-code artifacts that can break dependent packages
//! - **Library breaking changes** ([`LibraryBreakingChange`]): Changes to code APIs,
//!   types, and behaviors that can break consuming code
//!
//! # Why Two Categories?
//!
//! Breaking changes affect consumers differently depending on whether they impact
//! code or assets:
//!
//! | Aspect | Library Changes | Package Changes |
//! |--------|-----------------|-----------------|
//! | **Impact** | Runtime errors, type mismatches | Build failures, missing resources |
//! | **Detection** | Import statements, type checking | Build configs, static references |
//! | **Migration** | Code refactoring | Configuration updates |
//! | **Examples** | Function signature changes | CSS file removals |
//!
//! # Severity Levels
//!
//! All breaking changes have a [`Severity`] level:
//!
//! - `Minor` - Easy to migrate, low impact
//! - `Moderate` - Requires changes but straightforward (default)
//! - `Major` - Significant migration effort
//! - `Critical` - Fundamental incompatibility
//!
//! # Using Builders
//!
//! For complex changes, use the builder pattern:
//!
//! ```rust
//! use interlint_format::{
//!     ApiSignatureChangeBuilder, ChangeLocation, Severity
//! };
//!
//! let change = ApiSignatureChangeBuilder::new()
//!     .name("greet")
//!     .location(ChangeLocation::with_line("src/lib.rs", 42))
//!     .old_signature("fn greet() -> String")
//!     .new_signature("fn greet(name: &str) -> String")
//!     .description("Added required name parameter")
//!     .severity(Severity::Major)
//!     .build();
//! ```
//!
//! # The BreakingChange Trait
//!
//! Both change types implement [`BreakingChange`], allowing polymorphic handling:
//!
//! ```rust
//! use interlint_format::{BreakingChange, Severity};
//!
//! fn is_blocker(change: &dyn BreakingChange) -> bool {
//!     change.is_severe() // Returns true for Major or Critical
//! }
//! ```
//!
//! # Example: Complete Workflow
//!
//! ```rust
//! use interlint_format::{
//!     PackageBreakingChange, LibraryBreakingChange, BreakingChange,
//!     Severity, ChangeLocation,
//! };
//!
//! // Asset format change in a package
//! let pkg_change = PackageBreakingChange::AssetFormatChange {
//!     asset_path: "styles/main.css".to_string(),
//!     old_format: Some("css3".to_string()),
//!     new_format: "css4".to_string(),
//!     description: "Upgraded to CSS4 syntax".to_string(),
//!     severity: Severity::Major,
//! };
//!
//! // API signature change in a library
//! let lib_change = LibraryBreakingChange::ApiSignatureChange {
//!     name: "process_data".to_string(),
//!     location: ChangeLocation::new("src/processor.ts"),
//!     old_signature: Some("fn process_data(input: &str) -> Result<Data>".to_string()),
//!     new_signature: "fn process_data(input: &str, opts: Options) -> Result<Data>".to_string(),
//!     description: "Added required options parameter".to_string(),
//!     severity: Severity::Major,
//! };
//!
//! // Both implement the common BreakingChange trait
//! assert_eq!(pkg_change.severity(), &Severity::Major);
//! assert_eq!(lib_change.severity(), &Severity::Major);
//! assert_eq!(pkg_change.category(), "asset_format");
//! assert_eq!(lib_change.category(), "api_signature");
//! ```
//!
//! # See Also
//!
//! - [`crate::Asset`] - The asset type tracked for package changes
//! - [`crate::Signature`] - The signature types affected by library changes
//! - [`crate::module`] - Module and export types
//! - [Breaking Change Documentation](../docs/BREAKING_CHANGES.md)

use serde::{Deserialize, Serialize};

/// Severity level of a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Minor breaking change - easy to migrate, low impact
    Minor,
    /// Moderate breaking change - requires code changes but straightforward
    #[default]
    Moderate,
    /// Major breaking change - significant migration effort required
    Major,
    /// Critical breaking change - fundamental incompatibility
    Critical,
}

impl Severity {
    /// Returns true if this severity is at least as severe as the given level.
    pub fn is_at_least(&self, other: &Severity) -> bool {
        self.as_level() >= other.as_level()
    }

    /// Returns a numeric level for comparison (higher = more severe).
    pub fn as_level(&self) -> u8 {
        match self {
            Severity::Minor => 1,
            Severity::Moderate => 2,
            Severity::Major => 3,
            Severity::Critical => 4,
        }
    }
}

/// Location information for a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeLocation {
    /// File path relative to package root
    pub path: String,
    /// Line number where the change occurs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Column number where the change occurs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

impl ChangeLocation {
    /// Create a new location with just a path.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            line: None,
            column: None,
        }
    }

    /// Create a new location with path and line number.
    pub fn with_line(path: impl Into<String>, line: u32) -> Self {
        Self {
            path: path.into(),
            line: Some(line),
            column: None,
        }
    }

    /// Create a new location with path, line, and column.
    pub fn with_position(path: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            path: path.into(),
            line: Some(line),
            column: Some(column),
        }
    }
}

/// Common trait for all breaking changes.
pub trait BreakingChange {
    /// Get the severity of this breaking change.
    fn severity(&self) -> &Severity;

    /// Get a human-readable description of the change.
    fn description(&self) -> &str;

    /// Get the category of breaking change as a string.
    fn category(&self) -> &'static str;

    /// Check if this is a major or critical breaking change.
    fn is_severe(&self) -> bool {
        self.severity().is_at_least(&Severity::Major)
    }
}

// ============================================================================
// Package Breaking Changes
// ============================================================================

/// Breaking changes related to assets and package artifacts.
///
/// These changes affect non-code files and configurations that are part of
/// a package's distribution, such as CSS files, images, JSON schemas, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackageBreakingChange {
    /// Change in asset file format.
    AssetFormatChange {
        /// Path to the asset file
        asset_path: String,
        /// Previous format (if known)
        #[serde(skip_serializing_if = "Option::is_none")]
        old_format: Option<String>,
        /// New format
        new_format: String,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
    },

    /// Change in asset dependencies or references.
    AssetDependencyChange {
        /// Path to the affected asset
        asset_path: String,
        /// Dependencies that were removed
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        removed_dependencies: Vec<String>,
        /// Dependencies that were added
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        added_dependencies: Vec<String>,
        /// Dependencies that were modified
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        modified_dependencies: Vec<String>,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
    },

    /// Change in asset path or location.
    AssetPathChange {
        /// Original path
        old_path: String,
        /// New path (None if asset was removed)
        #[serde(skip_serializing_if = "Option::is_none")]
        new_path: Option<String>,
        /// Whether this is a removal
        #[serde(default)]
        is_removal: bool,
        /// Whether this is a rename
        #[serde(default)]
        is_rename: bool,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
    },

    /// Change in asset metadata or configuration.
    AssetMetadataChange {
        /// Path to the affected asset
        asset_path: String,
        /// Name of the metadata field that changed
        field_name: String,
        /// Previous value (if known)
        #[serde(skip_serializing_if = "Option::is_none")]
        old_value: Option<String>,
        /// New value
        #[serde(skip_serializing_if = "Option::is_none")]
        new_value: Option<String>,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
    },
}

impl BreakingChange for PackageBreakingChange {
    fn severity(&self) -> &Severity {
        match self {
            PackageBreakingChange::AssetFormatChange { severity, .. } => severity,
            PackageBreakingChange::AssetDependencyChange { severity, .. } => severity,
            PackageBreakingChange::AssetPathChange { severity, .. } => severity,
            PackageBreakingChange::AssetMetadataChange { severity, .. } => severity,
        }
    }

    fn description(&self) -> &str {
        match self {
            PackageBreakingChange::AssetFormatChange { description, .. } => description,
            PackageBreakingChange::AssetDependencyChange { description, .. } => description,
            PackageBreakingChange::AssetPathChange { description, .. } => description,
            PackageBreakingChange::AssetMetadataChange { description, .. } => description,
        }
    }

    fn category(&self) -> &'static str {
        match self {
            PackageBreakingChange::AssetFormatChange { .. } => "asset_format",
            PackageBreakingChange::AssetDependencyChange { .. } => "asset_dependency",
            PackageBreakingChange::AssetPathChange { .. } => "asset_path",
            PackageBreakingChange::AssetMetadataChange { .. } => "asset_metadata",
        }
    }
}

impl PackageBreakingChange {
    /// Get the asset path affected by this change.
    pub fn asset_path(&self) -> &str {
        match self {
            PackageBreakingChange::AssetFormatChange { asset_path, .. } => asset_path,
            PackageBreakingChange::AssetDependencyChange { asset_path, .. } => asset_path,
            PackageBreakingChange::AssetPathChange { old_path, .. } => old_path,
            PackageBreakingChange::AssetMetadataChange { asset_path, .. } => asset_path,
        }
    }
}

// ============================================================================
// Library Breaking Changes
// ============================================================================

/// Breaking changes related to code APIs and library interfaces.
///
/// These changes affect the programmatic interface of a library, including
/// function signatures, type definitions, and behavioral contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LibraryBreakingChange {
    /// Change in API function/method signature.
    ApiSignatureChange {
        /// Name of the function/method
        name: String,
        /// Location of the change
        location: ChangeLocation,
        /// Previous signature (if known)
        #[serde(skip_serializing_if = "Option::is_none")]
        old_signature: Option<String>,
        /// New signature
        new_signature: String,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
    },

    /// Change in type definition.
    TypeChange {
        /// Name of the type
        name: String,
        /// Location of the change
        location: ChangeLocation,
        /// Kind of type change
        change_kind: TypeChangeKind,
        /// Previous definition (if known)
        #[serde(skip_serializing_if = "Option::is_none")]
        old_definition: Option<String>,
        /// New definition
        #[serde(skip_serializing_if = "Option::is_none")]
        new_definition: Option<String>,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
    },

    /// Change in runtime behavior.
    BehaviorChange {
        /// Name of the affected component
        name: String,
        /// Location of the change
        location: ChangeLocation,
        /// Previous behavior description
        #[serde(skip_serializing_if = "Option::is_none")]
        old_behavior: Option<String>,
        /// New behavior description
        new_behavior: String,
        /// Whether this change affects error handling
        #[serde(default)]
        affects_error_handling: bool,
        /// Whether this change affects return values
        #[serde(default)]
        affects_return_values: bool,
        /// Whether this change affects side effects
        #[serde(default)]
        affects_side_effects: bool,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
    },

    /// Change in dependency version requirements.
    DependencyVersionChange {
        /// Name of the dependency
        dependency_name: String,
        /// Previous version constraint (if known)
        #[serde(skip_serializing_if = "Option::is_none")]
        old_version: Option<String>,
        /// New version constraint
        new_version: String,
        /// Whether this is a major version bump
        #[serde(default)]
        is_major_bump: bool,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
    },
}

/// Kind of type change in a library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeChangeKind {
    /// Type was removed
    Removed,
    /// Type was renamed
    Renamed,
    /// Type's structure changed (fields added/removed/changed)
    StructureChanged,
    /// Type's generic parameters changed
    GenericsChanged,
    /// Type's constraints/bounds changed
    ConstraintsChanged,
    /// Type's inheritance/implementation changed
    InheritanceChanged,
}

impl BreakingChange for LibraryBreakingChange {
    fn severity(&self) -> &Severity {
        match self {
            LibraryBreakingChange::ApiSignatureChange { severity, .. } => severity,
            LibraryBreakingChange::TypeChange { severity, .. } => severity,
            LibraryBreakingChange::BehaviorChange { severity, .. } => severity,
            LibraryBreakingChange::DependencyVersionChange { severity, .. } => severity,
        }
    }

    fn description(&self) -> &str {
        match self {
            LibraryBreakingChange::ApiSignatureChange { description, .. } => description,
            LibraryBreakingChange::TypeChange { description, .. } => description,
            LibraryBreakingChange::BehaviorChange { description, .. } => description,
            LibraryBreakingChange::DependencyVersionChange { description, .. } => description,
        }
    }

    fn category(&self) -> &'static str {
        match self {
            LibraryBreakingChange::ApiSignatureChange { .. } => "api_signature",
            LibraryBreakingChange::TypeChange { .. } => "type_change",
            LibraryBreakingChange::BehaviorChange { .. } => "behavior",
            LibraryBreakingChange::DependencyVersionChange { .. } => "dependency_version",
        }
    }
}

impl LibraryBreakingChange {
    /// Get the name of the affected component.
    pub fn name(&self) -> &str {
        match self {
            LibraryBreakingChange::ApiSignatureChange { name, .. } => name,
            LibraryBreakingChange::TypeChange { name, .. } => name,
            LibraryBreakingChange::BehaviorChange { name, .. } => name,
            LibraryBreakingChange::DependencyVersionChange {
                dependency_name, ..
            } => dependency_name,
        }
    }

    /// Get the location of the change, if applicable.
    pub fn location(&self) -> Option<&ChangeLocation> {
        match self {
            LibraryBreakingChange::ApiSignatureChange { location, .. } => Some(location),
            LibraryBreakingChange::TypeChange { location, .. } => Some(location),
            LibraryBreakingChange::BehaviorChange { location, .. } => Some(location),
            LibraryBreakingChange::DependencyVersionChange { .. } => None,
        }
    }
}

// ============================================================================
// Builders
// ============================================================================

/// Builder for `PackageBreakingChange::AssetFormatChange`.
#[derive(Debug, Clone, Default)]
pub struct AssetFormatChangeBuilder {
    asset_path: Option<String>,
    old_format: Option<String>,
    new_format: Option<String>,
    description: Option<String>,
    severity: Severity,
}

impl AssetFormatChangeBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the asset path.
    pub fn asset_path(mut self, path: impl Into<String>) -> Self {
        self.asset_path = Some(path.into());
        self
    }

    /// Set the old format.
    pub fn old_format(mut self, format: impl Into<String>) -> Self {
        self.old_format = Some(format.into());
        self
    }

    /// Set the new format.
    pub fn new_format(mut self, format: impl Into<String>) -> Self {
        self.new_format = Some(format.into());
        self
    }

    /// Set the description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the severity.
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Build the `PackageBreakingChange`.
    ///
    /// # Panics
    ///
    /// Panics if required fields (`asset_path`, `new_format`, `description`) are not set.
    pub fn build(self) -> PackageBreakingChange {
        PackageBreakingChange::AssetFormatChange {
            asset_path: self.asset_path.expect("asset_path is required"),
            old_format: self.old_format,
            new_format: self.new_format.expect("new_format is required"),
            description: self.description.expect("description is required"),
            severity: self.severity,
        }
    }
}

/// Builder for `LibraryBreakingChange::ApiSignatureChange`.
#[derive(Debug, Clone, Default)]
pub struct ApiSignatureChangeBuilder {
    name: Option<String>,
    location: Option<ChangeLocation>,
    old_signature: Option<String>,
    new_signature: Option<String>,
    description: Option<String>,
    severity: Severity,
}

impl ApiSignatureChangeBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the function/method name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the location.
    pub fn location(mut self, location: ChangeLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Set the old signature.
    pub fn old_signature(mut self, sig: impl Into<String>) -> Self {
        self.old_signature = Some(sig.into());
        self
    }

    /// Set the new signature.
    pub fn new_signature(mut self, sig: impl Into<String>) -> Self {
        self.new_signature = Some(sig.into());
        self
    }

    /// Set the description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the severity.
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Build the `LibraryBreakingChange`.
    ///
    /// # Panics
    ///
    /// Panics if required fields (`name`, `location`, `new_signature`, `description`) are not set.
    pub fn build(self) -> LibraryBreakingChange {
        LibraryBreakingChange::ApiSignatureChange {
            name: self.name.expect("name is required"),
            location: self.location.expect("location is required"),
            old_signature: self.old_signature,
            new_signature: self.new_signature.expect("new_signature is required"),
            description: self.description.expect("description is required"),
            severity: self.severity,
        }
    }
}

/// Builder for `LibraryBreakingChange::TypeChange`.
#[derive(Debug, Clone, Default)]
pub struct TypeChangeBuilder {
    name: Option<String>,
    location: Option<ChangeLocation>,
    change_kind: Option<TypeChangeKind>,
    old_definition: Option<String>,
    new_definition: Option<String>,
    description: Option<String>,
    severity: Severity,
}

impl TypeChangeBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the type name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the location.
    pub fn location(mut self, location: ChangeLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Set the change kind.
    pub fn change_kind(mut self, kind: TypeChangeKind) -> Self {
        self.change_kind = Some(kind);
        self
    }

    /// Set the old definition.
    pub fn old_definition(mut self, def: impl Into<String>) -> Self {
        self.old_definition = Some(def.into());
        self
    }

    /// Set the new definition.
    pub fn new_definition(mut self, def: impl Into<String>) -> Self {
        self.new_definition = Some(def.into());
        self
    }

    /// Set the description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the severity.
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Build the `LibraryBreakingChange`.
    ///
    /// # Panics
    ///
    /// Panics if required fields (`name`, `location`, `change_kind`, `description`) are not set.
    pub fn build(self) -> LibraryBreakingChange {
        LibraryBreakingChange::TypeChange {
            name: self.name.expect("name is required"),
            location: self.location.expect("location is required"),
            change_kind: self.change_kind.expect("change_kind is required"),
            old_definition: self.old_definition,
            new_definition: self.new_definition,
            description: self.description.expect("description is required"),
            severity: self.severity,
        }
    }
}

/// Builder for `LibraryBreakingChange::BehaviorChange`.
#[derive(Debug, Clone, Default)]
pub struct BehaviorChangeBuilder {
    name: Option<String>,
    location: Option<ChangeLocation>,
    old_behavior: Option<String>,
    new_behavior: Option<String>,
    affects_error_handling: bool,
    affects_return_values: bool,
    affects_side_effects: bool,
    description: Option<String>,
    severity: Severity,
}

impl BehaviorChangeBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the component name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the location.
    pub fn location(mut self, location: ChangeLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Set the old behavior description.
    pub fn old_behavior(mut self, behavior: impl Into<String>) -> Self {
        self.old_behavior = Some(behavior.into());
        self
    }

    /// Set the new behavior description.
    pub fn new_behavior(mut self, behavior: impl Into<String>) -> Self {
        self.new_behavior = Some(behavior.into());
        self
    }

    /// Set whether this affects error handling.
    pub fn affects_error_handling(mut self, affects: bool) -> Self {
        self.affects_error_handling = affects;
        self
    }

    /// Set whether this affects return values.
    pub fn affects_return_values(mut self, affects: bool) -> Self {
        self.affects_return_values = affects;
        self
    }

    /// Set whether this affects side effects.
    pub fn affects_side_effects(mut self, affects: bool) -> Self {
        self.affects_side_effects = affects;
        self
    }

    /// Set the description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the severity.
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Build the `LibraryBreakingChange`.
    ///
    /// # Panics
    ///
    /// Panics if required fields (`name`, `location`, `new_behavior`, `description`) are not set.
    pub fn build(self) -> LibraryBreakingChange {
        LibraryBreakingChange::BehaviorChange {
            name: self.name.expect("name is required"),
            location: self.location.expect("location is required"),
            old_behavior: self.old_behavior,
            new_behavior: self.new_behavior.expect("new_behavior is required"),
            affects_error_handling: self.affects_error_handling,
            affects_return_values: self.affects_return_values,
            affects_side_effects: self.affects_side_effects,
            description: self.description.expect("description is required"),
            severity: self.severity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_comparison() {
        assert!(Severity::Critical.is_at_least(&Severity::Major));
        assert!(Severity::Major.is_at_least(&Severity::Moderate));
        assert!(Severity::Moderate.is_at_least(&Severity::Minor));
        assert!(!Severity::Minor.is_at_least(&Severity::Moderate));
    }

    #[test]
    fn test_change_location() {
        let loc = ChangeLocation::new("src/lib.rs");
        assert_eq!(loc.path, "src/lib.rs");
        assert!(loc.line.is_none());

        let loc_with_line = ChangeLocation::with_line("src/lib.rs", 42);
        assert_eq!(loc_with_line.line, Some(42));

        let loc_with_pos = ChangeLocation::with_position("src/lib.rs", 42, 10);
        assert_eq!(loc_with_pos.line, Some(42));
        assert_eq!(loc_with_pos.column, Some(10));
    }

    #[test]
    fn test_package_breaking_change_trait() {
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
    fn test_library_breaking_change_trait() {
        let change = LibraryBreakingChange::ApiSignatureChange {
            name: "process".to_string(),
            location: ChangeLocation::new("src/processor.rs"),
            old_signature: Some("fn process(data: &str)".to_string()),
            new_signature: "fn process(data: &str, opts: Options)".to_string(),
            description: "Added required options parameter".to_string(),
            severity: Severity::Major,
        };

        assert_eq!(change.severity(), &Severity::Major);
        assert_eq!(change.description(), "Added required options parameter");
        assert_eq!(change.category(), "api_signature");
        assert!(change.is_severe());
        assert_eq!(change.name(), "process");
        assert!(change.location().is_some());
    }

    #[test]
    fn test_asset_format_change_builder() {
        let change = AssetFormatChangeBuilder::new()
            .asset_path("styles/main.css")
            .old_format("css3")
            .new_format("css4")
            .description("Upgraded to CSS4")
            .severity(Severity::Moderate)
            .build();

        if let PackageBreakingChange::AssetFormatChange {
            asset_path,
            old_format,
            new_format,
            description,
            severity,
        } = change
        {
            assert_eq!(asset_path, "styles/main.css");
            assert_eq!(old_format, Some("css3".to_string()));
            assert_eq!(new_format, "css4");
            assert_eq!(description, "Upgraded to CSS4");
            assert_eq!(severity, Severity::Moderate);
        } else {
            panic!("Expected AssetFormatChange variant");
        }
    }

    #[test]
    fn test_api_signature_change_builder() {
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
    fn test_type_change_builder() {
        let change = TypeChangeBuilder::new()
            .name("Config")
            .location(ChangeLocation::new("src/config.rs"))
            .change_kind(TypeChangeKind::StructureChanged)
            .old_definition("struct Config { name: String }")
            .new_definition("struct Config { name: String, version: u32 }")
            .description("Added version field")
            .severity(Severity::Moderate)
            .build();

        if let LibraryBreakingChange::TypeChange {
            name,
            change_kind,
            ..
        } = change
        {
            assert_eq!(name, "Config");
            assert_eq!(change_kind, TypeChangeKind::StructureChanged);
        } else {
            panic!("Expected TypeChange variant");
        }
    }

    #[test]
    fn test_behavior_change_builder() {
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

        if let LibraryBreakingChange::BehaviorChange {
            affects_error_handling,
            affects_return_values,
            affects_side_effects,
            severity,
            ..
        } = change
        {
            assert!(affects_error_handling);
            assert!(affects_return_values);
            assert!(!affects_side_effects);
            assert_eq!(severity, Severity::Critical);
        } else {
            panic!("Expected BehaviorChange variant");
        }
    }

    #[test]
    fn test_serialization_roundtrip() {
        let pkg_change = PackageBreakingChange::AssetPathChange {
            old_path: "assets/logo.png".to_string(),
            new_path: Some("images/logo.png".to_string()),
            is_removal: false,
            is_rename: true,
            description: "Moved logo to images directory".to_string(),
            severity: Severity::Minor,
        };

        let serialized = serde_json::to_string(&pkg_change).unwrap();
        let deserialized: PackageBreakingChange = serde_json::from_str(&serialized).unwrap();
        assert_eq!(pkg_change, deserialized);

        let lib_change = LibraryBreakingChange::DependencyVersionChange {
            dependency_name: "serde".to_string(),
            old_version: Some("1.0".to_string()),
            new_version: "2.0".to_string(),
            is_major_bump: true,
            description: "Upgraded serde to v2".to_string(),
            severity: Severity::Major,
        };

        let serialized = serde_json::to_string(&lib_change).unwrap();
        let deserialized: LibraryBreakingChange = serde_json::from_str(&serialized).unwrap();
        assert_eq!(lib_change, deserialized);
    }
}
