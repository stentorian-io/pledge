//! Module and export definitions for public interface representation.

use serde::{Deserialize, Serialize};

use crate::signature::Signature;

/// An export from a module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Export {
    /// The exported name (may differ from original name if renamed)
    pub name: String,
    /// Original name if this is a renamed export
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    /// The signature of the exported item
    pub signature: Signature,
    /// Whether this is a default export
    #[serde(default)]
    pub is_default: bool,
}

/// A re-export from another module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReExport {
    /// The source module path
    pub source: String,
    /// Specific names being re-exported (empty means all/wildcard)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<ReExportName>,
    /// Whether this is a wildcard re-export (export * from)
    #[serde(default)]
    pub is_wildcard: bool,
}

/// A single name in a re-export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReExportName {
    /// Original name from source module
    pub name: String,
    /// Renamed export name (if different)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

/// A module representing a single source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    /// Module path/identifier (relative to package root)
    pub path: String,
    /// Direct exports from this module
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<Export>,
    /// Re-exports from other modules
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub re_exports: Vec<ReExport>,
    /// Documentation comment for the module
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrypoint {
    /// The entrypoint name/key (e.g., ".", "./utils", "main")
    pub name: String,
    /// Path to the module file
    pub module_path: String,
    /// Conditions for this entrypoint (e.g., "import", "require", "types")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<String>,
}

/// A package asset (non-code file included in distribution).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// Path relative to package root
    pub path: String,
    /// File extension (e.g., "css", "png", "json")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
}

/// Metadata about the package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    /// Package name
    pub name: String,
    /// Package version
    pub version: String,
    /// Package description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Repository URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// License
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Language of the source code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// The complete public interface of a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicInterface {
    /// Package metadata
    pub metadata: Metadata,
    /// Package entrypoints
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entrypoints: Vec<Entrypoint>,
    /// All modules in the package
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<Module>,
    /// Non-code assets included in the package
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
