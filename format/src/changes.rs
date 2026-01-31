//! Breaking change representation for package and code changes.
//!
//! This module provides data structures for representing breaking changes
//! detected between two versions of a package. Changes are categorized into:
//!
//! - **Asset Changes**: Changes to package assets (files, formats, metadata)
//! - **Code Changes**: Changes to code signatures (APIs, types, behavior)
//!
//! # Architecture
//!
//! The design uses a two-tier enum hierarchy:
//!
//! ```text
//! BreakingChange (top-level)
//! ├── Asset(AssetChange)
//! │   ├── FormatChanged      - File format changed (e.g., JSON to YAML)
//! │   ├── DependencyChanged  - Asset dependency modified
//! │   ├── PathChanged        - Asset path/name changed
//! │   ├── MetadataChanged    - Asset metadata changed
//! │   ├── Removed            - Asset file removed
//! │   ├── Added              - New asset added (informational)
//! │   ├── SchemaChanged      - JSON/XML schema structure changed
//! │   └── EncodingChanged    - File encoding changed
//! └── Code(CodeChange)
//!     ├── SignatureChanged           - API signature changed
//!     ├── TypeChanged                - Type definition changed
//!     ├── BehaviorChanged            - Runtime behavior changed
//!     ├── DependencyVersionChanged   - Dependency version changed
//!     ├── ExportRemoved              - Export removed from public API
//!     ├── ExportAdded                - New export added (informational)
//!     ├── VisibilityChanged          - Visibility level changed
//!     ├── ModuleChanged              - Module structure changed
//!     ├── GenericConstraintChanged   - Generic constraints changed
//!     └── DefaultValueChanged        - Default value changed
//! ```
//!
//! All change types implement the [`Change`] trait for common functionality,
//! including `severity()`, `description()`, `location()`, `kind()`, and
//! `migration_hint()`.
//!
//! # Key Types
//!
//! - [`ChangeReport`]: Aggregate type for collecting changes with statistics
//! - [`ChangeSummary`]: Statistics about detected changes
//! - [`Change`]: Common trait with `migration_hint()` support
//! - [`Severity`]: Semver-aligned severity levels (Info, Patch, Minor, Major)
//!
//! # Example: Basic Usage
//!
//! ```rust
//! use interlint_format::changes::{
//!     BreakingChange, AssetChange, CodeChange, Severity,
//!     AssetFormatChange, SignatureChange, ChangeKind, Change,
//! };
//!
//! // Asset change: file format changed
//! let asset_change = BreakingChange::Asset(AssetChange::FormatChanged(AssetFormatChange {
//!     path: "config/settings.json".to_string(),
//!     old_format: "json".to_string(),
//!     new_format: "yaml".to_string(),
//!     migration_hint: Some("Convert JSON to YAML format".to_string()),
//! }));
//!
//! assert_eq!(asset_change.severity(), Severity::Major);
//! assert_eq!(asset_change.kind(), ChangeKind::Asset);
//! assert!(asset_change.migration_hint().is_some());
//!
//! // Code change: function signature changed
//! let code_change = BreakingChange::Code(CodeChange::SignatureChanged(SignatureChange {
//!     name: "processData".to_string(),
//!     module_path: "src/utils.ts".to_string(),
//!     old_signature: "processData(data: string): void".to_string(),
//!     new_signature: "processData(data: Buffer, options?: Options): Promise<void>".to_string(),
//!     change_details: vec![
//!         "Parameter type changed from string to Buffer".to_string(),
//!         "Return type changed from void to Promise<void>".to_string(),
//!     ],
//!     parameter_changes: vec![],
//!     return_type_change: None,
//!     migration_hint: None,
//! }));
//!
//! assert_eq!(code_change.severity(), Severity::Major);
//! assert_eq!(code_change.kind(), ChangeKind::Code);
//! ```
//!
//! # Example: Using ChangeReport
//!
//! ```rust
//! use interlint_format::changes::{
//!     ChangeReport, BreakingChange, AssetChange, CodeChange,
//!     AssetFormatChange, ExportRemovedChange, Change, Severity,
//! };
//!
//! // Create a change report
//! let mut report = ChangeReport::new("my-package", "1.0.0", "2.0.0");
//!
//! // Add an asset format change
//! report.add_change(BreakingChange::Asset(AssetChange::FormatChanged(
//!     AssetFormatChange::new("config.json", "json", "yaml")
//!         .with_hint("Convert using: yq -p json config.json > config.yaml")
//! )));
//!
//! // Add an export removal
//! report.add_change(BreakingChange::Code(CodeChange::ExportRemoved(
//!     ExportRemovedChange {
//!         name: "deprecatedFn".to_string(),
//!         module_path: "src/index.ts".to_string(),
//!         export_type: Some("function".to_string()),
//!         replacement: Some("newFn".to_string()),
//!         migration_hint: Some("Replace deprecatedFn() with newFn()".to_string()),
//!     }
//! )));
//!
//! // Analyze the report
//! assert!(report.has_breaking_changes());
//! assert_eq!(report.recommended_bump(), "major");
//! assert_eq!(report.summary.total_count, 2);
//! assert_eq!(report.summary.asset_count, 1);
//! assert_eq!(report.summary.code_count, 1);
//! assert_eq!(report.summary.major_count, 2);
//!
//! // Iterate over changes by type
//! for asset in report.asset_changes() {
//!     println!("Asset change: {}", asset.description());
//! }
//! for code in report.code_changes() {
//!     println!("Code change: {}", code.description());
//! }
//!
//! // Filter by severity
//! for change in report.changes_by_severity(Severity::Major) {
//!     if let Some(hint) = change.migration_hint() {
//!         println!("Migration hint: {}", hint);
//!     }
//! }
//! ```
//!
//! # See Also
//!
//! - [`crate::breaking_changes`] - Legacy API with builder patterns
//! - [BREAKING_CHANGES.md](../docs/BREAKING_CHANGES.md) - Full documentation

use serde::{Deserialize, Serialize};

use crate::types::TypeInfo;

// =============================================================================
// Module System Breaking Changes (package.json exports/imports)
// =============================================================================

/// Module system breaking changes related to package.json exports/imports.
///
/// These changes affect how consumers can import from the package, which
/// subpaths are available, and under what conditions (node/browser/import/require).
///
/// # Examples
///
/// ```rust
/// use interlint_format::changes::ModuleSystemChange;
///
/// // Subpath removal is critical - consumers will get import errors
/// let change = ModuleSystemChange::ExportsSubpathRemoved {
///     subpath: "./utils".to_string(),
///     conditions_affected: vec!["import".to_string(), "require".to_string()],
///     migration_hint: Some("Use ./helpers instead".to_string()),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModuleSystemChange {
    /// An exports subpath was removed entirely.
    ///
    /// This is always a critical breaking change as consumers importing
    /// from this subpath will get module resolution errors.
    ExportsSubpathRemoved {
        /// The subpath that was removed (e.g., "./utils", ".")
        subpath: String,
        /// Conditions that were affected (e.g., ["import", "require"])
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        conditions_affected: Vec<String>,
        /// Optional migration hint
        #[serde(skip_serializing_if = "Option::is_none")]
        migration_hint: Option<String>,
    },

    /// An exports subpath was renamed to a different path.
    ///
    /// Critical breaking change - old import paths will fail.
    ExportsSubpathRenamed {
        /// The old subpath
        old_subpath: String,
        /// The new subpath
        new_subpath: String,
        /// Optional migration hint
        #[serde(skip_serializing_if = "Option::is_none")]
        migration_hint: Option<String>,
    },

    /// A condition was removed from an exports subpath.
    ///
    /// For example, removing "require" means CommonJS consumers can no longer
    /// import from this subpath.
    ExportsConditionRemoved {
        /// The affected subpath
        subpath: String,
        /// The condition that was removed (e.g., "require", "node", "browser")
        condition: String,
        /// Optional migration hint
        #[serde(skip_serializing_if = "Option::is_none")]
        migration_hint: Option<String>,
    },

    /// The order of conditions changed for an exports subpath.
    ///
    /// Condition order matters in Node.js - first matching condition wins.
    /// Reordering can change which file is resolved.
    ExportsConditionOrderChanged {
        /// The affected subpath
        subpath: String,
        /// Previous condition order
        old_order: Vec<String>,
        /// New condition order
        new_order: Vec<String>,
        /// Optional migration hint
        #[serde(skip_serializing_if = "Option::is_none")]
        migration_hint: Option<String>,
    },

    /// A subpath was explicitly set to null (blocked).
    ///
    /// Setting an export to null explicitly prevents import from that path.
    ExportsNullAdded {
        /// The subpath that was nulled
        subpath: String,
        /// The condition under which it was nulled (if conditional)
        #[serde(skip_serializing_if = "Option::is_none")]
        condition: Option<String>,
        /// Optional migration hint
        #[serde(skip_serializing_if = "Option::is_none")]
        migration_hint: Option<String>,
    },

    /// The browser field structure changed.
    ///
    /// The browser field can be a string, object, or boolean, and changing
    /// between these affects bundler behavior.
    BrowserFieldStructureChanged {
        /// Previous structure type ("string", "object", "boolean", "absent")
        old_type: String,
        /// New structure type
        new_type: String,
        /// Optional migration hint
        #[serde(skip_serializing_if = "Option::is_none")]
        migration_hint: Option<String>,
    },

    /// The module type changed (ESM vs CommonJS).
    ///
    /// Changing "type" field affects how .js files are interpreted.
    ModuleTypeChanged {
        /// Previous type (None = CommonJS default)
        old_type: Option<String>,
        /// New type ("module" for ESM, "commonjs" for CJS)
        new_type: Option<String>,
        /// Optional migration hint
        #[serde(skip_serializing_if = "Option::is_none")]
        migration_hint: Option<String>,
    },

    /// A package entry point changed.
    ///
    /// Changes to main, module, types, or other entry point fields.
    EntryPointChanged {
        /// The field that changed (e.g., "main", "module", "types")
        field: String,
        /// Previous path (None if not present)
        old_path: Option<String>,
        /// New path (None if removed)
        new_path: Option<String>,
        /// Optional migration hint
        #[serde(skip_serializing_if = "Option::is_none")]
        migration_hint: Option<String>,
    },

    /// Import maps or imports field changed.
    ImportsChanged {
        /// The import specifier that changed (e.g., "#utils")
        specifier: String,
        /// The type of change
        change_type: ImportsChangeType,
        /// Previous resolution target
        #[serde(skip_serializing_if = "Option::is_none")]
        old_target: Option<String>,
        /// New resolution target
        #[serde(skip_serializing_if = "Option::is_none")]
        new_target: Option<String>,
    },
}

/// Type of change in the imports field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportsChangeType {
    /// Import specifier was added
    Added,
    /// Import specifier was removed
    Removed,
    /// Import specifier target changed
    TargetChanged,
}

impl ModuleSystemChange {
    /// Get the severity of this module system change.
    pub fn severity(&self) -> Severity {
        match self {
            // Critical - will break imports
            ModuleSystemChange::ExportsSubpathRemoved { .. }
            | ModuleSystemChange::ExportsSubpathRenamed { .. }
            | ModuleSystemChange::ExportsNullAdded { .. } => Severity::Major,

            // Major - can break specific environments
            ModuleSystemChange::ExportsConditionRemoved { .. }
            | ModuleSystemChange::ModuleTypeChanged { .. } => Severity::Major,

            // Minor - behavior may differ
            ModuleSystemChange::ExportsConditionOrderChanged { .. }
            | ModuleSystemChange::BrowserFieldStructureChanged { .. }
            | ModuleSystemChange::EntryPointChanged { .. } => Severity::Minor,

            // Imports changes vary
            ModuleSystemChange::ImportsChanged { change_type, .. } => match change_type {
                ImportsChangeType::Removed => Severity::Major,
                ImportsChangeType::TargetChanged => Severity::Minor,
                ImportsChangeType::Added => Severity::Info,
            },
        }
    }

    /// Get a description of this change.
    pub fn description(&self) -> String {
        match self {
            ModuleSystemChange::ExportsSubpathRemoved { subpath, .. } => {
                format!("Exports subpath '{}' was removed", subpath)
            }
            ModuleSystemChange::ExportsSubpathRenamed {
                old_subpath,
                new_subpath,
                ..
            } => {
                format!(
                    "Exports subpath '{}' was renamed to '{}'",
                    old_subpath, new_subpath
                )
            }
            ModuleSystemChange::ExportsConditionRemoved {
                subpath, condition, ..
            } => {
                format!(
                    "Condition '{}' was removed from exports subpath '{}'",
                    condition, subpath
                )
            }
            ModuleSystemChange::ExportsConditionOrderChanged { subpath, .. } => {
                format!("Condition order changed for exports subpath '{}'", subpath)
            }
            ModuleSystemChange::ExportsNullAdded { subpath, .. } => {
                format!("Exports subpath '{}' was explicitly blocked (set to null)", subpath)
            }
            ModuleSystemChange::BrowserFieldStructureChanged {
                old_type, new_type, ..
            } => {
                format!("Browser field structure changed from {} to {}", old_type, new_type)
            }
            ModuleSystemChange::ModuleTypeChanged { old_type, new_type, .. } => {
                format!(
                    "Module type changed from {} to {}",
                    old_type.as_deref().unwrap_or("commonjs"),
                    new_type.as_deref().unwrap_or("commonjs")
                )
            }
            ModuleSystemChange::EntryPointChanged { field, .. } => {
                format!("Entry point '{}' changed", field)
            }
            ModuleSystemChange::ImportsChanged { specifier, change_type, .. } => {
                match change_type {
                    ImportsChangeType::Added => format!("Import '{}' was added", specifier),
                    ImportsChangeType::Removed => format!("Import '{}' was removed", specifier),
                    ImportsChangeType::TargetChanged => {
                        format!("Import '{}' target changed", specifier)
                    }
                }
            }
        }
    }
}

// =============================================================================
// Type Change Impact - Who is affected by the change
// =============================================================================

/// Indicates who is affected by a type change.
///
/// Some type changes affect different groups differently:
///
/// - **ConsumerBreaking**: Breaks code that *uses* the API (calls functions,
///   reads properties, etc.)
/// - **ImplementerBreaking**: Breaks code that *implements* interfaces or
///   extends classes
/// - **Both**: Breaks both consumers and implementers
///
/// # Example
///
/// Adding an optional property to an interface is:
/// - Safe for consumers (they don't need to provide it)
/// - Breaking for implementers (they must add the property)
///
/// Removing a method from a class is:
/// - Breaking for consumers (they can't call it anymore)
/// - Safe for subclasses (they don't need to override it)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeChangeImpact {
    /// Breaks code that uses/consumes the API.
    ///
    /// Examples:
    /// - Removing a method (consumers can't call it)
    /// - Changing return type (consumers expect different type)
    /// - Adding required parameter (consumers must update calls)
    ConsumerBreaking,

    /// Breaks code that implements interfaces or extends classes.
    ///
    /// Examples:
    /// - Adding required method to interface (implementers must add it)
    /// - Adding optional property (implementers may need to handle it)
    /// - Tightening generic constraint (implementations may not satisfy)
    ImplementerBreaking,

    /// Breaks both consumers and implementers.
    ///
    /// Examples:
    /// - Changing property type (affects both readers and providers)
    /// - Removing required property (consumers can't read, implementers can't omit)
    Both,
}

impl TypeChangeImpact {
    /// Returns true if this change affects API consumers.
    pub fn affects_consumers(&self) -> bool {
        matches!(self, TypeChangeImpact::ConsumerBreaking | TypeChangeImpact::Both)
    }

    /// Returns true if this change affects implementers.
    pub fn affects_implementers(&self) -> bool {
        matches!(self, TypeChangeImpact::ImplementerBreaking | TypeChangeImpact::Both)
    }

    /// Combine two impacts into the broader one.
    pub fn combine(self, other: TypeChangeImpact) -> TypeChangeImpact {
        match (self, other) {
            (TypeChangeImpact::Both, _) | (_, TypeChangeImpact::Both) => TypeChangeImpact::Both,
            (TypeChangeImpact::ConsumerBreaking, TypeChangeImpact::ImplementerBreaking)
            | (TypeChangeImpact::ImplementerBreaking, TypeChangeImpact::ConsumerBreaking) => {
                TypeChangeImpact::Both
            }
            (TypeChangeImpact::ConsumerBreaking, _) => TypeChangeImpact::ConsumerBreaking,
            (TypeChangeImpact::ImplementerBreaking, _) => TypeChangeImpact::ImplementerBreaking,
        }
    }
}

// =============================================================================
// Severity and Classification
// =============================================================================

/// Severity level of a breaking change.
///
/// Based on semantic versioning principles:
/// - **Major**: Incompatible API changes (requires major version bump)
/// - **Minor**: Backwards-compatible additions (requires minor version bump)
/// - **Patch**: Backwards-compatible fixes (requires patch version bump)
/// - **Info**: Informational, no version bump required
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Informational change, no impact on consumers
    Info,
    /// Patch-level change, backwards compatible fix
    Patch,
    /// Minor change, backwards compatible addition
    Minor,
    /// Major breaking change, incompatible modification
    Major,
}

impl Default for Severity {
    fn default() -> Self {
        Self::Major
    }
}

impl Severity {
    /// Returns true if this severity represents a breaking change.
    pub fn is_breaking(&self) -> bool {
        matches!(self, Self::Major)
    }

    /// Returns the semver component that should be bumped.
    pub fn semver_bump(&self) -> &'static str {
        match self {
            Self::Info => "none",
            Self::Patch => "patch",
            Self::Minor => "minor",
            Self::Major => "major",
        }
    }
}

/// The kind of change (asset or code).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Change to a package asset (file, format, metadata)
    Asset,
    /// Change to code (signature, type, behavior)
    Code,
}

// =============================================================================
// Change Trait - Common Interface
// =============================================================================

/// Common interface for all breaking changes.
///
/// This trait provides a unified way to access change information
/// regardless of whether it's an asset or code change.
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
    fn migration_hint(&self) -> Option<&str> {
        None
    }
}

// =============================================================================
// Top-Level Breaking Change Enum
// =============================================================================

/// A breaking change detected between two versions.
///
/// This is the top-level enum that categorizes changes into asset
/// changes and code changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum BreakingChange {
    /// Change to a package asset
    Asset(AssetChange),
    /// Change to code/API
    Code(CodeChange),
}

impl Change for BreakingChange {
    fn severity(&self) -> Severity {
        match self {
            Self::Asset(c) => c.severity(),
            Self::Code(c) => c.severity(),
        }
    }

    fn description(&self) -> String {
        match self {
            Self::Asset(c) => c.description(),
            Self::Code(c) => c.description(),
        }
    }

    fn location(&self) -> &str {
        match self {
            Self::Asset(c) => c.location(),
            Self::Code(c) => c.location(),
        }
    }

    fn kind(&self) -> ChangeKind {
        match self {
            Self::Asset(_) => ChangeKind::Asset,
            Self::Code(_) => ChangeKind::Code,
        }
    }

    fn migration_hint(&self) -> Option<&str> {
        match self {
            Self::Asset(c) => c.migration_hint(),
            Self::Code(c) => c.migration_hint(),
        }
    }
}

impl BreakingChange {
    /// Returns true if this is an asset change.
    pub fn is_asset(&self) -> bool {
        matches!(self, Self::Asset(_))
    }

    /// Returns true if this is a code change.
    pub fn is_code(&self) -> bool {
        matches!(self, Self::Code(_))
    }

    /// Returns the inner asset change if this is an asset change.
    pub fn as_asset(&self) -> Option<&AssetChange> {
        match self {
            Self::Asset(c) => Some(c),
            Self::Code(_) => None,
        }
    }

    /// Returns the inner code change if this is a code change.
    pub fn as_code(&self) -> Option<&CodeChange> {
        match self {
            Self::Code(c) => Some(c),
            Self::Asset(_) => None,
        }
    }
}

// =============================================================================
// Asset Changes - Package/Asset Breaking Changes
// =============================================================================

/// Changes to package assets (non-code files).
///
/// Asset changes represent modifications to files that are part of
/// the package distribution but are not code. These include:
/// - Configuration files
/// - Static resources (images, fonts, etc.)
/// - Data files (JSON, YAML, etc.)
/// - Build artifacts
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssetChange {
    /// Asset file format changed (e.g., JSON to YAML)
    FormatChanged(AssetFormatChange),
    /// Asset dependency changed (external reference modified)
    DependencyChanged(AssetDependencyChange),
    /// Asset path/name changed
    PathChanged(AssetPathChange),
    /// Asset metadata changed
    MetadataChanged(AssetMetadataChange),
    /// Asset was removed
    Removed(AssetRemovedChange),
    /// Asset was added (informational, typically not breaking)
    Added(AssetAddedChange),
    /// Asset content schema changed
    SchemaChanged(AssetSchemaChange),
    /// Asset encoding changed (e.g., UTF-8 to UTF-16)
    EncodingChanged(AssetEncodingChange),
}

impl Change for AssetChange {
    fn severity(&self) -> Severity {
        match self {
            Self::FormatChanged(_) => Severity::Major,
            Self::DependencyChanged(c) => c.severity,
            Self::PathChanged(c) => c.severity,
            Self::MetadataChanged(c) => c.severity,
            Self::Removed(_) => Severity::Major,
            Self::Added(_) => Severity::Minor,
            Self::SchemaChanged(c) => c.severity,
            Self::EncodingChanged(_) => Severity::Major,
        }
    }

    fn description(&self) -> String {
        match self {
            Self::FormatChanged(c) => {
                format!(
                    "Asset format changed from '{}' to '{}' at '{}'",
                    c.old_format, c.new_format, c.path
                )
            }
            Self::DependencyChanged(c) => {
                format!(
                    "Asset dependency '{}' changed at '{}': {} -> {}",
                    c.dependency_name,
                    c.path,
                    c.old_reference.as_deref().unwrap_or("(none)"),
                    c.new_reference.as_deref().unwrap_or("(none)")
                )
            }
            Self::PathChanged(c) => {
                format!(
                    "Asset path changed from '{}' to '{}'",
                    c.old_path, c.new_path
                )
            }
            Self::MetadataChanged(c) => {
                format!(
                    "Asset metadata '{}' changed at '{}': {} -> {}",
                    c.key,
                    c.path,
                    c.old_value.as_deref().unwrap_or("(none)"),
                    c.new_value.as_deref().unwrap_or("(none)")
                )
            }
            Self::Removed(c) => format!("Asset removed: '{}'", c.path),
            Self::Added(c) => format!("Asset added: '{}'", c.path),
            Self::SchemaChanged(c) => {
                format!("Asset schema changed at '{}': {}", c.path, c.change_summary)
            }
            Self::EncodingChanged(c) => {
                format!(
                    "Asset encoding changed from '{}' to '{}' at '{}'",
                    c.old_encoding, c.new_encoding, c.path
                )
            }
        }
    }

    fn location(&self) -> &str {
        match self {
            Self::FormatChanged(c) => &c.path,
            Self::DependencyChanged(c) => &c.path,
            Self::PathChanged(c) => &c.old_path,
            Self::MetadataChanged(c) => &c.path,
            Self::Removed(c) => &c.path,
            Self::Added(c) => &c.path,
            Self::SchemaChanged(c) => &c.path,
            Self::EncodingChanged(c) => &c.path,
        }
    }

    fn kind(&self) -> ChangeKind {
        ChangeKind::Asset
    }

    fn migration_hint(&self) -> Option<&str> {
        match self {
            Self::FormatChanged(c) => c.migration_hint.as_deref(),
            Self::DependencyChanged(c) => c.migration_hint.as_deref(),
            Self::PathChanged(c) => c.migration_hint.as_deref(),
            Self::MetadataChanged(c) => c.migration_hint.as_deref(),
            Self::Removed(c) => c.migration_hint.as_deref(),
            Self::Added(_) => None,
            Self::SchemaChanged(c) => c.migration_hint.as_deref(),
            Self::EncodingChanged(c) => c.migration_hint.as_deref(),
        }
    }
}

/// Asset file format change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetFormatChange {
    /// Path to the asset
    pub path: String,
    /// Previous format (e.g., "json", "xml", "csv")
    pub old_format: String,
    /// New format
    pub new_format: String,
    /// Optional hint for migrating
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// Asset dependency change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetDependencyChange {
    /// Path to the asset
    pub path: String,
    /// Name of the dependency
    pub dependency_name: String,
    /// Previous reference (URL, path, version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_reference: Option<String>,
    /// New reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_reference: Option<String>,
    /// Severity of this change
    #[serde(default)]
    pub severity: Severity,
    /// Optional migration hint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// Asset path/name change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPathChange {
    /// Previous path
    pub old_path: String,
    /// New path
    pub new_path: String,
    /// Severity of this change
    #[serde(default)]
    pub severity: Severity,
    /// Optional migration hint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// Asset metadata change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMetadataChange {
    /// Path to the asset
    pub path: String,
    /// Metadata key that changed
    pub key: String,
    /// Previous value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    /// New value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    /// Severity of this change
    #[serde(default)]
    pub severity: Severity,
    /// Optional migration hint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// Asset removal - represents a deleted asset file.
///
/// This is always a major breaking change since consumers may depend
/// on the asset being present.
///
/// # Example
///
/// ```rust
/// use interlint_format::changes::{AssetChange, AssetRemovedChange};
///
/// let change = AssetChange::Removed(AssetRemovedChange {
///     path: "assets/legacy-theme.css".to_string(),
///     asset_type: Some("css".to_string()),
///     migration_hint: Some("Use assets/modern-theme.css instead".to_string()),
/// });
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRemovedChange {
    /// Path of the removed asset
    pub path: String,
    /// Type/format of the removed asset (e.g., "css", "json", "png")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_type: Option<String>,
    /// Optional migration hint for consumers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// Asset addition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetAddedChange {
    /// Path of the added asset
    pub path: String,
    /// Type/format of the added asset
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_type: Option<String>,
}

/// Asset schema change - represents changes to JSON/XML/YAML schema structure.
///
/// Schema changes can break consumers that parse the asset file and expect
/// a specific structure. Common cases include:
/// - Required fields added
/// - Fields removed
/// - Field types changed
/// - Validation constraints changed
///
/// # Example
///
/// ```rust
/// use interlint_format::changes::{
///     AssetChange, AssetSchemaChange, SchemaFieldChange, SchemaFieldChangeType, Severity
/// };
///
/// let change = AssetChange::SchemaChanged(AssetSchemaChange {
///     path: "config/schema.json".to_string(),
///     change_summary: "Added required 'apiVersion' field".to_string(),
///     field_changes: vec![
///         SchemaFieldChange {
///             field_path: "apiVersion".to_string(),
///             change_type: SchemaFieldChangeType::Added,
///             old_info: None,
///             new_info: Some("string (required)".to_string()),
///         },
///     ],
///     severity: Severity::Major,
///     migration_hint: Some("Add 'apiVersion: \"v2\"' to config files".to_string()),
/// });
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSchemaChange {
    /// Path to the asset file
    pub path: String,
    /// Human-readable summary of schema changes
    pub change_summary: String,
    /// Detailed list of individual field changes
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_changes: Vec<SchemaFieldChange>,
    /// Severity of this change (typically Major for breaking schema changes)
    #[serde(default)]
    pub severity: Severity,
    /// Optional migration hint for consumers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// A single field change within a schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaFieldChange {
    /// Field path (e.g., "user.address.city")
    pub field_path: String,
    /// Type of field change
    pub change_type: SchemaFieldChangeType,
    /// Previous type/value info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_info: Option<String>,
    /// New type/value info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_info: Option<String>,
}

/// Type of schema field change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaFieldChangeType {
    /// Field was added
    Added,
    /// Field was removed
    Removed,
    /// Field type changed
    TypeChanged,
    /// Field became required
    BecameRequired,
    /// Field became optional
    BecameOptional,
    /// Field constraints changed
    ConstraintsChanged,
}

/// Asset encoding change - represents file encoding changes.
///
/// Encoding changes can break consumers that read the file with a specific
/// encoding assumption. This is always a major breaking change.
///
/// Common encoding changes:
/// - UTF-8 to UTF-16
/// - ASCII to UTF-8
/// - Latin-1 to UTF-8
///
/// # Example
///
/// ```rust
/// use interlint_format::changes::{AssetChange, AssetEncodingChange};
///
/// let change = AssetChange::EncodingChanged(AssetEncodingChange {
///     path: "data/translations.json".to_string(),
///     old_encoding: "UTF-8".to_string(),
///     new_encoding: "UTF-16".to_string(),
///     migration_hint: Some("Re-save files as UTF-16 or use iconv".to_string()),
/// });
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetEncodingChange {
    /// Path to the asset file
    pub path: String,
    /// Previous file encoding (e.g., "UTF-8", "ASCII", "Latin-1")
    pub old_encoding: String,
    /// New file encoding
    pub new_encoding: String,
    /// Optional migration hint for consumers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

// =============================================================================
// Code Changes - Library/Code Breaking Changes
// =============================================================================

/// Changes to code/API signatures.
///
/// Code changes represent modifications to the public interface of
/// a library or package. These include:
/// - Function/method signature changes
/// - Type definition changes
/// - Behavioral changes
/// - Dependency version changes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodeChange {
    /// API signature changed (parameters, return type, generics)
    SignatureChanged(SignatureChange),
    /// Type definition changed
    TypeChanged(TypeChange),
    /// Behavior changed (semantics, side effects)
    BehaviorChanged(BehaviorChange),
    /// Dependency version changed
    DependencyVersionChanged(DependencyVersionChange),
    /// Export was removed
    ExportRemoved(ExportRemovedChange),
    /// Export was added (informational)
    ExportAdded(ExportAddedChange),
    /// Visibility changed
    VisibilityChanged(VisibilityChange),
    /// Module structure changed
    ModuleChanged(ModuleChange),
    /// Generic constraints changed
    GenericConstraintChanged(GenericConstraintChange),
    /// Default value changed
    DefaultValueChanged(DefaultValueChange),
}

impl Change for CodeChange {
    fn severity(&self) -> Severity {
        match self {
            Self::SignatureChanged(_) => Severity::Major,
            Self::TypeChanged(c) => c.severity,
            Self::BehaviorChanged(c) => c.severity,
            Self::DependencyVersionChanged(c) => c.severity,
            Self::ExportRemoved(_) => Severity::Major,
            Self::ExportAdded(_) => Severity::Minor,
            Self::VisibilityChanged(c) => c.severity,
            Self::ModuleChanged(c) => c.severity,
            Self::GenericConstraintChanged(c) => c.severity,
            Self::DefaultValueChanged(c) => c.severity,
        }
    }

    fn description(&self) -> String {
        match self {
            Self::SignatureChanged(c) => {
                format!("Signature changed for '{}' in '{}'", c.name, c.module_path)
            }
            Self::TypeChanged(c) => {
                format!(
                    "Type '{}' changed in '{}': {}",
                    c.name, c.module_path, c.change_summary
                )
            }
            Self::BehaviorChanged(c) => {
                format!(
                    "Behavior changed for '{}' in '{}': {}",
                    c.name, c.module_path, c.change_summary
                )
            }
            Self::DependencyVersionChanged(c) => {
                format!(
                    "Dependency '{}' version changed: {} -> {}",
                    c.dependency_name, c.old_version, c.new_version
                )
            }
            Self::ExportRemoved(c) => {
                format!("Export '{}' removed from '{}'", c.name, c.module_path)
            }
            Self::ExportAdded(c) => {
                format!("Export '{}' added to '{}'", c.name, c.module_path)
            }
            Self::VisibilityChanged(c) => {
                format!(
                    "Visibility of '{}' changed from {:?} to {:?}",
                    c.name, c.old_visibility, c.new_visibility
                )
            }
            Self::ModuleChanged(c) => {
                format!("Module structure changed: {}", c.change_summary)
            }
            Self::GenericConstraintChanged(c) => {
                format!(
                    "Generic constraint changed for '{}' in '{}'",
                    c.name, c.module_path
                )
            }
            Self::DefaultValueChanged(c) => {
                format!(
                    "Default value changed for '{}' in '{}'",
                    c.name, c.module_path
                )
            }
        }
    }

    fn location(&self) -> &str {
        match self {
            Self::SignatureChanged(c) => &c.module_path,
            Self::TypeChanged(c) => &c.module_path,
            Self::BehaviorChanged(c) => &c.module_path,
            Self::DependencyVersionChanged(c) => &c.dependency_name,
            Self::ExportRemoved(c) => &c.module_path,
            Self::ExportAdded(c) => &c.module_path,
            Self::VisibilityChanged(c) => &c.module_path,
            Self::ModuleChanged(c) => &c.module_path,
            Self::GenericConstraintChanged(c) => &c.module_path,
            Self::DefaultValueChanged(c) => &c.module_path,
        }
    }

    fn kind(&self) -> ChangeKind {
        ChangeKind::Code
    }

    fn migration_hint(&self) -> Option<&str> {
        match self {
            Self::SignatureChanged(c) => c.migration_hint.as_deref(),
            Self::TypeChanged(c) => c.migration_hint.as_deref(),
            Self::BehaviorChanged(c) => c.migration_hint.as_deref(),
            Self::DependencyVersionChanged(c) => c.migration_hint.as_deref(),
            Self::ExportRemoved(c) => c.migration_hint.as_deref(),
            Self::ExportAdded(_) => None,
            Self::VisibilityChanged(c) => c.migration_hint.as_deref(),
            Self::ModuleChanged(c) => c.migration_hint.as_deref(),
            Self::GenericConstraintChanged(c) => c.migration_hint.as_deref(),
            Self::DefaultValueChanged(c) => c.migration_hint.as_deref(),
        }
    }
}

/// API signature change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureChange {
    /// Name of the function/method/class
    pub name: String,
    /// Module path where the signature exists
    pub module_path: String,
    /// String representation of old signature
    pub old_signature: String,
    /// String representation of new signature
    pub new_signature: String,
    /// Detailed list of changes
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub change_details: Vec<String>,
    /// Specific parameter changes
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameter_changes: Vec<ParameterChange>,
    /// Return type change details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type_change: Option<ReturnTypeChange>,
    /// Optional migration hint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// A parameter change within a signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParameterChange {
    /// Parameter name
    pub name: String,
    /// Type of change
    pub change_type: ParameterChangeType,
    /// Previous type info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_type: Option<TypeInfo>,
    /// New type info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_type: Option<TypeInfo>,
    /// Previous position (for reordering)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_position: Option<usize>,
    /// New position (for reordering)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_position: Option<usize>,
}

/// Type of parameter change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterChangeType {
    /// Parameter was added
    Added,
    /// Parameter was removed
    Removed,
    /// Parameter type changed
    TypeChanged,
    /// Parameter became required
    BecameRequired,
    /// Parameter became optional
    BecameOptional,
    /// Parameter was reordered
    Reordered,
    /// Parameter was renamed
    Renamed,
}

/// Return type change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReturnTypeChange {
    /// Previous return type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_type: Option<TypeInfo>,
    /// New return type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_type: Option<TypeInfo>,
    /// Whether the function became async
    #[serde(default)]
    pub became_async: bool,
    /// Whether the function was async and is no longer
    #[serde(default)]
    pub was_async: bool,
}

/// Type definition change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeChange {
    /// Name of the type
    pub name: String,
    /// Module path where the type exists
    pub module_path: String,
    /// Summary of the change
    pub change_summary: String,
    /// Previous type info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_type: Option<TypeInfo>,
    /// New type info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_type: Option<TypeInfo>,
    /// Property/field changes for object types
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub property_changes: Vec<PropertyChange>,
    /// Severity of this change
    #[serde(default)]
    pub severity: Severity,
    /// Who is affected by this change (consumers, implementers, or both)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<TypeChangeImpact>,
    /// Optional migration hint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// A property change within a type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyChange {
    /// Property name
    pub name: String,
    /// Type of change
    pub change_type: PropertyChangeType,
    /// Previous type info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_type: Option<TypeInfo>,
    /// New type info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_type: Option<TypeInfo>,
}

/// Type of property change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyChangeType {
    /// Property was added
    Added,
    /// Property was removed
    Removed,
    /// Property type changed
    TypeChanged,
    /// Property became required
    BecameRequired,
    /// Property became optional
    BecameOptional,
    /// Property became readonly
    BecameReadonly,
    /// Property became mutable
    BecameMutable,
}

/// Behavior change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorChange {
    /// Name of the affected function/method
    pub name: String,
    /// Module path
    pub module_path: String,
    /// Summary of behavioral change
    pub change_summary: String,
    /// Previous behavior description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_behavior: Option<String>,
    /// New behavior description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_behavior: Option<String>,
    /// Specific aspects that changed
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_aspects: Vec<BehaviorAspect>,
    /// Severity of this change
    #[serde(default)]
    pub severity: Severity,
    /// Optional migration hint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// Aspect of behavior that changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorAspect {
    /// Error handling changed
    ErrorHandling,
    /// Side effects changed
    SideEffects,
    /// Performance characteristics changed
    Performance,
    /// Concurrency behavior changed
    Concurrency,
    /// Ordering/determinism changed
    Ordering,
    /// Null/undefined handling changed
    NullHandling,
    /// Edge case handling changed
    EdgeCases,
    /// Default behavior changed
    Defaults,
}

/// Dependency version change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyVersionChange {
    /// Name of the dependency
    pub dependency_name: String,
    /// Previous version constraint
    pub old_version: String,
    /// New version constraint
    pub new_version: String,
    /// Whether this is a peer dependency
    #[serde(default)]
    pub is_peer: bool,
    /// Whether this is a dev dependency
    #[serde(default)]
    pub is_dev: bool,
    /// Severity of this change
    #[serde(default)]
    pub severity: Severity,
    /// Breaking changes in the dependency itself
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_breaking_changes: Vec<String>,
    /// Optional migration hint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// Export removal - represents a removed export from the public API.
///
/// This is always a major breaking change since consumers importing
/// the export will get errors.
///
/// # Example
///
/// ```rust
/// use interlint_format::changes::{CodeChange, ExportRemovedChange};
///
/// let change = CodeChange::ExportRemoved(ExportRemovedChange {
///     name: "deprecatedHelper".to_string(),
///     module_path: "src/utils.ts".to_string(),
///     export_type: Some("function".to_string()),
///     replacement: Some("newHelper".to_string()),
///     migration_hint: Some("Replace deprecatedHelper() with newHelper()".to_string()),
/// });
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportRemovedChange {
    /// Name of the removed export
    pub name: String,
    /// Module path where the export existed
    pub module_path: String,
    /// Type of export ("function", "class", "variable", "type", etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_type: Option<String>,
    /// Suggested replacement export name (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    /// Migration hint for consumers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// Export addition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportAddedChange {
    /// Name of the added export
    pub name: String,
    /// Module path
    pub module_path: String,
    /// Type of export (function, class, variable, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_type: Option<String>,
}

/// Visibility change - represents a change in item visibility level.
///
/// Making an item less visible (e.g., public to private) is typically
/// a breaking change. Making an item more visible is typically minor.
///
/// # Example
///
/// ```rust
/// use interlint_format::changes::{CodeChange, VisibilityChange, VisibilityLevel, Severity};
///
/// // Breaking: public to private
/// let change = CodeChange::VisibilityChanged(VisibilityChange {
///     name: "internalHelper".to_string(),
///     module_path: "src/utils.ts".to_string(),
///     old_visibility: VisibilityLevel::Public,
///     new_visibility: VisibilityLevel::Private,
///     severity: Severity::Major,
///     migration_hint: Some("Use publicHelper() instead".to_string()),
/// });
///
/// // Non-breaking: private to public
/// let change = CodeChange::VisibilityChanged(VisibilityChange {
///     name: "newPublicFn".to_string(),
///     module_path: "src/utils.ts".to_string(),
///     old_visibility: VisibilityLevel::Private,
///     new_visibility: VisibilityLevel::Public,
///     severity: Severity::Minor,
///     migration_hint: None,
/// });
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibilityChange {
    /// Name of the affected item
    pub name: String,
    /// Module path where the item exists
    pub module_path: String,
    /// Previous visibility level
    pub old_visibility: VisibilityLevel,
    /// New visibility level
    pub new_visibility: VisibilityLevel,
    /// Severity (Major for public->private, Minor for private->public)
    #[serde(default)]
    pub severity: Severity,
    /// Optional migration hint for consumers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// Visibility level (simplified from signature::Visibility for changes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityLevel {
    /// Public/exported
    Public,
    /// Protected
    Protected,
    /// Private/internal
    Private,
    /// Package/module internal
    Internal,
}

/// Module structure change - represents reorganization of module exports.
///
/// Module structure changes can break consumers if exports move to different
/// modules, requiring import statement updates.
///
/// # Example
///
/// ```rust
/// use interlint_format::changes::{CodeChange, ModuleChange, MovedExport, Severity};
///
/// let change = CodeChange::ModuleChanged(ModuleChange {
///     module_path: "src/index.ts".to_string(),
///     change_summary: "Split exports into submodules".to_string(),
///     old_structure: Some("All exports in index.ts".to_string()),
///     new_structure: Some("Exports split into utils.ts and core.ts".to_string()),
///     moved_exports: vec![
///         MovedExport {
///             name: "formatDate".to_string(),
///             old_module: "src/index.ts".to_string(),
///             new_module: "src/utils.ts".to_string(),
///         },
///         MovedExport {
///             name: "processData".to_string(),
///             old_module: "src/index.ts".to_string(),
///             new_module: "src/core.ts".to_string(),
///         },
///     ],
///     severity: Severity::Major,
///     migration_hint: Some("Update imports: 'pkg' -> 'pkg/utils' or 'pkg/core'".to_string()),
/// });
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleChange {
    /// Primary module path affected
    pub module_path: String,
    /// Human-readable summary of the structural change
    pub change_summary: String,
    /// Description of previous module structure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_structure: Option<String>,
    /// Description of new module structure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_structure: Option<String>,
    /// List of exports that moved to different modules
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moved_exports: Vec<MovedExport>,
    /// Severity of this change
    #[serde(default)]
    pub severity: Severity,
    /// Optional migration hint for consumers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// An export that moved to a different module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovedExport {
    /// Name of the export
    pub name: String,
    /// Previous module path
    pub old_module: String,
    /// New module path
    pub new_module: String,
}

/// Generic constraint change - represents changes to generic type bounds.
///
/// Adding or tightening constraints is typically breaking since existing
/// type arguments may no longer satisfy the constraints. Removing or
/// loosening constraints is typically non-breaking.
///
/// # Example
///
/// ```rust
/// use interlint_format::changes::{CodeChange, GenericConstraintChange, Severity};
/// use interlint_format::TypeInfo;
///
/// // Breaking: added constraint requirement
/// let change = CodeChange::GenericConstraintChanged(GenericConstraintChange {
///     name: "Container".to_string(),
///     module_path: "src/container.ts".to_string(),
///     type_parameter: "T".to_string(),
///     old_constraint: None, // was unconstrained
///     new_constraint: Some(TypeInfo::primitive("Serializable")),
///     severity: Severity::Major,
///     migration_hint: Some("Ensure T implements Serializable".to_string()),
/// });
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenericConstraintChange {
    /// Name of the generic type or function
    pub name: String,
    /// Module path where the generic is defined
    pub module_path: String,
    /// Name of the type parameter that changed (e.g., "T", "K", "V")
    pub type_parameter: String,
    /// Previous constraint (None if unconstrained)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_constraint: Option<TypeInfo>,
    /// New constraint (None if unconstrained)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_constraint: Option<TypeInfo>,
    /// Severity (Major for tighter constraints, Minor for looser)
    #[serde(default)]
    pub severity: Severity,
    /// Optional migration hint for consumers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

/// Default value change - represents changes to default parameter/property values.
///
/// While default value changes don't break compilation, they can cause
/// behavioral differences if consumers relied on the previous default.
/// Severity depends on how significant the behavioral change is.
///
/// # Example
///
/// ```rust
/// use interlint_format::changes::{CodeChange, DefaultValueChange, Severity};
///
/// // Potentially breaking: timeout default changed significantly
/// let change = CodeChange::DefaultValueChanged(DefaultValueChange {
///     name: "timeout".to_string(),
///     module_path: "src/config.ts".to_string(),
///     parent_name: Some("RequestOptions".to_string()),
///     old_default: Some("5000".to_string()),  // 5 seconds
///     new_default: Some("30000".to_string()), // 30 seconds
///     severity: Severity::Minor,
///     migration_hint: Some("Default timeout increased from 5s to 30s".to_string()),
/// });
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultValueChange {
    /// Name of the parameter or property with the default
    pub name: String,
    /// Module path where the default is defined
    pub module_path: String,
    /// Parent function or class name (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_name: Option<String>,
    /// Previous default value (as string representation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_default: Option<String>,
    /// New default value (as string representation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_default: Option<String>,
    /// Severity (depends on behavioral impact)
    #[serde(default)]
    pub severity: Severity,
    /// Optional migration hint for consumers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_hint: Option<String>,
}

// =============================================================================
// Change Report - Collection of Changes
// =============================================================================

/// A report containing all breaking changes between two versions.
///
/// `ChangeReport` is the primary aggregate type for collecting and analyzing
/// breaking changes. It automatically maintains statistics via [`ChangeSummary`]
/// and provides methods for querying and filtering changes.
///
/// # Example
///
/// ```rust
/// use interlint_format::changes::{
///     ChangeReport, BreakingChange, AssetChange, CodeChange,
///     AssetFormatChange, SignatureChange, Severity, Change,
/// };
///
/// // Create a report for a version bump
/// let mut report = ChangeReport::new("my-package", "1.0.0", "2.0.0");
///
/// // Add changes (summary is updated automatically)
/// report.add_change(BreakingChange::Asset(AssetChange::FormatChanged(
///     AssetFormatChange::new("config.json", "json", "yaml")
/// )));
///
/// report.add_change(BreakingChange::Code(CodeChange::SignatureChanged(
///     SignatureChange::new("process", "src/lib.ts", "old_sig", "new_sig")
/// )));
///
/// // Query the report
/// assert!(report.has_breaking_changes());
/// assert_eq!(report.recommended_bump(), "major");
/// assert_eq!(report.summary.total_count, 2);
///
/// // Filter changes
/// let major_changes: Vec<_> = report.changes_by_severity(Severity::Major).collect();
/// let asset_changes: Vec<_> = report.asset_changes().collect();
/// let code_changes: Vec<_> = report.code_changes().collect();
/// ```
///
/// # Serialization
///
/// `ChangeReport` serializes to JSON for storage or transmission:
///
/// ```rust
/// use interlint_format::changes::ChangeReport;
///
/// let report = ChangeReport::new("pkg", "1.0.0", "2.0.0");
/// let json = serde_json::to_string_pretty(&report).unwrap();
/// let decoded: ChangeReport = serde_json::from_str(&json).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeReport {
    /// Package name being analyzed
    pub package_name: String,
    /// Previous version (baseline)
    pub old_version: String,
    /// New version being compared
    pub new_version: String,
    /// All detected changes between versions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<BreakingChange>,
    /// Summary statistics (automatically updated via `add_change`)
    #[serde(default)]
    pub summary: ChangeSummary,
}

impl ChangeReport {
    /// Create a new change report.
    pub fn new(
        package_name: impl Into<String>,
        old_version: impl Into<String>,
        new_version: impl Into<String>,
    ) -> Self {
        Self {
            package_name: package_name.into(),
            old_version: old_version.into(),
            new_version: new_version.into(),
            changes: Vec::new(),
            summary: ChangeSummary::default(),
        }
    }

    /// Add a change to the report.
    pub fn add_change(&mut self, change: BreakingChange) {
        self.summary.update(&change);
        self.changes.push(change);
    }

    /// Get all asset changes.
    pub fn asset_changes(&self) -> impl Iterator<Item = &AssetChange> {
        self.changes.iter().filter_map(|c| c.as_asset())
    }

    /// Get all code changes.
    pub fn code_changes(&self) -> impl Iterator<Item = &CodeChange> {
        self.changes.iter().filter_map(|c| c.as_code())
    }

    /// Get changes by severity.
    pub fn changes_by_severity(&self, severity: Severity) -> impl Iterator<Item = &BreakingChange> {
        self.changes
            .iter()
            .filter(move |c| c.severity() == severity)
    }

    /// Check if there are any breaking changes.
    pub fn has_breaking_changes(&self) -> bool {
        self.summary.major_count > 0
    }

    /// Get the recommended version bump.
    pub fn recommended_bump(&self) -> &'static str {
        if self.summary.major_count > 0 {
            "major"
        } else if self.summary.minor_count > 0 {
            "minor"
        } else if self.summary.patch_count > 0 {
            "patch"
        } else {
            "none"
        }
    }
}

/// Summary statistics for a change report.
///
/// `ChangeSummary` provides aggregate statistics about changes in a
/// [`ChangeReport`]. It tracks counts by change kind (asset vs code)
/// and by severity level.
///
/// # Example
///
/// ```rust
/// use interlint_format::changes::{ChangeSummary, ChangeReport};
///
/// let report = ChangeReport::new("pkg", "1.0.0", "2.0.0");
/// // After adding changes...
/// let summary = report.summary;
///
/// println!("Total: {}", summary.total_count);
/// println!("Assets: {} / Code: {}", summary.asset_count, summary.code_count);
/// println!("Breaking (major): {}", summary.major_count);
/// println!("Additions (minor): {}", summary.minor_count);
/// println!("Fixes (patch): {}", summary.patch_count);
/// ```
///
/// # Semver Determination
///
/// The counts in `ChangeSummary` determine the recommended semver bump:
///
/// | Condition | Recommended Bump |
/// |-----------|------------------|
/// | `major_count > 0` | major |
/// | `minor_count > 0` | minor |
/// | `patch_count > 0` | patch |
/// | otherwise | none |
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSummary {
    /// Total number of changes detected
    pub total_count: usize,
    /// Number of asset changes (format, path, schema, etc.)
    pub asset_count: usize,
    /// Number of code changes (signatures, types, exports, etc.)
    pub code_count: usize,
    /// Number of major (breaking) changes - requires major version bump
    pub major_count: usize,
    /// Number of minor changes (additions) - requires minor version bump
    pub minor_count: usize,
    /// Number of patch changes (fixes) - requires patch version bump
    pub patch_count: usize,
    /// Number of info-level changes (no version bump needed)
    pub info_count: usize,
}

impl ChangeSummary {
    /// Update summary with a new change.
    pub fn update(&mut self, change: &BreakingChange) {
        self.total_count += 1;

        match change.kind() {
            ChangeKind::Asset => self.asset_count += 1,
            ChangeKind::Code => self.code_count += 1,
        }

        match change.severity() {
            Severity::Major => self.major_count += 1,
            Severity::Minor => self.minor_count += 1,
            Severity::Patch => self.patch_count += 1,
            Severity::Info => self.info_count += 1,
        }
    }
}

// =============================================================================
// Builder Helpers
// =============================================================================

impl AssetFormatChange {
    /// Create a new asset format change.
    pub fn new(
        path: impl Into<String>,
        old_format: impl Into<String>,
        new_format: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            old_format: old_format.into(),
            new_format: new_format.into(),
            migration_hint: None,
        }
    }

    /// Add a migration hint.
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.migration_hint = Some(hint.into());
        self
    }
}

impl SignatureChange {
    /// Create a new signature change.
    pub fn new(
        name: impl Into<String>,
        module_path: impl Into<String>,
        old_signature: impl Into<String>,
        new_signature: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            module_path: module_path.into(),
            old_signature: old_signature.into(),
            new_signature: new_signature.into(),
            change_details: Vec::new(),
            parameter_changes: Vec::new(),
            return_type_change: None,
            migration_hint: None,
        }
    }

    /// Add a change detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.change_details.push(detail.into());
        self
    }

    /// Add a migration hint.
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.migration_hint = Some(hint.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Info < Severity::Patch);
        assert!(Severity::Patch < Severity::Minor);
        assert!(Severity::Minor < Severity::Major);
    }

    #[test]
    fn test_severity_is_breaking() {
        assert!(!Severity::Info.is_breaking());
        assert!(!Severity::Patch.is_breaking());
        assert!(!Severity::Minor.is_breaking());
        assert!(Severity::Major.is_breaking());
    }

    #[test]
    fn test_asset_format_change() {
        let change = AssetFormatChange::new("config.json", "json", "yaml")
            .with_hint("Convert JSON to YAML format");

        let breaking = BreakingChange::Asset(AssetChange::FormatChanged(change));

        assert_eq!(breaking.severity(), Severity::Major);
        assert_eq!(breaking.kind(), ChangeKind::Asset);
        assert!(breaking.is_asset());
        assert!(!breaking.is_code());
    }

    #[test]
    fn test_signature_change() {
        let change = SignatureChange::new(
            "processData",
            "src/utils.ts",
            "processData(data: string): void",
            "processData(data: Buffer): Promise<void>",
        )
        .with_detail("Parameter type changed")
        .with_detail("Return type changed to Promise")
        .with_hint("Update call sites to handle Promise");

        let breaking = BreakingChange::Code(CodeChange::SignatureChanged(change));

        assert_eq!(breaking.severity(), Severity::Major);
        assert_eq!(breaking.kind(), ChangeKind::Code);
        assert!(breaking.is_code());
        assert!(!breaking.is_asset());
    }

    #[test]
    fn test_module_system_change() {
        let change = ModuleSystemChange::ExportsSubpathRemoved {
            subpath: "./utils".to_string(),
            conditions_affected: vec!["import".to_string()],
            migration_hint: Some("Use ./helpers instead".to_string()),
        };

        assert_eq!(change.severity(), Severity::Major);
        assert!(change.description().contains("./utils"));
    }

    #[test]
    fn test_type_change_impact() {
        assert!(TypeChangeImpact::ConsumerBreaking.affects_consumers());
        assert!(!TypeChangeImpact::ConsumerBreaking.affects_implementers());

        assert!(!TypeChangeImpact::ImplementerBreaking.affects_consumers());
        assert!(TypeChangeImpact::ImplementerBreaking.affects_implementers());

        assert!(TypeChangeImpact::Both.affects_consumers());
        assert!(TypeChangeImpact::Both.affects_implementers());

        // Test combine
        assert_eq!(
            TypeChangeImpact::ConsumerBreaking.combine(TypeChangeImpact::ImplementerBreaking),
            TypeChangeImpact::Both
        );
    }

    #[test]
    fn test_change_report() {
        let mut report = ChangeReport::new("my-package", "1.0.0", "2.0.0");

        report.add_change(BreakingChange::Asset(AssetChange::FormatChanged(
            AssetFormatChange::new("config.json", "json", "yaml"),
        )));

        report.add_change(BreakingChange::Code(CodeChange::SignatureChanged(
            SignatureChange::new("fn", "mod.ts", "old", "new"),
        )));

        report.add_change(BreakingChange::Code(CodeChange::ExportAdded(
            ExportAddedChange {
                name: "newFn".to_string(),
                module_path: "mod.ts".to_string(),
                export_type: Some("function".to_string()),
            },
        )));

        assert_eq!(report.summary.total_count, 3);
        assert_eq!(report.summary.asset_count, 1);
        assert_eq!(report.summary.code_count, 2);
        assert_eq!(report.summary.major_count, 2);
        assert_eq!(report.summary.minor_count, 1);
        assert!(report.has_breaking_changes());
        assert_eq!(report.recommended_bump(), "major");
    }

    #[test]
    fn test_change_serialization() {
        let change = BreakingChange::Asset(AssetChange::FormatChanged(AssetFormatChange {
            path: "config.json".to_string(),
            old_format: "json".to_string(),
            new_format: "yaml".to_string(),
            migration_hint: Some("Convert to YAML".to_string()),
        }));

        let json = serde_json::to_string_pretty(&change).unwrap();
        let decoded: BreakingChange = serde_json::from_str(&json).unwrap();

        assert_eq!(change, decoded);
    }

    #[test]
    fn test_code_change_serialization() {
        let change = BreakingChange::Code(CodeChange::TypeChanged(TypeChange {
            name: "Config".to_string(),
            module_path: "src/types.ts".to_string(),
            change_summary: "Added required field".to_string(),
            old_type: None,
            new_type: None,
            property_changes: vec![PropertyChange {
                name: "apiKey".to_string(),
                change_type: PropertyChangeType::Added,
                old_type: None,
                new_type: Some(TypeInfo::primitive("string")),
            }],
            severity: Severity::Major,
            impact: None,
            migration_hint: Some("Add apiKey field".to_string()),
        }));

        let json = serde_json::to_string_pretty(&change).unwrap();
        let decoded: BreakingChange = serde_json::from_str(&json).unwrap();

        assert_eq!(change, decoded);
    }
}
