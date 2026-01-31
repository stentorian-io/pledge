//! Package.json configuration fields for breaking change detection.
//!
//! This module provides data structures for representing package.json configuration
//! fields that are relevant to breaking change detection in Node.js/npm packages.
//!
//! # Overview
//!
//! The package.json file in Node.js projects contains several fields that, when
//! changed, can cause breaking changes for consumers:
//!
//! - **engines**: Node.js and npm version requirements
//! - **peerDependencies**: Required peer packages
//! - **exports**: Package entry point mappings
//! - **bin**: CLI command definitions
//! - **types**: TypeScript type definition paths
//!
//! # Breaking Change Categories
//!
//! Package.json field changes trigger different types of breaking changes:
//!
//! ## Major Breaking Changes
//!
//! - Raising minimum Node.js version in `engines`
//! - Removing or narrowing `peerDependencies` ranges
//! - Removing `exports` subpaths
//! - Removing `bin` commands
//!
//! ## Minor Changes
//!
//! - Adding new `exports` subpaths
//! - Adding new `bin` commands
//! - Lowering minimum engine requirements (widening compatibility)
//!
//! # Examples
//!
//! ## Parsing Package Config
//!
//! ```rust
//! use interlint_format::PackageConfig;
//!
//! let config = PackageConfig {
//!     name: "my-package".to_string(),
//!     version: "1.0.0".to_string(),
//!     engines: [("node".to_string(), ">=16.0.0".to_string())].into_iter().collect(),
//!     peer_dependencies: [("react".to_string(), "^18.0.0".to_string())].into_iter().collect(),
//!     ..Default::default()
//! };
//!
//! assert_eq!(config.name, "my-package");
//! assert_eq!(config.engines.get("node"), Some(&">=16.0.0".to_string()));
//! ```
//!
//! ## Conditional Exports
//!
//! ```rust
//! use interlint_format::{ExportsField, ExportsValue};
//! use std::collections::HashMap;
//!
//! // Simple string export: "exports": "./index.js"
//! let simple = ExportsField::Simple("./index.js".to_string());
//!
//! // Conditional export: "exports": { ".": { "import": "./esm.js", "require": "./cjs.js" } }
//! let mut conditions = HashMap::new();
//! let mut subpath = HashMap::new();
//! subpath.insert("import".to_string(), ExportsValue::Path("./esm.js".to_string()));
//! subpath.insert("require".to_string(), ExportsValue::Path("./cjs.js".to_string()));
//! conditions.insert(".".to_string(), ExportsValue::Conditional(subpath));
//! let conditional = ExportsField::Conditional(conditions);
//! ```
//!
//! # See Also
//!
//! - [`crate::changes`] - Breaking change types that can be detected
//! - [`crate::module::Entrypoint`] - Resolved entrypoint representation
//! - [Node.js package.json documentation](https://nodejs.org/api/packages.html)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Package.json configuration fields relevant to breaking change detection.
///
/// This structure captures the key fields from package.json that can cause
/// breaking changes when modified between versions. It focuses on fields
/// that affect how consumers use and install the package.
///
/// # Breaking Change Detection
///
/// Changes to these fields can trigger various breaking change types:
///
/// | Field | Breaking When |
/// |-------|--------------|
/// | `engines` | Node/npm minimum version raised |
/// | `peer_dependencies` | Dependency removed or range narrowed |
/// | `exports` | Subpath removed or conditions changed |
/// | `bin` | Command removed |
/// | `main`/`module` | Entry point path changed |
/// | `types` | TypeScript types path changed |
///
/// # Examples
///
/// ## Minimal Configuration
///
/// ```rust
/// use interlint_format::PackageConfig;
///
/// let config = PackageConfig {
///     name: "my-package".to_string(),
///     version: "1.0.0".to_string(),
///     ..Default::default()
/// };
/// ```
///
/// ## Full Configuration
///
/// ```rust
/// use interlint_format::{PackageConfig, ExportsField};
///
/// let config = PackageConfig {
///     name: "my-package".to_string(),
///     version: "2.0.0".to_string(),
///     engines: [("node".to_string(), ">=18.0.0".to_string())].into_iter().collect(),
///     peer_dependencies: [("react".to_string(), "^18.0.0".to_string())].into_iter().collect(),
///     exports: Some(ExportsField::Simple("./dist/index.js".to_string())),
///     main: Some("./dist/index.cjs".to_string()),
///     module: Some("./dist/index.mjs".to_string()),
///     module_type: Some("module".to_string()),
///     bin: [("my-cli".to_string(), "./bin/cli.js".to_string())].into_iter().collect(),
///     types: Some("./dist/index.d.ts".to_string()),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PackageConfig {
    /// Package name (required).
    ///
    /// The unique identifier for the package in the npm registry.
    pub name: String,

    /// Package version (required).
    ///
    /// The semantic version string (e.g., `"1.2.3"`).
    pub version: String,

    /// Engine version requirements.
    ///
    /// Maps engine names (e.g., `"node"`, `"npm"`) to semver ranges.
    /// Raising minimum versions is a breaking change.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "engines": {
    ///     "node": ">=16.0.0",
    ///     "npm": ">=8.0.0"
    ///   }
    /// }
    /// ```
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub engines: HashMap<String, String>,

    /// Peer dependency version requirements.
    ///
    /// Maps package names to semver ranges. Consumers must install these
    /// packages alongside this package. Removing or narrowing ranges is breaking.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "peerDependencies": {
    ///     "react": "^17.0.0 || ^18.0.0"
    ///   }
    /// }
    /// ```
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub peer_dependencies: HashMap<String, String>,

    /// Peer dependency metadata.
    ///
    /// Provides additional information about peer dependencies,
    /// such as whether they are optional.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "peerDependenciesMeta": {
    ///     "typescript": { "optional": true }
    ///   }
    /// }
    /// ```
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub peer_dependencies_meta: HashMap<String, PeerDepMeta>,

    /// Package exports field.
    ///
    /// Defines the entry points of the package and how they resolve
    /// under different conditions. Can be a simple string or a complex
    /// conditional mapping.
    ///
    /// # Breaking Changes
    ///
    /// - Removing a subpath export is breaking
    /// - Removing a condition (e.g., `"import"`, `"require"`) is breaking
    /// - Adding new subpaths is non-breaking (minor)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exports: Option<ExportsField>,

    /// Main entry point (CommonJS).
    ///
    /// The primary entry point when the package is required via CommonJS.
    /// Used as fallback when `exports` is not defined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,

    /// ESM entry point.
    ///
    /// The primary entry point when the package is imported as an ES module.
    /// Used by bundlers and modern Node.js.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,

    /// Package module type.
    ///
    /// Either `"commonjs"` or `"module"`. Determines how `.js` files
    /// are interpreted. Changing this is a major breaking change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_type: Option<String>,

    /// Binary/CLI commands.
    ///
    /// Maps command names to script paths. These become available
    /// in the `node_modules/.bin` directory. Removing commands is breaking.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "bin": {
    ///     "my-cli": "./bin/cli.js"
    ///   }
    /// }
    /// ```
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub bin: HashMap<String, String>,

    /// TypeScript type definitions path.
    ///
    /// Points to the main `.d.ts` file for TypeScript consumers.
    /// Changing or removing this path is breaking for TS users.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub types: Option<String>,

    /// TypeScript version-specific type definitions.
    ///
    /// Allows different type definitions for different TypeScript versions.
    /// Structure: `{ "<version-range>": { "<subpath>": ["<type-file>"] } }`
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "typesVersions": {
    ///     "<4.0": { "*": ["./ts3-types/*"] }
    ///   }
    /// }
    /// ```
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub types_versions: HashMap<String, HashMap<String, Vec<String>>>,

    /// Production dependencies.
    ///
    /// Tracked for detecting major bumps of re-exported dependencies,
    /// which can transitively break consumers.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dependencies: HashMap<String, String>,
}

impl PackageConfig {
    /// Create a new package configuration with name and version.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use interlint_format::PackageConfig;
    ///
    /// let config = PackageConfig::new("my-package", "1.0.0");
    /// assert_eq!(config.name, "my-package");
    /// assert_eq!(config.version, "1.0.0");
    /// ```
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            ..Default::default()
        }
    }

    /// Check if this package has peer dependencies.
    #[must_use]
    pub fn has_peer_dependencies(&self) -> bool {
        !self.peer_dependencies.is_empty()
    }

    /// Check if this package has engine requirements.
    #[must_use]
    pub fn has_engine_requirements(&self) -> bool {
        !self.engines.is_empty()
    }

    /// Check if this package uses the exports field.
    #[must_use]
    pub fn has_exports(&self) -> bool {
        self.exports.is_some()
    }

    /// Check if this package provides CLI commands.
    #[must_use]
    pub fn has_bin(&self) -> bool {
        !self.bin.is_empty()
    }

    /// Get the effective main entry point.
    ///
    /// Returns the `main` field if set, otherwise `None`.
    /// Note: `exports` takes precedence in modern Node.js.
    #[must_use]
    pub fn main_entry(&self) -> Option<&str> {
        self.main.as_deref()
    }

    /// Get the effective ESM entry point.
    ///
    /// Returns the `module` field if set, otherwise `None`.
    #[must_use]
    pub fn module_entry(&self) -> Option<&str> {
        self.module.as_deref()
    }

    /// Check if a peer dependency is optional.
    #[must_use]
    pub fn is_peer_optional(&self, name: &str) -> bool {
        self.peer_dependencies_meta
            .get(name)
            .map(|meta| meta.optional)
            .unwrap_or(false)
    }
}

/// Metadata for a peer dependency.
///
/// Provides additional configuration for peer dependencies,
/// such as whether they are optional.
///
/// # Examples
///
/// ```rust
/// use interlint_format::PeerDepMeta;
///
/// // Optional peer dependency
/// let optional_peer = PeerDepMeta { optional: true };
///
/// // Required peer dependency (default)
/// let required_peer = PeerDepMeta::default();
/// assert!(!required_peer.optional);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PeerDepMeta {
    /// Whether this peer dependency is optional.
    ///
    /// When true, npm/yarn will not warn if the peer dependency
    /// is not installed. Making a required peer optional is non-breaking,
    /// but making an optional peer required is breaking.
    #[serde(default)]
    pub optional: bool,
}

/// Package exports field structure.
///
/// The `exports` field in package.json can take several forms:
///
/// 1. **Simple string**: `"exports": "./index.js"` - Single entry point
/// 2. **Conditional object**: `"exports": { "import": "./esm.js", "require": "./cjs.js" }`
/// 3. **Subpath exports**: `"exports": { ".": "./index.js", "./utils": "./utils.js" }`
///
/// This enum uses `#[serde(untagged)]` to handle these different formats
/// during JSON deserialization.
///
/// # Breaking Change Detection
///
/// - Removing a subpath (e.g., `"./utils"`) is a major breaking change
/// - Removing a condition (e.g., `"import"`) is a major breaking change
/// - Adding new subpaths is a minor (non-breaking) change
/// - Changing paths under existing conditions may break resolution
///
/// # Examples
///
/// ## Simple Export
///
/// ```rust
/// use interlint_format::ExportsField;
///
/// // "exports": "./dist/index.js"
/// let exports = ExportsField::Simple("./dist/index.js".to_string());
/// ```
///
/// ## Conditional Exports
///
/// ```rust
/// use interlint_format::{ExportsField, ExportsValue};
/// use std::collections::HashMap;
///
/// // "exports": {
/// //   ".": { "import": "./esm/index.js", "require": "./cjs/index.js" },
/// //   "./utils": "./utils.js"
/// // }
/// let mut exports_map = HashMap::new();
///
/// // Main entry with conditions
/// let mut main_conditions = HashMap::new();
/// main_conditions.insert("import".to_string(), ExportsValue::Path("./esm/index.js".to_string()));
/// main_conditions.insert("require".to_string(), ExportsValue::Path("./cjs/index.js".to_string()));
/// exports_map.insert(".".to_string(), ExportsValue::Conditional(main_conditions));
///
/// // Simple subpath
/// exports_map.insert("./utils".to_string(), ExportsValue::Path("./utils.js".to_string()));
///
/// let exports = ExportsField::Conditional(exports_map);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExportsField {
    /// Simple string path export.
    ///
    /// Example: `"exports": "./index.js"`
    Simple(String),

    /// Conditional or subpath exports object.
    ///
    /// Maps subpath keys (e.g., `"."`, `"./utils"`) or condition keys
    /// (e.g., `"import"`, `"require"`) to export values.
    Conditional(HashMap<String, ExportsValue>),
}

impl ExportsField {
    /// Create a simple string export.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use interlint_format::ExportsField;
    ///
    /// let exports = ExportsField::simple("./dist/index.js");
    /// ```
    #[must_use]
    pub fn simple(path: impl Into<String>) -> Self {
        Self::Simple(path.into())
    }

    /// Check if this is a simple string export.
    #[must_use]
    pub fn is_simple(&self) -> bool {
        matches!(self, Self::Simple(_))
    }

    /// Get the simple path if this is a simple export.
    #[must_use]
    pub fn as_simple(&self) -> Option<&str> {
        match self {
            Self::Simple(path) => Some(path),
            Self::Conditional(_) => None,
        }
    }

    /// Get all subpath keys from conditional exports.
    ///
    /// Returns an empty vector for simple exports.
    pub fn subpaths(&self) -> Vec<&str> {
        match self {
            Self::Simple(_) => Vec::new(),
            Self::Conditional(map) => {
                // Return keys that look like subpaths (start with ".")
                map.keys()
                    .filter(|k| k.starts_with('.'))
                    .map(|k| k.as_str())
                    .collect()
            }
        }
    }
}

/// A value in the exports field.
///
/// Export values can be:
///
/// 1. **Path string**: `"./dist/index.js"` - Direct file path
/// 2. **Null**: `null` - Explicitly blocks a subpath
/// 3. **Conditional object**: `{ "import": "./esm.js", "require": "./cjs.js" }`
///
/// This enum uses `#[serde(untagged)]` to handle JSON deserialization
/// of these different value types.
///
/// # Examples
///
/// ## Path Value
///
/// ```rust
/// use interlint_format::ExportsValue;
///
/// let value = ExportsValue::Path("./dist/utils.js".to_string());
/// assert!(value.is_path());
/// ```
///
/// ## Null Value (Block Subpath)
///
/// ```rust
/// use interlint_format::ExportsValue;
///
/// // "exports": { "./internal": null }
/// let blocked = ExportsValue::Null;
/// assert!(blocked.is_null());
/// ```
///
/// ## Nested Conditions
///
/// ```rust
/// use interlint_format::ExportsValue;
/// use std::collections::HashMap;
///
/// // { "node": { "import": "./node-esm.js", "require": "./node-cjs.js" } }
/// let mut node_conditions = HashMap::new();
/// node_conditions.insert("import".to_string(), ExportsValue::Path("./node-esm.js".to_string()));
/// node_conditions.insert("require".to_string(), ExportsValue::Path("./node-cjs.js".to_string()));
///
/// let value = ExportsValue::Conditional(node_conditions);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExportsValue {
    /// A direct path string.
    Path(String),

    /// Null value (explicitly blocks a subpath).
    ///
    /// Used to prevent access to internal modules:
    /// `"exports": { "./internal/*": null }`
    Null,

    /// Nested conditional mapping.
    ///
    /// Allows multiple levels of conditions:
    /// ```json
    /// {
    ///   "node": { "import": "./node.js" },
    ///   "browser": { "import": "./browser.js" }
    /// }
    /// ```
    Conditional(HashMap<String, ExportsValue>),
}

impl ExportsValue {
    /// Create a path value.
    #[must_use]
    pub fn path(value: impl Into<String>) -> Self {
        Self::Path(value.into())
    }

    /// Check if this is a path value.
    #[must_use]
    pub fn is_path(&self) -> bool {
        matches!(self, Self::Path(_))
    }

    /// Check if this is a null value.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Check if this is a conditional value.
    #[must_use]
    pub fn is_conditional(&self) -> bool {
        matches!(self, Self::Conditional(_))
    }

    /// Get the path string if this is a path value.
    #[must_use]
    pub fn as_path(&self) -> Option<&str> {
        match self {
            Self::Path(path) => Some(path),
            _ => None,
        }
    }

    /// Get the conditional map if this is a conditional value.
    #[must_use]
    pub fn as_conditional(&self) -> Option<&HashMap<String, ExportsValue>> {
        match self {
            Self::Conditional(map) => Some(map),
            _ => None,
        }
    }
}

impl Default for ExportsValue {
    fn default() -> Self {
        Self::Null
    }
}

// =============================================================================
// Package Config Change Types
// =============================================================================

/// Type of change to a package configuration field.
///
/// Used to categorize changes detected between two versions of package.json.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageConfigChangeType {
    /// Engine version requirement raised (e.g., node >=14 to >=18)
    EngineVersionRaised,
    /// Engine version requirement lowered (widening compatibility)
    EngineVersionLowered,
    /// New engine requirement added
    EngineAdded,
    /// Engine requirement removed
    EngineRemoved,

    /// Peer dependency version range narrowed
    PeerDependencyNarrowed,
    /// Peer dependency version range widened
    PeerDependencyWidened,
    /// New peer dependency added
    PeerDependencyAdded,
    /// Peer dependency removed
    PeerDependencyRemoved,
    /// Peer dependency became required (was optional)
    PeerDependencyBecameRequired,
    /// Peer dependency became optional (was required)
    PeerDependencyBecameOptional,

    /// Export subpath removed
    ExportSubpathRemoved,
    /// Export subpath added
    ExportSubpathAdded,
    /// Export condition removed (e.g., "import", "require")
    ExportConditionRemoved,
    /// Export condition added
    ExportConditionAdded,
    /// Export path changed
    ExportPathChanged,

    /// Binary command removed
    BinCommandRemoved,
    /// Binary command added
    BinCommandAdded,
    /// Binary command path changed
    BinCommandPathChanged,

    /// Main entry point changed
    MainEntryChanged,
    /// Module entry point changed
    ModuleEntryChanged,
    /// Module type changed (commonjs/module)
    ModuleTypeChanged,

    /// Types path changed
    TypesPathChanged,
    /// Types version mapping changed
    TypesVersionsChanged,

    /// Dependency version changed (for re-exported deps)
    DependencyVersionChanged,
}

impl PackageConfigChangeType {
    /// Check if this change type is typically breaking.
    #[must_use]
    pub fn is_breaking(&self) -> bool {
        matches!(
            self,
            Self::EngineVersionRaised
                | Self::PeerDependencyNarrowed
                | Self::PeerDependencyRemoved
                | Self::PeerDependencyBecameRequired
                | Self::ExportSubpathRemoved
                | Self::ExportConditionRemoved
                | Self::BinCommandRemoved
                | Self::MainEntryChanged
                | Self::ModuleEntryChanged
                | Self::ModuleTypeChanged
                | Self::TypesPathChanged
        )
    }

    /// Get a human-readable description of this change type.
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            Self::EngineVersionRaised => "Engine version requirement raised",
            Self::EngineVersionLowered => "Engine version requirement lowered",
            Self::EngineAdded => "New engine requirement added",
            Self::EngineRemoved => "Engine requirement removed",
            Self::PeerDependencyNarrowed => "Peer dependency version range narrowed",
            Self::PeerDependencyWidened => "Peer dependency version range widened",
            Self::PeerDependencyAdded => "New peer dependency added",
            Self::PeerDependencyRemoved => "Peer dependency removed",
            Self::PeerDependencyBecameRequired => "Peer dependency became required",
            Self::PeerDependencyBecameOptional => "Peer dependency became optional",
            Self::ExportSubpathRemoved => "Export subpath removed",
            Self::ExportSubpathAdded => "Export subpath added",
            Self::ExportConditionRemoved => "Export condition removed",
            Self::ExportConditionAdded => "Export condition added",
            Self::ExportPathChanged => "Export path changed",
            Self::BinCommandRemoved => "Binary command removed",
            Self::BinCommandAdded => "Binary command added",
            Self::BinCommandPathChanged => "Binary command path changed",
            Self::MainEntryChanged => "Main entry point changed",
            Self::ModuleEntryChanged => "Module entry point changed",
            Self::ModuleTypeChanged => "Module type changed",
            Self::TypesPathChanged => "Types path changed",
            Self::TypesVersionsChanged => "Types versions mapping changed",
            Self::DependencyVersionChanged => "Dependency version changed",
        }
    }
}

// =============================================================================
// API Request/Response Types for Package Config Comparison
// =============================================================================

/// Request body for comparing two package configurations.
///
/// Used when sending package.json configurations to the Interlint API
/// for breaking change analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageConfigCompareRequest {
    /// The previous version's package configuration.
    pub old_config: PackageConfig,
    /// The new version's package configuration.
    pub new_config: PackageConfig,
}

impl PackageConfigCompareRequest {
    /// Create a new comparison request.
    #[must_use]
    pub fn new(old_config: PackageConfig, new_config: PackageConfig) -> Self {
        Self {
            old_config,
            new_config,
        }
    }
}

/// Response from the package configuration comparison API.
///
/// Contains all detected changes between two versions of package.json
/// along with a summary of the analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageConfigCompareResponse {
    /// Name of the package being compared.
    pub package_name: String,
    /// Version of the old configuration.
    pub old_version: String,
    /// Version of the new configuration.
    pub new_version: String,
    /// List of detected changes.
    pub changes: Vec<PackageJsonChange>,
    /// Summary statistics of the comparison.
    pub summary: PackageConfigComparisonSummary,
}

/// Summary statistics from a package configuration comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageConfigComparisonSummary {
    /// Total number of changes detected.
    pub total_changes: usize,
    /// Number of breaking changes.
    pub breaking_count: usize,
    /// Number of potentially breaking changes.
    pub potentially_breaking_count: usize,
    /// Number of safe/non-breaking changes.
    pub safe_count: usize,
    /// Whether any breaking changes were detected.
    pub has_breaking_changes: bool,
    /// Recommended semver bump based on changes ("major", "minor", or "patch").
    pub recommended_bump: String,
}

/// Severity level for package.json changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageChangeSeverity {
    /// Breaking change - will definitely break consumers.
    Breaking,
    /// Potentially breaking - may break some consumers.
    PotentiallyBreaking,
    /// Safe change - no breaking impact expected.
    Safe,
}

/// Category of package.json change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageChangeCategory {
    /// Engine requirements (node, npm versions).
    EngineRequirements,
    /// Peer dependencies.
    PeerDependencies,
    /// Entry points (main, module, exports).
    EntryPoints,
    /// Module type (commonjs/module).
    ModuleType,
    /// Binary/CLI commands.
    BinaryCommands,
    /// TypeScript type definitions.
    TypeScript,
    /// Production dependencies.
    Dependencies,
}

impl PackageChangeCategory {
    /// Get a human-readable label for this category.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::EngineRequirements => "Engine Requirements",
            Self::PeerDependencies => "Peer Dependencies",
            Self::EntryPoints => "Entry Points",
            Self::ModuleType => "Module Type",
            Self::BinaryCommands => "Binary Commands",
            Self::TypeScript => "TypeScript",
            Self::Dependencies => "Dependencies",
        }
    }
}

impl std::fmt::Display for PackageChangeCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// A detected change in package.json configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageJsonChange {
    /// Severity of the change.
    pub severity: PackageChangeSeverity,
    /// Category of the change.
    pub category: PackageChangeCategory,
    /// Human-readable description of the change.
    pub description: String,
    /// JSON path of the affected field (e.g., "engines.node").
    pub field_path: String,
    /// Previous value (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    /// New value (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    /// Suggested action for consumers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl PackageJsonChange {
    /// Create a new package change.
    #[must_use]
    pub fn new(
        severity: PackageChangeSeverity,
        category: PackageChangeCategory,
        description: impl Into<String>,
        field_path: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            category,
            description: description.into(),
            field_path: field_path.into(),
            old_value: None,
            new_value: None,
            suggestion: None,
        }
    }

    /// Set the old value.
    #[must_use]
    pub fn with_old_value(mut self, value: impl Into<String>) -> Self {
        self.old_value = Some(value.into());
        self
    }

    /// Set the new value.
    #[must_use]
    pub fn with_new_value(mut self, value: impl Into<String>) -> Self {
        self.new_value = Some(value.into());
        self
    }

    /// Set the suggestion.
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Check if this is a breaking change.
    #[must_use]
    pub fn is_breaking(&self) -> bool {
        self.severity == PackageChangeSeverity::Breaking
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_config_new() {
        let config = PackageConfig::new("my-package", "1.0.0");
        assert_eq!(config.name, "my-package");
        assert_eq!(config.version, "1.0.0");
        assert!(config.engines.is_empty());
        assert!(config.peer_dependencies.is_empty());
    }

    #[test]
    fn test_package_config_with_engines() {
        let mut config = PackageConfig::new("my-package", "1.0.0");
        config
            .engines
            .insert("node".to_string(), ">=16.0.0".to_string());
        config
            .engines
            .insert("npm".to_string(), ">=8.0.0".to_string());

        assert!(config.has_engine_requirements());
        assert_eq!(config.engines.get("node"), Some(&">=16.0.0".to_string()));
    }

    #[test]
    fn test_package_config_with_peer_deps() {
        let mut config = PackageConfig::new("my-package", "1.0.0");
        config
            .peer_dependencies
            .insert("react".to_string(), "^18.0.0".to_string());
        config.peer_dependencies_meta.insert(
            "typescript".to_string(),
            PeerDepMeta { optional: true },
        );
        config
            .peer_dependencies
            .insert("typescript".to_string(), "^5.0.0".to_string());

        assert!(config.has_peer_dependencies());
        assert!(!config.is_peer_optional("react"));
        assert!(config.is_peer_optional("typescript"));
    }

    #[test]
    fn test_exports_field_simple() {
        let exports = ExportsField::simple("./dist/index.js");

        assert!(exports.is_simple());
        assert_eq!(exports.as_simple(), Some("./dist/index.js"));
    }

    #[test]
    fn test_exports_field_conditional() {
        let mut map = HashMap::new();
        map.insert(".".to_string(), ExportsValue::path("./index.js"));
        map.insert("./utils".to_string(), ExportsValue::path("./utils.js"));

        let exports = ExportsField::Conditional(map);

        assert!(!exports.is_simple());
        let subpaths = exports.subpaths();
        assert!(subpaths.contains(&"."));
        assert!(subpaths.contains(&"./utils"));
    }

    #[test]
    fn test_exports_value_path() {
        let value = ExportsValue::path("./dist/index.js");

        assert!(value.is_path());
        assert!(!value.is_null());
        assert!(!value.is_conditional());
        assert_eq!(value.as_path(), Some("./dist/index.js"));
    }

    #[test]
    fn test_exports_value_null() {
        let value = ExportsValue::Null;

        assert!(!value.is_path());
        assert!(value.is_null());
        assert!(!value.is_conditional());
        assert_eq!(value.as_path(), None);
    }

    #[test]
    fn test_exports_value_conditional() {
        let mut conditions = HashMap::new();
        conditions.insert("import".to_string(), ExportsValue::path("./esm.js"));
        conditions.insert("require".to_string(), ExportsValue::path("./cjs.js"));

        let value = ExportsValue::Conditional(conditions);

        assert!(!value.is_path());
        assert!(!value.is_null());
        assert!(value.is_conditional());
        assert!(value.as_conditional().is_some());
    }

    #[test]
    fn test_package_config_serialization() {
        let config = PackageConfig {
            name: "my-package".to_string(),
            version: "1.0.0".to_string(),
            engines: [("node".to_string(), ">=16".to_string())]
                .into_iter()
                .collect(),
            exports: Some(ExportsField::simple("./index.js")),
            ..Default::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: PackageConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_exports_json_deserialization() {
        // Simple string
        let json = r#""./index.js""#;
        let exports: ExportsField = serde_json::from_str(json).unwrap();
        assert!(matches!(exports, ExportsField::Simple(ref s) if s == "./index.js"));

        // Conditional object
        let json = r#"{ ".": "./index.js", "./utils": "./utils.js" }"#;
        let exports: ExportsField = serde_json::from_str(json).unwrap();
        assert!(matches!(exports, ExportsField::Conditional(_)));
    }

    #[test]
    fn test_package_config_change_type_is_breaking() {
        assert!(PackageConfigChangeType::EngineVersionRaised.is_breaking());
        assert!(PackageConfigChangeType::PeerDependencyRemoved.is_breaking());
        assert!(PackageConfigChangeType::ExportSubpathRemoved.is_breaking());
        assert!(PackageConfigChangeType::BinCommandRemoved.is_breaking());

        assert!(!PackageConfigChangeType::EngineVersionLowered.is_breaking());
        assert!(!PackageConfigChangeType::PeerDependencyWidened.is_breaking());
        assert!(!PackageConfigChangeType::ExportSubpathAdded.is_breaking());
        assert!(!PackageConfigChangeType::BinCommandAdded.is_breaking());
    }

    #[test]
    fn test_package_config_change_type_description() {
        assert_eq!(
            PackageConfigChangeType::EngineVersionRaised.description(),
            "Engine version requirement raised"
        );
        assert_eq!(
            PackageConfigChangeType::ExportSubpathRemoved.description(),
            "Export subpath removed"
        );
    }

    #[test]
    fn test_peer_dep_meta_default() {
        let meta = PeerDepMeta::default();
        assert!(!meta.optional);
    }

    #[test]
    fn test_bin_commands() {
        let mut config = PackageConfig::new("my-cli", "1.0.0");
        config
            .bin
            .insert("my-cli".to_string(), "./bin/cli.js".to_string());

        assert!(config.has_bin());
        assert_eq!(config.bin.get("my-cli"), Some(&"./bin/cli.js".to_string()));
    }

    #[test]
    fn test_types_versions() {
        let mut config = PackageConfig::new("my-package", "1.0.0");
        config.types = Some("./dist/index.d.ts".to_string());

        let mut ts3_mapping = HashMap::new();
        ts3_mapping.insert("*".to_string(), vec!["./ts3-types/*".to_string()]);
        config
            .types_versions
            .insert("<4.0".to_string(), ts3_mapping);

        assert!(config.types.is_some());
        assert!(!config.types_versions.is_empty());
    }

    #[test]
    fn test_main_and_module_entries() {
        let mut config = PackageConfig::new("my-package", "1.0.0");
        config.main = Some("./dist/index.cjs".to_string());
        config.module = Some("./dist/index.mjs".to_string());
        config.module_type = Some("module".to_string());

        assert_eq!(config.main_entry(), Some("./dist/index.cjs"));
        assert_eq!(config.module_entry(), Some("./dist/index.mjs"));
    }

    #[test]
    fn test_complex_exports_structure() {
        // Simulate: { ".": { "import": "./esm/index.js", "require": "./cjs/index.js" } }
        let mut main_conditions = HashMap::new();
        main_conditions.insert(
            "import".to_string(),
            ExportsValue::path("./esm/index.js"),
        );
        main_conditions.insert(
            "require".to_string(),
            ExportsValue::path("./cjs/index.js"),
        );

        let mut exports_map = HashMap::new();
        exports_map.insert(".".to_string(), ExportsValue::Conditional(main_conditions));

        let exports = ExportsField::Conditional(exports_map);

        let json = serde_json::to_string(&exports).unwrap();
        let deserialized: ExportsField = serde_json::from_str(&json).unwrap();

        assert_eq!(exports, deserialized);
    }
}
