//! Module and export definitions for public interface representation.
//!
//! This module provides the core data structures for representing a package's
//! public interface, including modules, exports, re-exports, entrypoints, and assets.
//!
//! # Overview
//!
//! The module hierarchy represents how code is organized within a package:
//!
//! - [`PublicInterface`] - The root container for all public API information
//! - [`Module`] - A single source file with its exports
//! - [`Export`] - A named item exported from a module
//! - [`ReExport`] - A re-export from another module
//! - [`Entrypoint`] - A package entry point (from package.json `exports`)
//! - [`Asset`] - A non-code file included in the package
//!
//! # Breaking Change Categories
//!
//! This module defines structures that can trigger two categories of breaking changes:
//!
//! ## Library/Code Breaking Changes
//!
//! Changes to [`Module`], [`Export`], [`ReExport`], and [`Entrypoint`] affect
//! the programmatic API that consumers import and use:
//!
//! - `ModuleRemoved` / `ModuleAdded`
//! - `ExportRemoved` / `ExportAdded`
//! - `EntrypointRemoved` / `EntrypointAdded`
//!
//! ## Package/Asset Breaking Changes
//!
//! Changes to [`Asset`] affect non-code files distributed with the package:
//!
//! - `AssetRemoved` / `AssetAdded`
//!
//! # Examples
//!
//! ## Creating a Public Interface
//!
//! ```rust
//! use interlint_format::{PublicInterface, Metadata, Module, Export, Signature, FunctionSig, TypeInfo, Visibility};
//!
//! let metadata = Metadata {
//!     name: "my-package".to_string(),
//!     version: "1.0.0".to_string(),
//!     description: Some("A sample package".to_string()),
//!     repository: None,
//!     license: Some("MIT".to_string()),
//!     language: Some("typescript".to_string()),
//! };
//!
//! let mut interface = PublicInterface::new(metadata);
//!
//! let mut module = Module::new("src/index.ts");
//! module.add_export(Export {
//!     name: "greet".to_string(),
//!     original_name: None,
//!     signature: Signature::Function(FunctionSig {
//!         name: "greet".to_string(),
//!         type_parameters: vec![],
//!         parameters: vec![],
//!         return_type: TypeInfo::void(),
//!         is_async: false,
//!         is_generator: false,
//!         visibility: Visibility::Public,
//!         doc: None,
//!     }),
//!     is_default: false,
//! });
//!
//! interface.add_module(module);
//! ```

use serde::{Deserialize, Serialize};

use crate::signature::Signature;

/// An export from a module.
///
/// Exports represent items that are made available for import by other modules.
/// Each export has a name, an optional original name (for renamed exports),
/// a signature describing the exported item, and a flag indicating if it's
/// the default export.
///
/// # Breaking Change Detection
///
/// Changes to exports can trigger several breaking change types:
///
/// - [`ExportRemoved`](crate::BreakingChangeType::ExportRemoved) - Critical: Export was removed
/// - [`ExportAdded`](crate::BreakingChangeType::ExportAdded) - Warning: New export added
/// - [`ExportKindChanged`](crate::BreakingChangeType::ExportKindChanged) - Critical: Export kind changed
/// - [`DefaultExportChanged`](crate::BreakingChangeType::DefaultExportChanged) - Major: Default status changed
///
/// # Examples
///
/// ## Named Export
///
/// ```rust
/// use interlint_format::{Export, Signature, FunctionSig, TypeInfo, Visibility};
///
/// // export function greet(): void
/// let export = Export {
///     name: "greet".to_string(),
///     original_name: None,
///     signature: Signature::Function(FunctionSig {
///         name: "greet".to_string(),
///         type_parameters: vec![],
///         parameters: vec![],
///         return_type: TypeInfo::void(),
///         is_async: false,
///         is_generator: false,
///         visibility: Visibility::Public,
///         doc: None,
///     }),
///     is_default: false,
/// };
/// ```
///
/// ## Renamed Export
///
/// ```rust
/// use interlint_format::{Export, Signature, VariableSig, TypeInfo, Visibility};
///
/// // const _internal = 42; export { _internal as publicName }
/// let export = Export {
///     name: "publicName".to_string(),
///     original_name: Some("_internal".to_string()),
///     signature: Signature::Variable(VariableSig {
///         name: "_internal".to_string(),
///         type_info: TypeInfo::primitive("number"),
///         is_const: true,
///         visibility: Visibility::Public,
///         doc: None,
///     }),
///     is_default: false,
/// };
/// ```
///
/// ## Default Export
///
/// ```rust
/// use interlint_format::{Export, Signature, ClassSig, Visibility};
///
/// // export default class MyComponent { }
/// let export = Export {
///     name: "default".to_string(),
///     original_name: Some("MyComponent".to_string()),
///     signature: Signature::Class(ClassSig {
///         name: "MyComponent".to_string(),
///         type_parameters: vec![],
///         extends: None,
///         implements: vec![],
///         properties: vec![],
///         methods: vec![],
///         constructor: None,
///         is_abstract: false,
///         visibility: Visibility::Public,
///         doc: None,
///     }),
///     is_default: true,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Export {
    /// The exported name (may differ from original name if renamed).
    ///
    /// This is the name that consumers use to import this item.
    /// For default exports, this is typically `"default"`.
    pub name: String,

    /// Original name if this is a renamed export.
    ///
    /// When an export is renamed (e.g., `export { foo as bar }`),
    /// this contains the original internal name (`"foo"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,

    /// The signature of the exported item.
    ///
    /// Contains the complete type information for the export,
    /// whether it's a function, class, variable, etc.
    pub signature: Signature,

    /// Whether this is a default export.
    ///
    /// When true, this export can be imported with default import syntax:
    /// `import MyComponent from 'package'` instead of `import { MyComponent } from 'package'`.
    #[serde(default)]
    pub is_default: bool,
}

/// A re-export from another module.
///
/// Re-exports allow a module to expose items from other modules without
/// importing and re-exporting them explicitly. This is common for creating
/// "barrel" files that aggregate exports from multiple modules.
///
/// # Types of Re-exports
///
/// 1. **Wildcard**: `export * from './utils'` - Re-exports all exports
/// 2. **Named**: `export { foo, bar } from './utils'` - Re-exports specific items
/// 3. **Renamed**: `export { foo as bar } from './utils'` - Re-exports with aliasing
///
/// # Examples
///
/// ## Wildcard Re-export
///
/// ```rust
/// use interlint_format::ReExport;
///
/// // export * from './utils'
/// let re_export = ReExport {
///     source: "./utils".to_string(),
///     names: vec![],
///     is_wildcard: true,
/// };
/// ```
///
/// ## Named Re-export with Alias
///
/// ```rust
/// use interlint_format::{ReExport, ReExportName};
///
/// // export { helper as publicHelper } from './internal'
/// let re_export = ReExport {
///     source: "./internal".to_string(),
///     names: vec![
///         ReExportName {
///             name: "helper".to_string(),
///             alias: Some("publicHelper".to_string()),
///         },
///     ],
///     is_wildcard: false,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReExport {
    /// The source module path.
    ///
    /// This is the path to the module being re-exported from,
    /// as it would appear in the source code (e.g., `"./utils"`).
    pub source: String,

    /// Specific names being re-exported (empty means all/wildcard).
    ///
    /// For named re-exports, this contains the list of items to re-export.
    /// Empty when `is_wildcard` is true.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<ReExportName>,

    /// Whether this is a wildcard re-export (`export * from`).
    ///
    /// When true, all exports from the source module are re-exported.
    /// The `names` field should be empty in this case.
    #[serde(default)]
    pub is_wildcard: bool,
}

/// A single name in a re-export.
///
/// Represents one item being re-exported, with optional aliasing.
///
/// # Examples
///
/// ```rust
/// use interlint_format::ReExportName;
///
/// // export { foo } from './module' - no alias
/// let name1 = ReExportName {
///     name: "foo".to_string(),
///     alias: None,
/// };
///
/// // export { foo as bar } from './module' - with alias
/// let name2 = ReExportName {
///     name: "foo".to_string(),
///     alias: Some("bar".to_string()),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReExportName {
    /// Original name from source module.
    ///
    /// The name of the item as it exists in the source module.
    pub name: String,

    /// Renamed export name (if different from original).
    ///
    /// When present, the item is re-exported under this alias
    /// instead of its original name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

/// A module representing a single source file.
///
/// A module corresponds to a single source file in the package and contains
/// all the exports from that file. Modules are identified by their path
/// relative to the package root.
///
/// # Breaking Change Detection
///
/// Module-level changes trigger:
///
/// - [`ModuleRemoved`](crate::BreakingChangeType::ModuleRemoved) - Critical: Module was removed
/// - [`ModuleAdded`](crate::BreakingChangeType::ModuleAdded) - Warning: New module added
///
/// # Examples
///
/// ## Creating a Module with Exports
///
/// ```rust
/// use interlint_format::{Module, Export, Signature, FunctionSig, TypeInfo, Visibility};
///
/// let mut module = Module::new("src/utils.ts");
/// module.doc = Some("Utility functions".to_string());
///
/// module.add_export(Export {
///     name: "formatDate".to_string(),
///     original_name: None,
///     signature: Signature::Function(FunctionSig {
///         name: "formatDate".to_string(),
///         type_parameters: vec![],
///         parameters: vec![],
///         return_type: TypeInfo::primitive("string"),
///         is_async: false,
///         is_generator: false,
///         visibility: Visibility::Public,
///         doc: Some("Formats a date as a string".to_string()),
///     }),
///     is_default: false,
/// });
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    /// Module path/identifier (relative to package root).
    ///
    /// This is the path used to import from this module, e.g., `"src/index.ts"`.
    pub path: String,

    /// Direct exports from this module.
    ///
    /// Contains all items explicitly exported from this module
    /// (not including re-exports).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<Export>,

    /// Re-exports from other modules.
    ///
    /// Contains re-export statements that make items from other
    /// modules available through this module.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub re_exports: Vec<ReExport>,

    /// Documentation comment for the module.
    ///
    /// Top-level documentation extracted from the module file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

impl Module {
    /// Create a new module with the given path.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            exports: Vec::new(),
            re_exports: Vec::new(),
            doc: None,
        }
    }

    /// Add an export to this module.
    pub fn add_export(&mut self, export: Export) {
        self.exports.push(export);
    }

    /// Add a re-export to this module.
    pub fn add_re_export(&mut self, re_export: ReExport) {
        self.re_exports.push(re_export);
    }

    /// Get all export names from this module.
    pub fn export_names(&self) -> impl Iterator<Item = &str> {
        self.exports.iter().map(|e| e.name.as_str())
    }
}

/// An entrypoint into a package (e.g., main, bin, exports).
///
/// Entrypoints define how consumers can import from a package. In Node.js
/// packages, these correspond to the `exports` field in package.json.
/// An entrypoint maps an import path to an actual file in the package.
///
/// # Breaking Change Detection
///
/// Entrypoint changes trigger:
///
/// - [`EntrypointRemoved`](crate::BreakingChangeType::EntrypointRemoved) - Critical: Entrypoint removed
/// - [`EntrypointAdded`](crate::BreakingChangeType::EntrypointAdded) - Warning: New entrypoint added
///
/// # Examples
///
/// ## Main Entrypoint
///
/// ```rust
/// use interlint_format::Entrypoint;
///
/// // package.json: "exports": { ".": "./dist/index.js" }
/// let main_entry = Entrypoint {
///     name: ".".to_string(),
///     module_path: "dist/index.js".to_string(),
///     conditions: vec!["import".to_string(), "default".to_string()],
/// };
/// ```
///
/// ## Subpath Entrypoint
///
/// ```rust
/// use interlint_format::Entrypoint;
///
/// // package.json: "exports": { "./utils": "./dist/utils.js" }
/// let utils_entry = Entrypoint {
///     name: "./utils".to_string(),
///     module_path: "dist/utils.js".to_string(),
///     conditions: vec!["import".to_string()],
/// };
/// ```
///
/// ## Conditional Entrypoint
///
/// ```rust
/// use interlint_format::Entrypoint;
///
/// // package.json: "exports": { ".": { "types": "./dist/index.d.ts", "import": "./dist/index.js" } }
/// let types_entry = Entrypoint {
///     name: ".".to_string(),
///     module_path: "dist/index.d.ts".to_string(),
///     conditions: vec!["types".to_string()],
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrypoint {
    /// The entrypoint name/key (e.g., `"."`, `"./utils"`, `"main"`).
    ///
    /// This is the import specifier that consumers use to access this entrypoint.
    /// `"."` represents the main/default entrypoint.
    pub name: String,

    /// Path to the module file.
    ///
    /// The actual file path within the package that this entrypoint resolves to.
    pub module_path: String,

    /// Conditions for this entrypoint (e.g., `"import"`, `"require"`, `"types"`).
    ///
    /// These correspond to Node.js conditional exports, allowing different
    /// files to be served based on the import context.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<String>,
}

/// A package asset (non-code file included in distribution).
///
/// Assets represent non-code files that are distributed with the package.
/// These might be CSS stylesheets, images, configuration files, fonts,
/// or any other static resources that consumers might reference.
///
/// # Package/Asset Breaking Changes
///
/// Unlike code changes (Library/Code Breaking Changes), asset changes
/// affect the static resources that consumers depend on:
///
/// - [`AssetRemoved`](crate::BreakingChangeType::AssetRemoved) - Major: Asset was removed
/// - [`AssetAdded`](crate::BreakingChangeType::AssetAdded) - Warning: New asset added
///
/// ## When Asset Changes Matter
///
/// Asset changes are breaking when consumers:
///
/// 1. **Import CSS directly**: `import 'my-package/styles/main.css'`
/// 2. **Reference images**: `<img src="node_modules/my-package/logo.png">`
/// 3. **Use configuration files**: Tooling that reads package configs
/// 4. **Bundle static assets**: Build tools that copy package assets
///
/// ## Difference from Code Breaking Changes
///
/// | Aspect | Code Changes | Asset Changes |
/// |--------|--------------|---------------|
/// | Impact | Runtime errors, type mismatches | Build/bundle failures, missing resources |
/// | Detection | Import statements | Build configs, static references |
/// | Severity | Usually Critical/Major | Usually Major/Warning |
///
/// # Examples
///
/// ## CSS Stylesheet
///
/// ```rust
/// use interlint_format::Asset;
///
/// let css_asset = Asset {
///     path: "styles/main.css".to_string(),
///     extension: Some("css".to_string()),
/// };
/// ```
///
/// ## Image Asset
///
/// ```rust
/// use interlint_format::Asset;
///
/// let image_asset = Asset {
///     path: "images/logo.png".to_string(),
///     extension: Some("png".to_string()),
/// };
/// ```
///
/// ## Configuration File
///
/// ```rust
/// use interlint_format::Asset;
///
/// let config_asset = Asset {
///     path: "config/default.json".to_string(),
///     extension: Some("json".to_string()),
/// };
/// ```
///
/// ## Asset Without Extension
///
/// ```rust
/// use interlint_format::Asset;
///
/// let extensionless_asset = Asset {
///     path: "bin/cli".to_string(),
///     extension: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// Path relative to package root.
    ///
    /// The file path where the asset is located within the package.
    /// This path is used by consumers to reference the asset.
    pub path: String,

    /// File extension (e.g., `"css"`, `"png"`, `"json"`).
    ///
    /// The extension of the file, without the leading dot.
    /// This helps categorize the type of asset for analysis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
}

/// Metadata about the package.
///
/// Contains descriptive information about the package, extracted from
/// the package manifest (e.g., `package.json`, `Cargo.toml`, `pyproject.toml`).
///
/// # Examples
///
/// ```rust
/// use interlint_format::Metadata;
///
/// let metadata = Metadata {
///     name: "my-awesome-package".to_string(),
///     version: "2.1.0".to_string(),
///     description: Some("A library for doing awesome things".to_string()),
///     repository: Some("https://github.com/user/my-awesome-package".to_string()),
///     license: Some("MIT".to_string()),
///     language: Some("typescript".to_string()),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    /// Package name.
    ///
    /// The unique identifier for the package in its registry
    /// (e.g., npm package name, crate name, PyPI package name).
    pub name: String,

    /// Package version.
    ///
    /// The semantic version string (e.g., `"1.2.3"`, `"0.1.0-beta.1"`).
    pub version: String,

    /// Package description.
    ///
    /// A brief human-readable description of what the package does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Repository URL.
    ///
    /// Link to the source code repository (e.g., GitHub, GitLab URL).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    /// License.
    ///
    /// The SPDX license identifier or license name (e.g., `"MIT"`, `"Apache-2.0"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// Language of the source code.
    ///
    /// The primary programming language (e.g., `"typescript"`, `"rust"`, `"python"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// The complete public interface of a package.
///
/// `PublicInterface` is the root container for all information about a package's
/// public API. It includes metadata, entrypoints, modules with their exports,
/// and non-code assets.
///
/// # Breaking Change Categories
///
/// The public interface contains two categories of items that can change:
///
/// ## Library/Code Breaking Changes
///
/// Changes to [`entrypoints`](Self::entrypoints) and [`modules`](Self::modules)
/// affect the programmatic API:
///
/// - Import paths that no longer resolve
/// - Functions, classes, or types that were removed or changed
/// - Re-exports that are no longer available
///
/// ## Package/Asset Breaking Changes
///
/// Changes to [`assets`](Self::assets) affect non-code resources:
///
/// - CSS files that consumers import
/// - Images or fonts that were removed
/// - Configuration files that tooling depends on
///
/// # Examples
///
/// ## Creating a Complete Interface
///
/// ```rust
/// use interlint_format::{
///     PublicInterface, Metadata, Module, Export, Entrypoint, Asset,
///     Signature, FunctionSig, TypeInfo, Visibility,
/// };
///
/// let metadata = Metadata {
///     name: "my-library".to_string(),
///     version: "1.0.0".to_string(),
///     description: Some("A useful library".to_string()),
///     repository: None,
///     license: Some("MIT".to_string()),
///     language: Some("typescript".to_string()),
/// };
///
/// let mut interface = PublicInterface::new(metadata);
///
/// // Add main entrypoint
/// interface.add_entrypoint(Entrypoint {
///     name: ".".to_string(),
///     module_path: "src/index.ts".to_string(),
///     conditions: vec!["import".to_string()],
/// });
///
/// // Add module with exports
/// let mut module = Module::new("src/index.ts");
/// module.add_export(Export {
///     name: "greet".to_string(),
///     original_name: None,
///     signature: Signature::Function(FunctionSig {
///         name: "greet".to_string(),
///         type_parameters: vec![],
///         parameters: vec![],
///         return_type: TypeInfo::primitive("string"),
///         is_async: false,
///         is_generator: false,
///         visibility: Visibility::Public,
///         doc: None,
///     }),
///     is_default: false,
/// });
/// interface.add_module(module);
///
/// // Add CSS asset
/// interface.add_asset(Asset {
///     path: "styles/main.css".to_string(),
///     extension: Some("css".to_string()),
/// });
/// ```
///
/// ## Analyzing an Interface
///
/// ```rust
/// use interlint_format::PublicInterface;
///
/// fn analyze(interface: &PublicInterface) {
///     println!("Package: {} v{}", interface.metadata.name, interface.metadata.version);
///     println!("Entrypoints: {}", interface.entrypoints.len());
///     println!("Modules: {}", interface.modules.len());
///     println!("Total exports: {}", interface.export_count());
///     println!("Assets: {}", interface.assets.len());
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicInterface {
    /// Package metadata.
    ///
    /// Contains the package name, version, and other descriptive information.
    pub metadata: Metadata,

    /// Package entrypoints.
    ///
    /// Defines how consumers can import from the package. Maps import paths
    /// to actual module files. Empty for packages without explicit exports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entrypoints: Vec<Entrypoint>,

    /// All modules in the package.
    ///
    /// Contains the source files and their exports. Each module represents
    /// a single file that can be imported from.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<Module>,

    /// Non-code assets included in the package.
    ///
    /// Contains static resources like CSS, images, and configuration files.
    /// These are tracked separately from code exports because they represent
    /// a different category of breaking changes (Package/Asset vs Library/Code).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<Asset>,
}

impl PublicInterface {
    /// Create a new public interface with the given metadata.
    pub fn new(metadata: Metadata) -> Self {
        Self {
            metadata,
            entrypoints: Vec::new(),
            modules: Vec::new(),
            assets: Vec::new(),
        }
    }

    /// Add an entrypoint to this interface.
    pub fn add_entrypoint(&mut self, entrypoint: Entrypoint) {
        self.entrypoints.push(entrypoint);
    }

    /// Add a module to this interface.
    pub fn add_module(&mut self, module: Module) {
        self.modules.push(module);
    }

    /// Add an asset to this interface.
    pub fn add_asset(&mut self, asset: Asset) {
        self.assets.push(asset);
    }

    /// Find a module by path.
    pub fn find_module(&self, path: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.path == path)
    }

    /// Find a module by path (mutable).
    pub fn find_module_mut(&mut self, path: &str) -> Option<&mut Module> {
        self.modules.iter_mut().find(|m| m.path == path)
    }

    /// Get all exports across all modules.
    pub fn all_exports(&self) -> impl Iterator<Item = (&Module, &Export)> {
        self.modules
            .iter()
            .flat_map(|m| m.exports.iter().map(move |e| (m, e)))
    }

    /// Get the total number of exports.
    pub fn export_count(&self) -> usize {
        self.modules.iter().map(|m| m.exports.len()).sum()
    }
}
