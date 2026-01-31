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
pub mod module;
pub mod signature;
pub mod types;

// Re-export all public types for convenience
pub use binary::{decode, encode, is_valid_format, BinaryError, FormatInfo, MAGIC, FORMAT_VERSION};
pub use module::{Asset, Entrypoint, Export, Metadata, Module, PublicInterface, ReExport, ReExportName};
pub use signature::{
    ClassSig, EnumMember, EnumSig, FunctionSig, IndexSignature, InterfaceSig, NamespaceSig,
    Parameter, Property, Signature, TypeAliasSig, VariableSig, Visibility,
};
pub use types::{TypeInfo, TypeKind, TypeParameter};
