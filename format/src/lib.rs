//! # interlint-format
//!
//! Language-agnostic format for describing code signatures and exports.
//!
//! This crate provides data structures for representing the public interface
//! of a package or module, including functions, classes, interfaces, enums,
//! type aliases, and variables.
//!
//! ## Features
//!
//! - **Language-agnostic**: Works with JavaScript, TypeScript, Rust, Python, Go, etc.
//! - **Package-manager agnostic**: Not tied to npm, cargo, pip, or any specific ecosystem
//! - **Binary format**: Compact MessagePack serialization with integrity checks
//! - **Type-safe**: Strong typing for all signature components
//!
//! ## Breaking Change Detection
//!
//! The interlint format provides two modules for breaking change detection:
//!
//! ### Recommended: [`changes`] Module (New)
//!
//! The [`changes`] module provides a comprehensive breaking change API with:
//!
//! - **[`ChangeReport`]**: Aggregate type for collecting and analyzing changes
//! - **[`ChangeSummary`]**: Statistics about detected changes
//! - **[`Change`] trait**: Common interface with `migration_hint()` and semver support
//! - **[`AssetChange`]**: Asset changes including `Removed`, `SchemaChanged`, `EncodingChanged`
//! - **[`CodeChange`]**: Code changes including `ExportRemoved`, `VisibilityChanged`, etc.
//!
//! ```rust
//! use interlint_format::{ChangeReport, BreakingChange, AssetChange, AssetFormatChange, Change};
//!
//! let mut report = ChangeReport::new("my-package", "1.0.0", "2.0.0");
//! report.add_change(BreakingChange::Asset(AssetChange::FormatChanged(
//!     AssetFormatChange::new("config.json", "json", "yaml")
//!         .with_hint("Convert JSON files to YAML format")
//! )));
//!
//! assert!(report.has_breaking_changes());
//! assert_eq!(report.recommended_bump(), "major");
//! ```
//!
//! ### Legacy: [`breaking_changes`] Module
//!
//! The [`breaking_changes`] module provides the original API with builders:
//!
//! - **[`LibraryBreakingChange`]**: Code/API breaking changes
//! - **[`PackageBreakingChange`]**: Asset/package breaking changes
//! - **Builder patterns**: `ApiSignatureChangeBuilder`, `TypeChangeBuilder`, etc.
//!
//! ## Breaking Change Categories
//!
//! Breaking changes are categorized into two primary domains:
//!
//! ### Library/Code Breaking Changes
//!
//! Changes that affect the programmatic API consumers import and use:
//!
//! - **API Signature Changes**: Function parameters, return types, async modifiers
//! - **Type Changes**: Struct fields, enum variants, generics, inheritance
//! - **Behavior Changes**: Error handling, return values, side effects
//! - **Export Removals**: Deleted exports from the public API
//! - **Visibility Changes**: Public to private transitions
//! - **Generic Constraint Changes**: Tighter type bounds
//! - **Default Value Changes**: Changed default parameter/property values
//!
//! ### Package/Asset Breaking Changes
//!
//! Changes that affect non-code files distributed with the package:
//!
//! - **Asset Format Changes**: CSS/JSON/image format changes
//! - **Asset Path Changes**: Moved, renamed, or removed assets
//! - **Asset Dependency Changes**: References between assets
//! - **Asset Metadata Changes**: Configuration and metadata files
//! - **Asset Removals**: Deleted asset files
//! - **Schema Changes**: JSON/XML schema structure changes
//! - **Encoding Changes**: File encoding changes (UTF-8 to UTF-16, etc.)
//!
//! This distinction is important because asset changes affect build/deployment
//! workflows rather than runtime code.
//!
//! See [`changes`] module for the recommended API and [`breaking_changes`] for the legacy API.
//!
//! ## Binary Format
//!
//! The crate uses a binary format with:
//! - Magic bytes header ("ILMJ" - InterLint MessagePack JSON)
//! - Version field for forward compatibility
//! - CRC32 checksum for data integrity
//! - MessagePack payload for compact serialization
//!
//! ## Example
//!
//! ```rust
//! use interlint_format::{
//!     PublicInterface, Metadata, Module, Export, Entrypoint,
//!     Signature, FunctionSig, TypeInfo, Visibility, binary,
//! };
//!
//! // Create package metadata
//! let metadata = Metadata {
//!     name: "my-package".to_string(),
//!     version: "1.0.0".to_string(),
//!     description: Some("A sample package".to_string()),
//!     repository: None,
//!     license: Some("MIT".to_string()),
//!     language: Some("typescript".to_string()),
//! };
//!
//! // Create a public interface
//! let mut interface = PublicInterface::new(metadata);
//!
//! // Add an entrypoint
//! interface.add_entrypoint(Entrypoint {
//!     name: ".".to_string(),
//!     module_path: "src/index.ts".to_string(),
//!     conditions: vec!["import".to_string(), "types".to_string()],
//! });
//!
//! // Create a module with exports
//! let mut module = Module::new("src/index.ts");
//!
//! module.add_export(Export {
//!     name: "greet".to_string(),
//!     original_name: None,
//!     signature: Signature::Function(FunctionSig {
//!         name: "greet".to_string(),
//!         type_parameters: vec![],
//!         parameters: vec![],
//!         return_type: TypeInfo::primitive("string"),
//!         is_async: false,
//!         is_generator: false,
//!         visibility: Visibility::Public,
//!         doc: Some("Returns a greeting".to_string()),
//!     }),
//!     is_default: false,
//! });
//!
//! interface.add_module(module);
//!
//! // Encode to binary format
//! let bytes = binary::encode(&interface).unwrap();
//! assert!(binary::is_valid_format(&bytes));
//!
//! // Decode from binary format
//! let decoded: PublicInterface = binary::decode(&bytes).unwrap();
//! assert_eq!(decoded.metadata.name, "my-package");
//! ```

pub mod binary;
pub mod breaking_changes;
pub mod changes;
pub mod module;
pub mod signature;
pub mod types;

// Re-export all public types for convenience
pub use binary::{decode, encode, is_valid_format, BinaryError, FormatInfo, FORMAT_VERSION, MAGIC};

// Legacy breaking_changes module exports (trait-based API)
pub use breaking_changes::{
    ApiSignatureChangeBuilder, AssetFormatChangeBuilder, BehaviorChangeBuilder,
    BehaviorChangeData,
    // Export the trait under both names for backwards compatibility
    BreakingChange as BreakingChangeTrait, BreakingChange as LegacyBreakingChangeTrait,
    ChangeLocation, LibraryBreakingChange, PackageBreakingChange,
    Severity as LegacySeverity, TypeChangeBuilder, TypeChangeKind,
};
pub use module::{
    Asset, Entrypoint, Export, Metadata, Module, PublicInterface, ReExport, ReExportName,
};
pub use signature::{
    ClassSig, EnumMember, EnumSig, FunctionSig, IndexSignature, InterfaceSig, NamespaceSig,
    Parameter, Property, Signature, TypeAliasSig, VariableSig, Visibility,
};
pub use types::{TypeInfo, TypeKind, TypeParameter};

// Re-export new changes module types (preferred API)
pub use changes::{
    AssetAddedChange,
    // Asset changes
    AssetChange,
    AssetDependencyChange,
    AssetEncodingChange,
    AssetFormatChange,
    AssetMetadataChange,
    AssetPathChange,
    AssetRemovedChange,
    AssetSchemaChange,
    // Behavior aspects
    BehaviorAspect,
    BehaviorChange,
    // Core types
    BreakingChange,
    Change,
    ChangeKind,
    // Report and summary
    ChangeReport,
    ChangeSummary,
    // Code changes
    CodeChange,
    DefaultValueChange,
    DependencyVersionChange,
    ExportAddedChange,
    ExportRemovedChange,
    GenericConstraintChange,
    ModuleChange,
    MovedExport,
    // Parameter and property changes
    ParameterChange,
    ParameterChangeType,
    PropertyChange,
    PropertyChangeType,
    ReturnTypeChange,
    SchemaFieldChange,
    SchemaFieldChangeType,
    Severity,
    SignatureChange,
    TypeChange,
    VisibilityChange,
    VisibilityLevel,
};
