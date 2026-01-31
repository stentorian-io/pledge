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
//!     ApiSignatureChangeBuilder, ChangeLocation,
//!     LegacySeverity as Severity,
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
//! use interlint_format::{BreakingChangeTrait, LegacySeverity as Severity};
//!
//! fn is_blocker(change: &dyn BreakingChangeTrait) -> bool {
//!     change.is_severe() // Returns true for Major or Critical
//! }
//! ```
//!
//! # Example: Complete Workflow
//!
//! ```rust
//! use interlint_format::{
//!     PackageBreakingChange, LibraryBreakingChange, BreakingChangeTrait,
//!     LegacySeverity as Severity, ChangeLocation,
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
///
/// Severity levels are ordered from least to most severe:
/// `Minor < Moderate < Major < Critical`
///
/// This ordering allows for easy comparison using standard comparison operators.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
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

    /// Returns the maximum of two severities.
    pub fn max(self, other: Severity) -> Severity {
        if self.as_level() >= other.as_level() {
            self
        } else {
            other
        }
    }
}

/// Suggested semantic version bump based on breaking changes.
///
/// Semver bumps are ordered from least to most significant:
/// `None < Patch < Minor < Major`
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum SemverBump {
    /// No version bump needed
    #[default]
    None,
    /// Patch version bump (backwards-compatible bug fixes)
    Patch,
    /// Minor version bump (backwards-compatible functionality)
    Minor,
    /// Major version bump (breaking changes)
    Major,
}

impl SemverBump {
    /// Returns a numeric level for comparison (higher = more significant).
    pub fn as_level(&self) -> u8 {
        match self {
            SemverBump::None => 0,
            SemverBump::Patch => 1,
            SemverBump::Minor => 2,
            SemverBump::Major => 3,
        }
    }

    /// Returns the maximum of two semver bumps.
    pub fn max(self, other: SemverBump) -> SemverBump {
        if self.as_level() >= other.as_level() {
            self
        } else {
            other
        }
    }
}

/// Location information for a change.
///
/// Can be used as a HashMap key due to Hash implementation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            line: None,
            column: None,
        }
    }

    /// Create a new location with path and line number.
    #[must_use]
    pub fn with_line(path: impl Into<String>, line: u32) -> Self {
        Self {
            path: path.into(),
            line: Some(line),
            column: None,
        }
    }

    /// Create a new location with path, line, and column.
    #[must_use]
    pub fn with_position(path: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            path: path.into(),
            line: Some(line),
            column: Some(column),
        }
    }
}

/// Summary statistics for a collection of breaking changes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSummary {
    /// Total number of breaking changes
    pub total_count: usize,
    /// Number of package-level changes
    pub package_count: usize,
    /// Number of library-level changes
    pub library_count: usize,
    /// Maximum severity among all changes
    #[serde(default)]
    pub max_severity: Severity,
}

impl ChangeSummary {
    /// Create a new empty summary.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if there are any breaking changes.
    pub fn has_changes(&self) -> bool {
        self.total_count > 0
    }

    /// Check if there are any severe (Major or Critical) changes.
    pub fn has_severe_changes(&self) -> bool {
        self.max_severity.is_at_least(&Severity::Major)
    }
}

/// A report containing all breaking changes between two versions.
///
/// This aggregate structure collects both package and library breaking changes
/// and provides summary statistics and version bump recommendations.
///
/// # Example
///
/// ```rust
/// use interlint_format::breaking_changes::{
///     ChangeReport, PackageBreakingChange, Severity, SemverBump,
/// };
///
/// let mut report = ChangeReport::new("my-package", "1.0.0", "2.0.0");
///
/// report.add_package_change(PackageBreakingChange::AssetPathChange {
///     old_path: "assets/logo.png".to_string(),
///     new_path: None,
///     is_removal: true,
///     is_rename: false,
///     description: "Removed logo asset".to_string(),
///     severity: Severity::Major,
/// });
///
/// report.compute_summary();
///
/// assert_eq!(report.summary.total_count, 1);
/// assert_eq!(report.suggested_bump(), SemverBump::Major);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeReport {
    /// Name of the package being compared
    pub package_name: String,
    /// Previous version being compared from
    pub old_version: String,
    /// New version being compared to
    pub new_version: String,
    /// Package-level breaking changes
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub package_changes: Vec<PackageBreakingChange>,
    /// Library-level breaking changes
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub library_changes: Vec<LibraryBreakingChange>,
    /// Summary statistics for all changes
    #[serde(default)]
    pub summary: ChangeSummary,
}

impl Default for ChangeReport {
    fn default() -> Self {
        Self {
            package_name: String::new(),
            old_version: String::new(),
            new_version: String::new(),
            package_changes: Vec::new(),
            library_changes: Vec::new(),
            summary: ChangeSummary::default(),
        }
    }
}

impl ChangeReport {
    /// Create a new empty change report.
    #[must_use]
    pub fn new(
        package_name: impl Into<String>,
        old_version: impl Into<String>,
        new_version: impl Into<String>,
    ) -> Self {
        Self {
            package_name: package_name.into(),
            old_version: old_version.into(),
            new_version: new_version.into(),
            package_changes: Vec::new(),
            library_changes: Vec::new(),
            summary: ChangeSummary::default(),
        }
    }

    /// Add a package-level breaking change.
    pub fn add_package_change(&mut self, change: PackageBreakingChange) {
        self.package_changes.push(change);
    }

    /// Add a library-level breaking change.
    pub fn add_library_change(&mut self, change: LibraryBreakingChange) {
        self.library_changes.push(change);
    }

    /// Compute the summary statistics from the current changes.
    ///
    /// This should be called after all changes have been added to update
    /// the `summary` field with accurate counts and severity information.
    pub fn compute_summary(&mut self) {
        let package_count = self.package_changes.len();
        let library_count = self.library_changes.len();
        let total_count = package_count + library_count;

        let mut max_severity = Severity::default();

        for change in &self.package_changes {
            max_severity = max_severity.max(*change.severity());
        }

        for change in &self.library_changes {
            max_severity = max_severity.max(*change.severity());
        }

        // If there are no changes, reset to default severity
        if total_count == 0 {
            max_severity = Severity::default();
        }

        self.summary = ChangeSummary {
            total_count,
            package_count,
            library_count,
            max_severity,
        };
    }

    /// Get the suggested semver bump based on all changes.
    ///
    /// Returns the highest semver bump required by any change in the report.
    pub fn suggested_bump(&self) -> SemverBump {
        let mut max_bump = SemverBump::None;

        for change in &self.package_changes {
            max_bump = max_bump.max(change.semver_bump());
        }

        for change in &self.library_changes {
            max_bump = max_bump.max(change.semver_bump());
        }

        max_bump
    }

    /// Check if the report has any breaking changes.
    pub fn has_changes(&self) -> bool {
        !self.package_changes.is_empty() || !self.library_changes.is_empty()
    }

    /// Check if any changes are severe (Major or Critical).
    pub fn has_severe_changes(&self) -> bool {
        self.package_changes.iter().any(|c| c.is_severe())
            || self.library_changes.iter().any(|c| c.is_severe())
    }

    /// Get the total number of breaking changes.
    pub fn total_changes(&self) -> usize {
        self.package_changes.len() + self.library_changes.len()
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

    /// Get an optional migration hint for this change.
    ///
    /// Returns guidance on how to migrate code affected by this breaking change.
    fn migration_hint(&self) -> Option<&str> {
        None
    }

    /// Get the suggested semver bump for this change based on its severity.
    fn semver_bump(&self) -> SemverBump {
        match self.severity() {
            Severity::Minor => SemverBump::Patch,
            Severity::Moderate => SemverBump::Minor,
            Severity::Major | Severity::Critical => SemverBump::Major,
        }
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

    /// Asset was completely removed from the package.
    AssetRemoved {
        /// Path to the removed asset
        asset_path: String,
        /// Type of asset (e.g., "css", "image", "config")
        #[serde(skip_serializing_if = "Option::is_none")]
        asset_type: Option<String>,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
        /// Hint for replacement or migration
        #[serde(skip_serializing_if = "Option::is_none")]
        replacement_hint: Option<String>,
    },

    /// Asset schema/structure changed (for JSON, YAML, TOML configs).
    AssetSchemaChange {
        /// Path to the affected asset
        asset_path: String,
        /// Fields that were added to the schema
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        added_fields: Vec<String>,
        /// Fields that were removed from the schema
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        removed_fields: Vec<String>,
        /// Fields that had their type or structure changed
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        changed_fields: Vec<String>,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
    },

    /// Asset encoding changed (e.g., UTF-8 to UTF-16).
    AssetEncodingChange {
        /// Path to the affected asset
        asset_path: String,
        /// Previous encoding (if known)
        #[serde(skip_serializing_if = "Option::is_none")]
        old_encoding: Option<String>,
        /// New encoding
        new_encoding: String,
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
            PackageBreakingChange::AssetRemoved { severity, .. } => severity,
            PackageBreakingChange::AssetSchemaChange { severity, .. } => severity,
            PackageBreakingChange::AssetEncodingChange { severity, .. } => severity,
        }
    }

    fn description(&self) -> &str {
        match self {
            PackageBreakingChange::AssetFormatChange { description, .. } => description,
            PackageBreakingChange::AssetDependencyChange { description, .. } => description,
            PackageBreakingChange::AssetPathChange { description, .. } => description,
            PackageBreakingChange::AssetMetadataChange { description, .. } => description,
            PackageBreakingChange::AssetRemoved { description, .. } => description,
            PackageBreakingChange::AssetSchemaChange { description, .. } => description,
            PackageBreakingChange::AssetEncodingChange { description, .. } => description,
        }
    }

    fn category(&self) -> &'static str {
        match self {
            PackageBreakingChange::AssetFormatChange { .. } => "asset_format",
            PackageBreakingChange::AssetDependencyChange { .. } => "asset_dependency",
            PackageBreakingChange::AssetPathChange { .. } => "asset_path",
            PackageBreakingChange::AssetMetadataChange { .. } => "asset_metadata",
            PackageBreakingChange::AssetRemoved { .. } => "asset_removed",
            PackageBreakingChange::AssetSchemaChange { .. } => "asset_schema",
            PackageBreakingChange::AssetEncodingChange { .. } => "asset_encoding",
        }
    }

    fn migration_hint(&self) -> Option<&str> {
        match self {
            PackageBreakingChange::AssetRemoved {
                replacement_hint, ..
            } => replacement_hint.as_deref(),
            _ => None,
        }
    }
}

impl PackageBreakingChange {
    /// Get the asset path affected by this change.
    #[must_use]
    pub fn asset_path(&self) -> &str {
        match self {
            PackageBreakingChange::AssetFormatChange { asset_path, .. } => asset_path,
            PackageBreakingChange::AssetDependencyChange { asset_path, .. } => asset_path,
            PackageBreakingChange::AssetPathChange { old_path, .. } => old_path,
            PackageBreakingChange::AssetMetadataChange { asset_path, .. } => asset_path,
            PackageBreakingChange::AssetRemoved { asset_path, .. } => asset_path,
            PackageBreakingChange::AssetSchemaChange { asset_path, .. } => asset_path,
            PackageBreakingChange::AssetEncodingChange { asset_path, .. } => asset_path,
        }
    }
}

// ============================================================================
// Library Breaking Changes
// ============================================================================

/// Data for a runtime behavior change.
///
/// This struct is boxed in [`LibraryBreakingChange::BehaviorChange`] to reduce
/// the size of the enum (optimization for memory layout).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorChangeData {
    /// Name of the affected component
    pub name: String,
    /// Location of the change
    pub location: ChangeLocation,
    /// Previous behavior description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_behavior: Option<String>,
    /// New behavior description
    pub new_behavior: String,
    /// Whether this change affects error handling
    #[serde(default)]
    pub affects_error_handling: bool,
    /// Whether this change affects return values
    #[serde(default)]
    pub affects_return_values: bool,
    /// Whether this change affects side effects
    #[serde(default)]
    pub affects_side_effects: bool,
    /// Human-readable description of the change
    pub description: String,
    /// Severity of the change
    #[serde(default)]
    pub severity: Severity,
}

/// Breaking changes related to code APIs and library interfaces.
///
/// These changes affect the programmatic interface of a library, including
/// function signatures, type definitions, and behavioral contracts.
///
/// Note: The `BehaviorChange` variant uses `Box<BehaviorChangeData>` to reduce
/// the overall enum size, as it has the most fields.
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

    /// Change in runtime behavior (boxed to reduce enum size).
    BehaviorChange(Box<BehaviorChangeData>),

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

    /// Export was removed from module.
    ExportRemoved {
        /// Name of the removed export
        name: String,
        /// Path to the module containing the export
        module_path: String,
        /// Kind of export ("function", "class", "interface", etc.)
        export_kind: String,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
        /// Suggested replacement for the removed export
        #[serde(skip_serializing_if = "Option::is_none")]
        replacement: Option<String>,
    },

    /// Visibility changed (e.g., public to private).
    VisibilityChange {
        /// Name of the affected item
        name: String,
        /// Location of the change
        location: ChangeLocation,
        /// Previous visibility (e.g., "public", "protected")
        old_visibility: String,
        /// New visibility
        new_visibility: String,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
    },

    /// Module structure changed (exports moved between modules).
    ModuleStructureChange {
        /// Original module path
        old_path: String,
        /// New module path (None if module was removed)
        #[serde(skip_serializing_if = "Option::is_none")]
        new_path: Option<String>,
        /// Exports affected by this change
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        affected_exports: Vec<String>,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
    },

    /// Generic type constraint changed.
    GenericConstraintChange {
        /// Name of the affected type/function
        name: String,
        /// Location of the change
        location: ChangeLocation,
        /// Name of the type parameter that changed
        type_parameter: String,
        /// Previous constraint (if any)
        #[serde(skip_serializing_if = "Option::is_none")]
        old_constraint: Option<String>,
        /// New constraint (if any)
        #[serde(skip_serializing_if = "Option::is_none")]
        new_constraint: Option<String>,
        /// Human-readable description of the change
        description: String,
        /// Severity of the change
        #[serde(default)]
        severity: Severity,
    },

    /// Default parameter value changed.
    DefaultValueChange {
        /// Name of the function/method
        name: String,
        /// Location of the change
        location: ChangeLocation,
        /// Name of the parameter
        parameter_name: String,
        /// Previous default value (if any)
        #[serde(skip_serializing_if = "Option::is_none")]
        old_default: Option<String>,
        /// New default value (if any)
        #[serde(skip_serializing_if = "Option::is_none")]
        new_default: Option<String>,
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
            LibraryBreakingChange::BehaviorChange(data) => &data.severity,
            LibraryBreakingChange::DependencyVersionChange { severity, .. } => severity,
            LibraryBreakingChange::ExportRemoved { severity, .. } => severity,
            LibraryBreakingChange::VisibilityChange { severity, .. } => severity,
            LibraryBreakingChange::ModuleStructureChange { severity, .. } => severity,
            LibraryBreakingChange::GenericConstraintChange { severity, .. } => severity,
            LibraryBreakingChange::DefaultValueChange { severity, .. } => severity,
        }
    }

    fn description(&self) -> &str {
        match self {
            LibraryBreakingChange::ApiSignatureChange { description, .. } => description,
            LibraryBreakingChange::TypeChange { description, .. } => description,
            LibraryBreakingChange::BehaviorChange(data) => &data.description,
            LibraryBreakingChange::DependencyVersionChange { description, .. } => description,
            LibraryBreakingChange::ExportRemoved { description, .. } => description,
            LibraryBreakingChange::VisibilityChange { description, .. } => description,
            LibraryBreakingChange::ModuleStructureChange { description, .. } => description,
            LibraryBreakingChange::GenericConstraintChange { description, .. } => description,
            LibraryBreakingChange::DefaultValueChange { description, .. } => description,
        }
    }

    fn category(&self) -> &'static str {
        match self {
            LibraryBreakingChange::ApiSignatureChange { .. } => "api_signature",
            LibraryBreakingChange::TypeChange { .. } => "type_change",
            LibraryBreakingChange::BehaviorChange(_) => "behavior",
            LibraryBreakingChange::DependencyVersionChange { .. } => "dependency_version",
            LibraryBreakingChange::ExportRemoved { .. } => "export_removed",
            LibraryBreakingChange::VisibilityChange { .. } => "visibility_change",
            LibraryBreakingChange::ModuleStructureChange { .. } => "module_structure",
            LibraryBreakingChange::GenericConstraintChange { .. } => "generic_constraint",
            LibraryBreakingChange::DefaultValueChange { .. } => "default_value",
        }
    }

    fn migration_hint(&self) -> Option<&str> {
        match self {
            LibraryBreakingChange::ExportRemoved { replacement, .. } => replacement.as_deref(),
            _ => None,
        }
    }
}

impl LibraryBreakingChange {
    /// Get the name of the affected component.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            LibraryBreakingChange::ApiSignatureChange { name, .. } => name,
            LibraryBreakingChange::TypeChange { name, .. } => name,
            LibraryBreakingChange::BehaviorChange(data) => &data.name,
            LibraryBreakingChange::DependencyVersionChange {
                dependency_name, ..
            } => dependency_name,
            LibraryBreakingChange::ExportRemoved { name, .. } => name,
            LibraryBreakingChange::VisibilityChange { name, .. } => name,
            LibraryBreakingChange::ModuleStructureChange { old_path, .. } => old_path,
            LibraryBreakingChange::GenericConstraintChange { name, .. } => name,
            LibraryBreakingChange::DefaultValueChange { name, .. } => name,
        }
    }

    /// Get the location of the change, if applicable.
    #[must_use]
    pub fn location(&self) -> Option<&ChangeLocation> {
        match self {
            LibraryBreakingChange::ApiSignatureChange { location, .. } => Some(location),
            LibraryBreakingChange::TypeChange { location, .. } => Some(location),
            LibraryBreakingChange::BehaviorChange(data) => Some(&data.location),
            LibraryBreakingChange::DependencyVersionChange { .. } => None,
            LibraryBreakingChange::ExportRemoved { .. } => None,
            LibraryBreakingChange::VisibilityChange { location, .. } => Some(location),
            LibraryBreakingChange::ModuleStructureChange { .. } => None,
            LibraryBreakingChange::GenericConstraintChange { location, .. } => Some(location),
            LibraryBreakingChange::DefaultValueChange { location, .. } => Some(location),
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the asset path.
    #[must_use]
    pub fn asset_path(mut self, path: impl Into<String>) -> Self {
        self.asset_path = Some(path.into());
        self
    }

    /// Set the old format.
    #[must_use]
    pub fn old_format(mut self, format: impl Into<String>) -> Self {
        self.old_format = Some(format.into());
        self
    }

    /// Set the new format.
    #[must_use]
    pub fn new_format(mut self, format: impl Into<String>) -> Self {
        self.new_format = Some(format.into());
        self
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the severity.
    #[must_use]
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Build the `PackageBreakingChange`.
    ///
    /// # Panics
    ///
    /// Panics if required fields (`asset_path`, `new_format`, `description`) are not set.
    #[must_use]
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the function/method name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the location.
    #[must_use]
    pub fn location(mut self, location: ChangeLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Set the old signature.
    #[must_use]
    pub fn old_signature(mut self, sig: impl Into<String>) -> Self {
        self.old_signature = Some(sig.into());
        self
    }

    /// Set the new signature.
    #[must_use]
    pub fn new_signature(mut self, sig: impl Into<String>) -> Self {
        self.new_signature = Some(sig.into());
        self
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the severity.
    #[must_use]
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Build the `LibraryBreakingChange`.
    ///
    /// # Panics
    ///
    /// Panics if required fields (`name`, `location`, `new_signature`, `description`) are not set.
    #[must_use]
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the type name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the location.
    #[must_use]
    pub fn location(mut self, location: ChangeLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Set the change kind.
    #[must_use]
    pub fn change_kind(mut self, kind: TypeChangeKind) -> Self {
        self.change_kind = Some(kind);
        self
    }

    /// Set the old definition.
    #[must_use]
    pub fn old_definition(mut self, def: impl Into<String>) -> Self {
        self.old_definition = Some(def.into());
        self
    }

    /// Set the new definition.
    #[must_use]
    pub fn new_definition(mut self, def: impl Into<String>) -> Self {
        self.new_definition = Some(def.into());
        self
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the severity.
    #[must_use]
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Build the `LibraryBreakingChange`.
    ///
    /// # Panics
    ///
    /// Panics if required fields (`name`, `location`, `change_kind`, `description`) are not set.
    #[must_use]
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the component name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the location.
    #[must_use]
    pub fn location(mut self, location: ChangeLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Set the old behavior description.
    #[must_use]
    pub fn old_behavior(mut self, behavior: impl Into<String>) -> Self {
        self.old_behavior = Some(behavior.into());
        self
    }

    /// Set the new behavior description.
    #[must_use]
    pub fn new_behavior(mut self, behavior: impl Into<String>) -> Self {
        self.new_behavior = Some(behavior.into());
        self
    }

    /// Set whether this affects error handling.
    #[must_use]
    pub fn affects_error_handling(mut self, affects: bool) -> Self {
        self.affects_error_handling = affects;
        self
    }

    /// Set whether this affects return values.
    #[must_use]
    pub fn affects_return_values(mut self, affects: bool) -> Self {
        self.affects_return_values = affects;
        self
    }

    /// Set whether this affects side effects.
    #[must_use]
    pub fn affects_side_effects(mut self, affects: bool) -> Self {
        self.affects_side_effects = affects;
        self
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the severity.
    #[must_use]
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Build the `LibraryBreakingChange`.
    ///
    /// # Panics
    ///
    /// Panics if required fields (`name`, `location`, `new_behavior`, `description`) are not set.
    #[must_use]
    pub fn build(self) -> LibraryBreakingChange {
        LibraryBreakingChange::BehaviorChange(Box::new(BehaviorChangeData {
            name: self.name.expect("name is required"),
            location: self.location.expect("location is required"),
            old_behavior: self.old_behavior,
            new_behavior: self.new_behavior.expect("new_behavior is required"),
            affects_error_handling: self.affects_error_handling,
            affects_return_values: self.affects_return_values,
            affects_side_effects: self.affects_side_effects,
            description: self.description.expect("description is required"),
            severity: self.severity,
        }))
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
            name, change_kind, ..
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

        if let LibraryBreakingChange::BehaviorChange(data) = change {
            assert!(data.affects_error_handling);
            assert!(data.affects_return_values);
            assert!(!data.affects_side_effects);
            assert_eq!(data.severity, Severity::Critical);
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

    // ========================================================================
    // Performance Optimization Tests
    // ========================================================================

    #[test]
    fn test_severity_ord_trait() {
        // Test PartialOrd/Ord derives for easy comparison
        assert!(Severity::Minor < Severity::Moderate);
        assert!(Severity::Moderate < Severity::Major);
        assert!(Severity::Major < Severity::Critical);
        assert!(Severity::Critical > Severity::Minor);

        // Test max using Ord
        assert_eq!(Severity::Minor.max(Severity::Major), Severity::Major);
        assert_eq!(Severity::Critical.max(Severity::Minor), Severity::Critical);
    }

    #[test]
    fn test_semver_bump_ord_trait() {
        // Test PartialOrd/Ord derives
        assert!(SemverBump::None < SemverBump::Patch);
        assert!(SemverBump::Patch < SemverBump::Minor);
        assert!(SemverBump::Minor < SemverBump::Major);

        // Test max using Ord
        assert_eq!(SemverBump::Patch.max(SemverBump::Major), SemverBump::Major);
        assert_eq!(SemverBump::Minor.max(SemverBump::None), SemverBump::Minor);
    }

    #[test]
    fn test_change_location_as_hash_key() {
        use std::collections::HashMap;

        let mut map: HashMap<ChangeLocation, String> = HashMap::new();

        let loc1 = ChangeLocation::with_line("src/lib.rs", 42);
        let loc2 = ChangeLocation::with_position("src/main.rs", 10, 5);

        map.insert(loc1.clone(), "first".to_string());
        map.insert(loc2.clone(), "second".to_string());

        assert_eq!(map.get(&loc1), Some(&"first".to_string()));
        assert_eq!(map.get(&loc2), Some(&"second".to_string()));

        // Test that equal locations map to same entry
        let loc1_copy = ChangeLocation::with_line("src/lib.rs", 42);
        assert_eq!(map.get(&loc1_copy), Some(&"first".to_string()));
    }

    #[test]
    fn test_change_report_default() {
        let report = ChangeReport::default();
        assert!(report.package_name.is_empty());
        assert!(report.old_version.is_empty());
        assert!(report.new_version.is_empty());
        assert!(report.package_changes.is_empty());
        assert!(report.library_changes.is_empty());
        assert_eq!(report.summary.total_count, 0);
    }

    #[test]
    fn test_change_report_workflow() {
        let mut report = ChangeReport::new("my-package", "1.0.0", "2.0.0");

        // Add a package change
        report.add_package_change(PackageBreakingChange::AssetPathChange {
            old_path: "assets/logo.png".to_string(),
            new_path: None,
            is_removal: true,
            is_rename: false,
            description: "Removed logo".to_string(),
            severity: Severity::Major,
        });

        // Add a library change
        report.add_library_change(LibraryBreakingChange::ApiSignatureChange {
            name: "process".to_string(),
            location: ChangeLocation::new("src/lib.rs"),
            old_signature: Some("fn process()".to_string()),
            new_signature: "fn process(opts: Options)".to_string(),
            description: "Added options parameter".to_string(),
            severity: Severity::Major,
        });

        report.compute_summary();

        assert_eq!(report.summary.total_count, 2);
        assert_eq!(report.summary.package_count, 1);
        assert_eq!(report.summary.library_count, 1);
        assert_eq!(report.summary.max_severity, Severity::Major);
        assert_eq!(report.suggested_bump(), SemverBump::Major);
        assert!(report.has_changes());
        assert!(report.has_severe_changes());
    }

    #[test]
    fn test_boxed_behavior_change() {
        // Verify the boxed BehaviorChange variant works correctly
        let change = LibraryBreakingChange::BehaviorChange(Box::new(BehaviorChangeData {
            name: "process".to_string(),
            location: ChangeLocation::new("src/lib.rs"),
            old_behavior: Some("Returns null on error".to_string()),
            new_behavior: "Throws exception on error".to_string(),
            affects_error_handling: true,
            affects_return_values: true,
            affects_side_effects: false,
            description: "Changed error handling".to_string(),
            severity: Severity::Critical,
        }));

        // Verify trait methods work through the box
        assert_eq!(change.severity(), &Severity::Critical);
        assert_eq!(change.description(), "Changed error handling");
        assert_eq!(change.category(), "behavior");
        assert_eq!(change.name(), "process");
        assert!(change.location().is_some());
        assert!(change.is_severe());
        assert_eq!(change.semver_bump(), SemverBump::Major);
    }

    #[test]
    fn test_behavior_change_serialization() {
        let change = LibraryBreakingChange::BehaviorChange(Box::new(BehaviorChangeData {
            name: "fetch".to_string(),
            location: ChangeLocation::with_line("src/api.rs", 100),
            old_behavior: None,
            new_behavior: "Now async".to_string(),
            affects_error_handling: false,
            affects_return_values: true,
            affects_side_effects: false,
            description: "Made async".to_string(),
            severity: Severity::Major,
        }));

        let serialized = serde_json::to_string(&change).unwrap();
        let deserialized: LibraryBreakingChange = serde_json::from_str(&serialized).unwrap();
        assert_eq!(change, deserialized);
    }

    // ========================================================================
    // New Breaking Change Variant Tests
    // ========================================================================

    #[test]
    fn test_asset_removed_variant() {
        let change = PackageBreakingChange::AssetRemoved {
            asset_path: "assets/legacy-icon.png".to_string(),
            asset_type: Some("image".to_string()),
            description: "Removed deprecated icon asset".to_string(),
            severity: Severity::Major,
            replacement_hint: Some("Use assets/new-icon.svg instead".to_string()),
        };

        assert_eq!(change.severity(), &Severity::Major);
        assert_eq!(change.description(), "Removed deprecated icon asset");
        assert_eq!(change.category(), "asset_removed");
        assert_eq!(change.asset_path(), "assets/legacy-icon.png");
        assert_eq!(
            change.migration_hint(),
            Some("Use assets/new-icon.svg instead")
        );

        // Test serialization
        let serialized = serde_json::to_string(&change).unwrap();
        let deserialized: PackageBreakingChange = serde_json::from_str(&serialized).unwrap();
        assert_eq!(change, deserialized);
    }

    #[test]
    fn test_asset_schema_change_variant() {
        let change = PackageBreakingChange::AssetSchemaChange {
            asset_path: "config/settings.json".to_string(),
            added_fields: vec!["newFeature".to_string(), "enableCache".to_string()],
            removed_fields: vec!["oldSetting".to_string()],
            changed_fields: vec!["timeout".to_string()],
            description: "Configuration schema updated".to_string(),
            severity: Severity::Moderate,
        };

        assert_eq!(change.severity(), &Severity::Moderate);
        assert_eq!(change.category(), "asset_schema");
        assert_eq!(change.asset_path(), "config/settings.json");
        assert!(change.migration_hint().is_none());

        // Test serialization
        let serialized = serde_json::to_string(&change).unwrap();
        let deserialized: PackageBreakingChange = serde_json::from_str(&serialized).unwrap();
        assert_eq!(change, deserialized);
    }

    #[test]
    fn test_asset_encoding_change_variant() {
        let change = PackageBreakingChange::AssetEncodingChange {
            asset_path: "data/translations.json".to_string(),
            old_encoding: Some("UTF-8".to_string()),
            new_encoding: "UTF-16".to_string(),
            description: "Changed encoding to UTF-16".to_string(),
            severity: Severity::Minor,
        };

        assert_eq!(change.severity(), &Severity::Minor);
        assert_eq!(change.category(), "asset_encoding");
        assert_eq!(change.asset_path(), "data/translations.json");

        // Test serialization
        let serialized = serde_json::to_string(&change).unwrap();
        let deserialized: PackageBreakingChange = serde_json::from_str(&serialized).unwrap();
        assert_eq!(change, deserialized);
    }

    #[test]
    fn test_export_removed_variant() {
        let change = LibraryBreakingChange::ExportRemoved {
            name: "oldHelper".to_string(),
            module_path: "src/utils/helpers.ts".to_string(),
            export_kind: "function".to_string(),
            description: "Removed deprecated helper function".to_string(),
            severity: Severity::Major,
            replacement: Some("Use newHelper from @mylib/utils".to_string()),
        };

        assert_eq!(change.severity(), &Severity::Major);
        assert_eq!(change.description(), "Removed deprecated helper function");
        assert_eq!(change.category(), "export_removed");
        assert_eq!(change.name(), "oldHelper");
        assert!(change.location().is_none());
        assert_eq!(
            change.migration_hint(),
            Some("Use newHelper from @mylib/utils")
        );

        // Test serialization
        let serialized = serde_json::to_string(&change).unwrap();
        let deserialized: LibraryBreakingChange = serde_json::from_str(&serialized).unwrap();
        assert_eq!(change, deserialized);
    }

    #[test]
    fn test_visibility_change_variant() {
        let change = LibraryBreakingChange::VisibilityChange {
            name: "internalMethod".to_string(),
            location: ChangeLocation::with_line("src/core/engine.ts", 150),
            old_visibility: "public".to_string(),
            new_visibility: "private".to_string(),
            description: "Made method private".to_string(),
            severity: Severity::Major,
        };

        assert_eq!(change.severity(), &Severity::Major);
        assert_eq!(change.category(), "visibility_change");
        assert_eq!(change.name(), "internalMethod");
        assert!(change.location().is_some());

        // Test serialization
        let serialized = serde_json::to_string(&change).unwrap();
        let deserialized: LibraryBreakingChange = serde_json::from_str(&serialized).unwrap();
        assert_eq!(change, deserialized);
    }

    #[test]
    fn test_module_structure_change_variant() {
        let change = LibraryBreakingChange::ModuleStructureChange {
            old_path: "src/utils/index.ts".to_string(),
            new_path: Some("src/helpers/index.ts".to_string()),
            affected_exports: vec!["formatDate".to_string(), "parseDate".to_string()],
            description: "Moved utility functions to helpers module".to_string(),
            severity: Severity::Moderate,
        };

        assert_eq!(change.severity(), &Severity::Moderate);
        assert_eq!(change.category(), "module_structure");
        assert_eq!(change.name(), "src/utils/index.ts");
        assert!(change.location().is_none());

        // Test serialization
        let serialized = serde_json::to_string(&change).unwrap();
        let deserialized: LibraryBreakingChange = serde_json::from_str(&serialized).unwrap();
        assert_eq!(change, deserialized);
    }

    #[test]
    fn test_generic_constraint_change_variant() {
        let change = LibraryBreakingChange::GenericConstraintChange {
            name: "Container".to_string(),
            location: ChangeLocation::with_line("src/types.ts", 25),
            type_parameter: "T".to_string(),
            old_constraint: None,
            new_constraint: Some("extends Serializable".to_string()),
            description: "Added Serializable constraint to Container<T>".to_string(),
            severity: Severity::Major,
        };

        assert_eq!(change.severity(), &Severity::Major);
        assert_eq!(change.category(), "generic_constraint");
        assert_eq!(change.name(), "Container");
        assert!(change.location().is_some());

        // Test serialization
        let serialized = serde_json::to_string(&change).unwrap();
        let deserialized: LibraryBreakingChange = serde_json::from_str(&serialized).unwrap();
        assert_eq!(change, deserialized);
    }

    #[test]
    fn test_default_value_change_variant() {
        let change = LibraryBreakingChange::DefaultValueChange {
            name: "createConfig".to_string(),
            location: ChangeLocation::with_line("src/config.ts", 42),
            parameter_name: "timeout".to_string(),
            old_default: Some("30000".to_string()),
            new_default: Some("5000".to_string()),
            description: "Reduced default timeout from 30s to 5s".to_string(),
            severity: Severity::Moderate,
        };

        assert_eq!(change.severity(), &Severity::Moderate);
        assert_eq!(change.category(), "default_value");
        assert_eq!(change.name(), "createConfig");
        assert!(change.location().is_some());

        // Test serialization
        let serialized = serde_json::to_string(&change).unwrap();
        let deserialized: LibraryBreakingChange = serde_json::from_str(&serialized).unwrap();
        assert_eq!(change, deserialized);
    }
}
