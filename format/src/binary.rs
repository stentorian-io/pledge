//! Binary serialization format for interlint public interfaces.
//!
//! This module provides a compact binary format using MessagePack with:
//! - Magic bytes header ("ILMJ" - InterLint MessagePack JSON)
//! - Version field for forward compatibility
//! - CRC32 checksum for data integrity
//!
//! # Format Layout
//!
//! ```text
//! +--------+--------+--------+--------+--------+--------+--------+--...--+
//! | MAGIC (4 bytes) | VER(2) | CRC32 (4 bytes) |   MessagePack Data     |
//! +--------+--------+--------+--------+--------+--------+--------+--...--+
//! | 'I' 'L' 'M' 'J' | u16 LE |    u32 LE       |   Variable length      |
//! +--------+--------+--------+--------+--------+--------+--------+--...--+
//! ```
//!
//! # Example
//!
//! ```rust
//! use interlint_format::{PublicInterface, Metadata, binary};
//!
//! let metadata = Metadata {
//!     name: "my-package".to_string(),
//!     version: "1.0.0".to_string(),
//!     description: None,
//!     repository: None,
//!     license: None,
//!     language: Some("typescript".to_string()),
//! };
//!
//! let interface = PublicInterface::new(metadata);
//!
//! // Encode to binary
//! let bytes = binary::encode(&interface).unwrap();
//!
//! // Check format validity
//! assert!(binary::is_valid_format(&bytes));
//!
//! // Decode back
//! let decoded: PublicInterface = binary::decode(&bytes).unwrap();
//! assert_eq!(decoded.metadata.name, "my-package");
//! ```

use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

/// Magic bytes identifying the interlint binary format.
pub const MAGIC: &[u8; 4] = b"ILMJ";

/// Current format version.
pub const FORMAT_VERSION: u16 = 1;

/// Header size in bytes (magic + version + checksum).
pub const HEADER_SIZE: usize = 4 + 2 + 4;

/// Errors that can occur during binary serialization/deserialization.
#[derive(Debug, Error)]
pub enum BinaryError {
    /// Data is too short to contain a valid header.
    #[error("data too short: expected at least {HEADER_SIZE} bytes, got {0}")]
    DataTooShort(usize),

    /// Magic bytes don't match expected value.
    #[error("invalid magic bytes: expected ILMJ, got {0:?}")]
    InvalidMagic([u8; 4]),

    /// Format version is not supported.
    #[error("unsupported format version: {0} (supported: {FORMAT_VERSION})")]
    UnsupportedVersion(u16),

    /// Checksum verification failed.
    #[error("checksum mismatch: expected {expected:#010x}, got {actual:#010x}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    /// MessagePack serialization error.
    #[error("serialization error: {0}")]
    Serialize(#[from] rmp_serde::encode::Error),

    /// MessagePack deserialization error.
    #[error("deserialization error: {0}")]
    Deserialize(#[from] rmp_serde::decode::Error),
}

/// Result type for binary operations.
pub type Result<T> = std::result::Result<T, BinaryError>;

/// Encode a value to the binary format.
///
/// The output includes:
/// - 4-byte magic header ("ILMJ")
/// - 2-byte version (little-endian)
/// - 4-byte CRC32 checksum of the payload (little-endian)
/// - MessagePack-encoded payload
///
/// # Errors
///
/// Returns `BinaryError::Serialize` if MessagePack serialization fails.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    // Serialize payload first
    let payload = rmp_serde::to_vec_named(value)?;

    // Calculate checksum of payload
    let checksum = crc32fast::hash(&payload);

    // Build complete buffer
    let mut buffer = Vec::with_capacity(HEADER_SIZE + payload.len());

    // Write header
    buffer.extend_from_slice(MAGIC);
    buffer.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    buffer.extend_from_slice(&checksum.to_le_bytes());

    // Write payload
    buffer.extend_from_slice(&payload);

    Ok(buffer)
}

/// Decode a value from the binary format.
///
/// This function:
/// 1. Validates the magic bytes
/// 2. Checks the format version
/// 3. Verifies the CRC32 checksum
/// 4. Deserializes the MessagePack payload
///
/// # Errors
///
/// - `BinaryError::DataTooShort` if data is smaller than header size
/// - `BinaryError::InvalidMagic` if magic bytes don't match
/// - `BinaryError::UnsupportedVersion` if version is not supported
/// - `BinaryError::ChecksumMismatch` if checksum verification fails
/// - `BinaryError::Deserialize` if MessagePack deserialization fails
pub fn decode<T: DeserializeOwned>(data: &[u8]) -> Result<T> {
    // Check minimum size
    if data.len() < HEADER_SIZE {
        return Err(BinaryError::DataTooShort(data.len()));
    }

    // Validate magic bytes
    let magic: [u8; 4] = data[0..4].try_into().unwrap();
    if &magic != MAGIC {
        return Err(BinaryError::InvalidMagic(magic));
    }

    // Check version
    let version = u16::from_le_bytes(data[4..6].try_into().unwrap());
    if version != FORMAT_VERSION {
        return Err(BinaryError::UnsupportedVersion(version));
    }

    // Extract and verify checksum
    let stored_checksum = u32::from_le_bytes(data[6..10].try_into().unwrap());
    let payload = &data[HEADER_SIZE..];
    let actual_checksum = crc32fast::hash(payload);

    if stored_checksum != actual_checksum {
        return Err(BinaryError::ChecksumMismatch {
            expected: stored_checksum,
            actual: actual_checksum,
        });
    }

    // Deserialize payload
    let value = rmp_serde::from_slice(payload)?;
    Ok(value)
}

/// Check if data appears to be in the valid binary format.
///
/// This performs a quick validation by checking:
/// - Minimum size requirement
/// - Magic bytes
/// - Version number
/// - CRC32 checksum
///
/// Note: This does not validate that the payload can be deserialized
/// to any particular type.
pub fn is_valid_format(data: &[u8]) -> bool {
    // Check minimum size
    if data.len() < HEADER_SIZE {
        return false;
    }

    // Check magic bytes
    if &data[0..4] != MAGIC {
        return false;
    }

    // Check version
    let version = u16::from_le_bytes(data[4..6].try_into().unwrap());
    if version != FORMAT_VERSION {
        return false;
    }

    // Verify checksum
    let stored_checksum = u32::from_le_bytes(data[6..10].try_into().unwrap());
    let payload = &data[HEADER_SIZE..];
    let actual_checksum = crc32fast::hash(payload);

    stored_checksum == actual_checksum
}

/// Get format information from binary data without full deserialization.
///
/// Returns `None` if the data is too short or has invalid magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatInfo {
    /// Format version
    pub version: u16,
    /// Stored CRC32 checksum
    pub checksum: u32,
    /// Payload size in bytes
    pub payload_size: usize,
    /// Total size in bytes
    pub total_size: usize,
}

/// Extract format information from binary data.
///
/// This is useful for inspecting files without fully deserializing them.
pub fn get_format_info(data: &[u8]) -> Option<FormatInfo> {
    if data.len() < HEADER_SIZE {
        return None;
    }

    if &data[0..4] != MAGIC {
        return None;
    }

    let version = u16::from_le_bytes(data[4..6].try_into().unwrap());
    let checksum = u32::from_le_bytes(data[6..10].try_into().unwrap());

    Some(FormatInfo {
        version,
        checksum,
        payload_size: data.len() - HEADER_SIZE,
        total_size: data.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ClassSig, EnumMember, EnumSig, Export, FunctionSig, InterfaceSig, Metadata, Module,
        NamespaceSig, Parameter, Property, PublicInterface, Signature, TypeAliasSig, TypeInfo,
        TypeKind, TypeParameter, VariableSig, Visibility,
    };

    fn sample_interface() -> PublicInterface {
        let metadata = Metadata {
            name: "test-package".to_string(),
            version: "1.0.0".to_string(),
            description: Some("A test package".to_string()),
            repository: Some("https://github.com/test/test".to_string()),
            license: Some("MIT".to_string()),
            language: Some("typescript".to_string()),
        };

        let mut interface = PublicInterface::new(metadata);

        let mut module = Module::new("src/index.ts");
        module.add_export(Export {
            name: "greet".to_string(),
            original_name: None,
            signature: Signature::Function(FunctionSig {
                name: "greet".to_string(),
                type_parameters: vec![],
                parameters: vec![],
                return_type: TypeInfo::primitive("string"),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: Some("Returns a greeting".to_string()),
            }),
            is_default: false,
        });

        interface.add_module(module);
        interface
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let interface = sample_interface();

        let encoded = encode(&interface).expect("encoding should succeed");
        let decoded: PublicInterface = decode(&encoded).expect("decoding should succeed");

        assert_eq!(interface, decoded);
    }

    #[test]
    fn test_is_valid_format() {
        let interface = sample_interface();
        let encoded = encode(&interface).unwrap();

        assert!(is_valid_format(&encoded));
    }

    #[test]
    fn test_invalid_magic() {
        let mut data = encode(&sample_interface()).unwrap();
        data[0] = b'X';

        assert!(!is_valid_format(&data));
        assert!(matches!(
            decode::<PublicInterface>(&data),
            Err(BinaryError::InvalidMagic(_))
        ));
    }

    #[test]
    fn test_invalid_version() {
        let mut data = encode(&sample_interface()).unwrap();
        // Set version to 99
        data[4] = 99;
        data[5] = 0;

        assert!(!is_valid_format(&data));
        assert!(matches!(
            decode::<PublicInterface>(&data),
            Err(BinaryError::UnsupportedVersion(99))
        ));
    }

    #[test]
    fn test_checksum_mismatch() {
        let mut data = encode(&sample_interface()).unwrap();
        // Corrupt the payload
        if data.len() > HEADER_SIZE {
            data[HEADER_SIZE] ^= 0xFF;
        }

        assert!(!is_valid_format(&data));
        assert!(matches!(
            decode::<PublicInterface>(&data),
            Err(BinaryError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn test_data_too_short() {
        let short_data = vec![0u8; 5];

        assert!(!is_valid_format(&short_data));
        assert!(matches!(
            decode::<PublicInterface>(&short_data),
            Err(BinaryError::DataTooShort(5))
        ));
    }

    #[test]
    fn test_empty_data() {
        let empty: Vec<u8> = vec![];

        assert!(!is_valid_format(&empty));
        assert!(matches!(
            decode::<PublicInterface>(&empty),
            Err(BinaryError::DataTooShort(0))
        ));
    }

    #[test]
    fn test_get_format_info() {
        let interface = sample_interface();
        let encoded = encode(&interface).unwrap();

        let info = get_format_info(&encoded).expect("should get format info");
        assert_eq!(info.version, FORMAT_VERSION);
        assert_eq!(info.total_size, encoded.len());
        assert_eq!(info.payload_size, encoded.len() - HEADER_SIZE);
    }

    #[test]
    fn test_header_structure() {
        let interface = sample_interface();
        let encoded = encode(&interface).unwrap();

        // Verify magic bytes
        assert_eq!(&encoded[0..4], b"ILMJ");

        // Verify version
        let version = u16::from_le_bytes(encoded[4..6].try_into().unwrap());
        assert_eq!(version, FORMAT_VERSION);

        // Verify checksum matches payload
        let stored_checksum = u32::from_le_bytes(encoded[6..10].try_into().unwrap());
        let actual_checksum = crc32fast::hash(&encoded[HEADER_SIZE..]);
        assert_eq!(stored_checksum, actual_checksum);
    }

    #[test]
    fn test_compact_size() {
        let interface = sample_interface();
        let encoded = encode(&interface).unwrap();

        // Binary format should be reasonably compact
        // Header is 10 bytes, payload should be compact MessagePack
        assert!(encoded.len() < 500, "encoded size should be compact");
        assert!(encoded.len() > HEADER_SIZE, "should have payload");
    }

    #[test]
    fn test_all_signature_types() {
        let metadata = Metadata {
            name: "all-types".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            license: None,
            language: None,
        };

        let mut interface = PublicInterface::new(metadata);
        let mut module = Module::new("src/all.ts");

        // Function
        module.add_export(Export {
            name: "func".to_string(),
            original_name: None,
            signature: Signature::Function(FunctionSig {
                name: "func".to_string(),
                type_parameters: vec![TypeParameter {
                    name: "T".to_string(),
                    constraint: Some(Box::new(TypeInfo::reference("Comparable"))),
                    default: None,
                }],
                parameters: vec![Parameter {
                    name: "value".to_string(),
                    type_info: TypeInfo::reference("T"),
                    optional: true,
                    rest: false,
                    default_value: Some("null".to_string()),
                }],
                return_type: TypeInfo::void(),
                is_async: true,
                is_generator: false,
                visibility: Visibility::Public,
                doc: Some("A test function".to_string()),
            }),
            is_default: false,
        });

        // Class
        module.add_export(Export {
            name: "MyClass".to_string(),
            original_name: None,
            signature: Signature::Class(ClassSig {
                name: "MyClass".to_string(),
                type_parameters: vec![],
                extends: Some(TypeInfo::reference("BaseClass")),
                implements: vec![TypeInfo::reference("Interface1")],
                properties: vec![Property {
                    name: "prop".to_string(),
                    type_info: TypeInfo::primitive("string"),
                    optional: false,
                    readonly: true,
                    is_static: false,
                    visibility: Visibility::Private,
                    doc: None,
                }],
                methods: vec![],
                constructor: None,
                is_abstract: false,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        // Variable
        module.add_export(Export {
            name: "MY_CONST".to_string(),
            original_name: None,
            signature: Signature::Variable(VariableSig {
                name: "MY_CONST".to_string(),
                type_info: TypeInfo::primitive("number"),
                is_const: true,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        // Interface
        module.add_export(Export {
            name: "IConfig".to_string(),
            original_name: None,
            signature: Signature::Interface(InterfaceSig {
                name: "IConfig".to_string(),
                type_parameters: vec![],
                extends: vec![],
                properties: vec![],
                methods: vec![],
                index_signature: None,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        // Enum
        module.add_export(Export {
            name: "Status".to_string(),
            original_name: None,
            signature: Signature::Enum(EnumSig {
                name: "Status".to_string(),
                members: vec![
                    EnumMember {
                        name: "Active".to_string(),
                        value: Some("1".to_string()),
                        doc: None,
                    },
                    EnumMember {
                        name: "Inactive".to_string(),
                        value: Some("0".to_string()),
                        doc: None,
                    },
                ],
                is_const: true,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        // TypeAlias
        module.add_export(Export {
            name: "ID".to_string(),
            original_name: None,
            signature: Signature::TypeAlias(TypeAliasSig {
                name: "ID".to_string(),
                type_parameters: vec![],
                aliased_type: TypeInfo::primitive("string"),
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        // Namespace
        module.add_export(Export {
            name: "Utils".to_string(),
            original_name: None,
            signature: Signature::Namespace(NamespaceSig {
                name: "Utils".to_string(),
                exports: vec![Signature::Variable(VariableSig {
                    name: "VERSION".to_string(),
                    type_info: TypeInfo::primitive("string"),
                    is_const: true,
                    visibility: Visibility::Public,
                    doc: None,
                })],
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        interface.add_module(module);

        // Roundtrip test
        let encoded = encode(&interface).expect("should encode all types");
        assert!(is_valid_format(&encoded));
        let decoded: PublicInterface = decode(&encoded).expect("should decode all types");
        assert_eq!(interface, decoded);
    }

    #[test]
    fn test_all_type_kinds() {
        let metadata = Metadata {
            name: "type-kinds".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            license: None,
            language: None,
        };

        let mut interface = PublicInterface::new(metadata);
        let mut module = Module::new("types.ts");

        let type_kinds = vec![
            TypeKind::Primitive,
            TypeKind::Array,
            TypeKind::Tuple,
            TypeKind::Object,
            TypeKind::Function,
            TypeKind::Union,
            TypeKind::Intersection,
            TypeKind::Generic,
            TypeKind::Reference,
            TypeKind::Literal,
            TypeKind::Optional,
            TypeKind::Map,
            TypeKind::Set,
            TypeKind::Async,
            TypeKind::Void,
            TypeKind::Never,
            TypeKind::Any,
            TypeKind::Unknown,
        ];

        for (i, kind) in type_kinds.into_iter().enumerate() {
            module.add_export(Export {
                name: format!("type_{i}"),
                original_name: None,
                signature: Signature::Variable(VariableSig {
                    name: format!("type_{i}"),
                    type_info: TypeInfo {
                        kind,
                        repr: format!("type_{i}"),
                        type_arguments: vec![],
                        constituents: vec![],
                        element_type: None,
                        key_type: None,
                        value_type: None,
                        param_types: vec![],
                        return_type: None,
                        nullable: false,
                    },
                    is_const: true,
                    visibility: Visibility::Public,
                    doc: None,
                }),
                is_default: false,
            });
        }

        interface.add_module(module);

        let encoded = encode(&interface).expect("should encode all type kinds");
        let decoded: PublicInterface = decode(&encoded).expect("should decode all type kinds");
        assert_eq!(interface, decoded);
    }

    #[test]
    fn test_complex_nested_types() {
        let metadata = Metadata {
            name: "nested".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            license: None,
            language: None,
        };

        let mut interface = PublicInterface::new(metadata);
        let mut module = Module::new("nested.ts");

        // Create a deeply nested type: Map<string, Array<Option<number>>>
        let nested_type = TypeInfo {
            kind: TypeKind::Map,
            repr: "Map<string, Array<number | null>>".to_string(),
            type_arguments: vec![],
            constituents: vec![],
            element_type: None,
            key_type: Some(Box::new(TypeInfo::primitive("string"))),
            value_type: Some(Box::new(TypeInfo {
                kind: TypeKind::Array,
                repr: "Array<number | null>".to_string(),
                type_arguments: vec![],
                constituents: vec![],
                element_type: Some(Box::new(TypeInfo {
                    kind: TypeKind::Union,
                    repr: "number | null".to_string(),
                    type_arguments: vec![],
                    constituents: vec![
                        TypeInfo::primitive("number"),
                        TypeInfo {
                            kind: TypeKind::Literal,
                            repr: "null".to_string(),
                            type_arguments: vec![],
                            constituents: vec![],
                            element_type: None,
                            key_type: None,
                            value_type: None,
                            param_types: vec![],
                            return_type: None,
                            nullable: true,
                        },
                    ],
                    element_type: None,
                    key_type: None,
                    value_type: None,
                    param_types: vec![],
                    return_type: None,
                    nullable: true,
                })),
                key_type: None,
                value_type: None,
                param_types: vec![],
                return_type: None,
                nullable: false,
            })),
            param_types: vec![],
            return_type: None,
            nullable: false,
        };

        module.add_export(Export {
            name: "nestedMap".to_string(),
            original_name: None,
            signature: Signature::Variable(VariableSig {
                name: "nestedMap".to_string(),
                type_info: nested_type,
                is_const: true,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        interface.add_module(module);

        let encoded = encode(&interface).expect("should encode nested types");
        let decoded: PublicInterface = decode(&encoded).expect("should decode nested types");
        assert_eq!(interface, decoded);
    }

    #[test]
    fn test_unicode_strings() {
        let metadata = Metadata {
            name: "unicode-test".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Unicode test: Hello World".to_string()),
            repository: None,
            license: None,
            language: None,
        };

        let mut interface = PublicInterface::new(metadata);
        let mut module = Module::new("src/unicode.ts");

        module.add_export(Export {
            name: "greet".to_string(),
            original_name: None,
            signature: Signature::Function(FunctionSig {
                name: "greet".to_string(),
                type_parameters: vec![],
                parameters: vec![],
                return_type: TypeInfo::primitive("string"),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: Some("Returns greeting in multiple languages".to_string()),
            }),
            is_default: false,
        });

        interface.add_module(module);

        let encoded = encode(&interface).expect("should encode unicode");
        let decoded: PublicInterface = decode(&encoded).expect("should decode unicode");
        assert_eq!(interface, decoded);
        assert_eq!(
            decoded.metadata.description,
            Some("Unicode test: Hello World".to_string())
        );
    }

    #[test]
    fn test_empty_interface() {
        let metadata = Metadata {
            name: "empty".to_string(),
            version: "0.0.1".to_string(),
            description: None,
            repository: None,
            license: None,
            language: None,
        };

        let interface = PublicInterface::new(metadata);

        let encoded = encode(&interface).expect("should encode empty interface");
        assert!(is_valid_format(&encoded));
        let decoded: PublicInterface = decode(&encoded).expect("should decode empty interface");
        assert_eq!(interface, decoded);
        assert!(decoded.modules.is_empty());
        assert!(decoded.entrypoints.is_empty());
    }

    #[test]
    fn test_large_interface() {
        let metadata = Metadata {
            name: "large".to_string(),
            version: "1.0.0".to_string(),
            description: Some("A large interface for stress testing".to_string()),
            repository: None,
            license: None,
            language: Some("typescript".to_string()),
        };

        let mut interface = PublicInterface::new(metadata);

        // Add 100 modules with 10 exports each
        for m in 0..100 {
            let mut module = Module::new(format!("src/module_{m}.ts"));

            for e in 0..10 {
                module.add_export(Export {
                    name: format!("export_{m}_{e}"),
                    original_name: None,
                    signature: Signature::Function(FunctionSig {
                        name: format!("export_{m}_{e}"),
                        type_parameters: vec![],
                        parameters: vec![Parameter {
                            name: "input".to_string(),
                            type_info: TypeInfo::primitive("string"),
                            optional: false,
                            rest: false,
                            default_value: None,
                        }],
                        return_type: TypeInfo::primitive("void"),
                        is_async: false,
                        is_generator: false,
                        visibility: Visibility::Public,
                        doc: Some(format!("Function {e} in module {m}")),
                    }),
                    is_default: false,
                });
            }

            interface.add_module(module);
        }

        assert_eq!(interface.export_count(), 1000);

        let encoded = encode(&interface).expect("should encode large interface");
        assert!(is_valid_format(&encoded));

        let decoded: PublicInterface = decode(&encoded).expect("should decode large interface");
        assert_eq!(interface, decoded);
        assert_eq!(decoded.export_count(), 1000);
    }

    // ==================== Additional Edge Case Tests ====================

    #[test]
    fn test_deeply_nested_array_types() {
        // Test deeply nested arrays: string[][][][][][]
        let mut type_info = TypeInfo::primitive("string");
        for _ in 0..10 {
            type_info = TypeInfo::array(type_info);
        }

        let var = VariableSig {
            name: "deepArray".to_string(),
            type_info,
            is_const: true,
            visibility: Visibility::Public,
            doc: None,
        };

        let encoded = encode(&var).expect("should encode deeply nested array");
        let decoded: VariableSig = decode(&encoded).expect("should decode deeply nested array");
        assert_eq!(var, decoded);
    }

    #[test]
    fn test_all_visibility_levels() {
        let visibilities = vec![
            Visibility::Public,
            Visibility::Protected,
            Visibility::Private,
            Visibility::Internal,
        ];

        for vis in visibilities {
            let var = VariableSig {
                name: format!("{:?}_var", vis),
                type_info: TypeInfo::primitive("string"),
                is_const: false,
                visibility: vis.clone(),
                doc: None,
            };

            let encoded = encode(&var).expect("should encode visibility");
            let decoded: VariableSig = decode(&encoded).expect("should decode visibility");
            assert_eq!(var.visibility, decoded.visibility);
        }
    }

    #[test]
    fn test_empty_strings() {
        let metadata = Metadata {
            name: "".to_string(),
            version: "".to_string(),
            description: Some("".to_string()),
            repository: Some("".to_string()),
            license: Some("".to_string()),
            language: Some("".to_string()),
        };

        let interface = PublicInterface::new(metadata);
        let encoded = encode(&interface).expect("should encode empty strings");
        let decoded: PublicInterface = decode(&encoded).expect("should decode empty strings");
        assert_eq!(interface, decoded);
    }

    #[test]
    fn test_very_long_strings() {
        let long_doc = "x".repeat(100_000);
        let func = FunctionSig {
            name: "longDoc".to_string(),
            type_parameters: vec![],
            parameters: vec![],
            return_type: TypeInfo::void(),
            is_async: false,
            is_generator: false,
            visibility: Visibility::Public,
            doc: Some(long_doc.clone()),
        };

        let encoded = encode(&func).expect("should encode long string");
        let decoded: FunctionSig = decode(&encoded).expect("should decode long string");
        assert_eq!(decoded.doc.as_ref().unwrap().len(), 100_000);
    }

    #[test]
    fn test_many_type_parameters() {
        let type_params: Vec<TypeParameter> = (0..50)
            .map(|i| TypeParameter {
                name: format!("T{}", i),
                constraint: if i % 2 == 0 {
                    Some(Box::new(TypeInfo::reference("object")))
                } else {
                    None
                },
                default: if i % 3 == 0 {
                    Some(Box::new(TypeInfo::any()))
                } else {
                    None
                },
            })
            .collect();

        let func = FunctionSig {
            name: "manyGenerics".to_string(),
            type_parameters: type_params.clone(),
            parameters: vec![],
            return_type: TypeInfo::void(),
            is_async: false,
            is_generator: false,
            visibility: Visibility::Public,
            doc: None,
        };

        let encoded = encode(&func).expect("should encode many type params");
        let decoded: FunctionSig = decode(&encoded).expect("should decode many type params");
        assert_eq!(decoded.type_parameters.len(), 50);
    }

    #[test]
    fn test_many_parameters() {
        let params: Vec<Parameter> = (0..100)
            .map(|i| Parameter {
                name: format!("param{}", i),
                type_info: TypeInfo::primitive("any"),
                optional: i % 2 == 0,
                rest: i == 99,
                default_value: if i % 3 == 0 {
                    Some(format!("default{}", i))
                } else {
                    None
                },
            })
            .collect();

        let func = FunctionSig {
            name: "manyParams".to_string(),
            type_parameters: vec![],
            parameters: params,
            return_type: TypeInfo::void(),
            is_async: false,
            is_generator: false,
            visibility: Visibility::Public,
            doc: None,
        };

        let encoded = encode(&func).expect("should encode many params");
        let decoded: FunctionSig = decode(&encoded).expect("should decode many params");
        assert_eq!(decoded.parameters.len(), 100);
    }

    #[test]
    fn test_generator_function() {
        let func = FunctionSig {
            name: "numberGenerator".to_string(),
            type_parameters: vec![],
            parameters: vec![],
            return_type: TypeInfo {
                kind: TypeKind::Generic,
                repr: "Generator<number>".to_string(),
                type_arguments: vec![TypeInfo::primitive("number")],
                constituents: vec![],
                element_type: None,
                key_type: None,
                value_type: None,
                param_types: vec![],
                return_type: None,
                nullable: false,
            },
            is_async: false,
            is_generator: true,
            visibility: Visibility::Public,
            doc: None,
        };

        let encoded = encode(&func).expect("should encode generator");
        let decoded: FunctionSig = decode(&encoded).expect("should decode generator");
        assert!(decoded.is_generator);
        assert!(!decoded.is_async);
    }

    #[test]
    fn test_async_generator_function() {
        let func = FunctionSig {
            name: "asyncGenerator".to_string(),
            type_parameters: vec![],
            parameters: vec![],
            return_type: TypeInfo {
                kind: TypeKind::Generic,
                repr: "AsyncGenerator<number>".to_string(),
                type_arguments: vec![TypeInfo::primitive("number")],
                constituents: vec![],
                element_type: None,
                key_type: None,
                value_type: None,
                param_types: vec![],
                return_type: None,
                nullable: false,
            },
            is_async: true,
            is_generator: true,
            visibility: Visibility::Public,
            doc: None,
        };

        let encoded = encode(&func).expect("should encode async generator");
        let decoded: FunctionSig = decode(&encoded).expect("should decode async generator");
        assert!(decoded.is_async);
        assert!(decoded.is_generator);
    }

    #[test]
    fn test_abstract_class() {
        let class = ClassSig {
            name: "AbstractBase".to_string(),
            type_parameters: vec![],
            extends: None,
            implements: vec![],
            properties: vec![],
            methods: vec![FunctionSig {
                name: "abstractMethod".to_string(),
                type_parameters: vec![],
                parameters: vec![],
                return_type: TypeInfo::void(),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: None,
            }],
            constructor: None,
            is_abstract: true,
            visibility: Visibility::Public,
            doc: None,
        };

        let encoded = encode(&class).expect("should encode abstract class");
        let decoded: ClassSig = decode(&encoded).expect("should decode abstract class");
        assert!(decoded.is_abstract);
    }

    #[test]
    fn test_class_with_constructor() {
        let class = ClassSig {
            name: "MyClass".to_string(),
            type_parameters: vec![],
            extends: None,
            implements: vec![],
            properties: vec![],
            methods: vec![],
            constructor: Some(Box::new(FunctionSig {
                name: "constructor".to_string(),
                type_parameters: vec![],
                parameters: vec![
                    Parameter {
                        name: "name".to_string(),
                        type_info: TypeInfo::primitive("string"),
                        optional: false,
                        rest: false,
                        default_value: None,
                    },
                    Parameter {
                        name: "age".to_string(),
                        type_info: TypeInfo::primitive("number"),
                        optional: true,
                        rest: false,
                        default_value: Some("0".to_string()),
                    },
                ],
                return_type: TypeInfo::void(),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: Some("Creates a new instance".to_string()),
            })),
            is_abstract: false,
            visibility: Visibility::Public,
            doc: None,
        };

        let encoded = encode(&class).expect("should encode class with constructor");
        let decoded: ClassSig = decode(&encoded).expect("should decode class with constructor");
        assert!(decoded.constructor.is_some());
        assert_eq!(decoded.constructor.unwrap().parameters.len(), 2);
    }

    #[test]
    fn test_interface_with_index_signature() {
        use crate::IndexSignature;

        let iface = InterfaceSig {
            name: "StringDict".to_string(),
            type_parameters: vec![],
            extends: vec![],
            properties: vec![],
            methods: vec![],
            index_signature: Some(IndexSignature {
                key_type: TypeInfo::primitive("string"),
                value_type: TypeInfo::any(),
                readonly: true,
            }),
            visibility: Visibility::Public,
            doc: None,
        };

        let encoded = encode(&iface).expect("should encode interface with index");
        let decoded: InterfaceSig = decode(&encoded).expect("should decode interface with index");
        assert!(decoded.index_signature.is_some());
        let idx = decoded.index_signature.unwrap();
        assert!(idx.readonly);
    }

    #[test]
    fn test_interface_extends_multiple() {
        let iface = InterfaceSig {
            name: "Combined".to_string(),
            type_parameters: vec![],
            extends: vec![
                TypeInfo::reference("Base1"),
                TypeInfo::reference("Base2"),
                TypeInfo::reference("Base3"),
            ],
            properties: vec![],
            methods: vec![],
            index_signature: None,
            visibility: Visibility::Public,
            doc: None,
        };

        let encoded = encode(&iface).expect("should encode multi-extends");
        let decoded: InterfaceSig = decode(&encoded).expect("should decode multi-extends");
        assert_eq!(decoded.extends.len(), 3);
    }

    #[test]
    fn test_const_enum() {
        let enum_sig = EnumSig {
            name: "Direction".to_string(),
            members: vec![
                EnumMember {
                    name: "Up".to_string(),
                    value: None,
                    doc: None,
                },
                EnumMember {
                    name: "Down".to_string(),
                    value: None,
                    doc: Some("Go down".to_string()),
                },
            ],
            is_const: true,
            visibility: Visibility::Public,
            doc: None,
        };

        let encoded = encode(&enum_sig).expect("should encode const enum");
        let decoded: EnumSig = decode(&encoded).expect("should decode const enum");
        assert!(decoded.is_const);
    }

    #[test]
    fn test_wildcard_reexport() {
        use crate::ReExport;

        let mut module = Module::new("index.ts");
        module.add_re_export(ReExport {
            source: "./utils".to_string(),
            names: vec![],
            is_wildcard: true,
        });

        let encoded = encode(&module).expect("should encode wildcard reexport");
        let decoded: Module = decode(&encoded).expect("should decode wildcard reexport");
        assert!(decoded.re_exports[0].is_wildcard);
        assert!(decoded.re_exports[0].names.is_empty());
    }

    #[test]
    fn test_named_reexport_with_alias() {
        use crate::{ReExport, ReExportName};

        let mut module = Module::new("index.ts");
        module.add_re_export(ReExport {
            source: "./internal".to_string(),
            names: vec![
                ReExportName {
                    name: "original".to_string(),
                    alias: Some("renamed".to_string()),
                },
                ReExportName {
                    name: "unchanged".to_string(),
                    alias: None,
                },
            ],
            is_wildcard: false,
        });

        let encoded = encode(&module).expect("should encode named reexport");
        let decoded: Module = decode(&encoded).expect("should decode named reexport");
        assert!(!decoded.re_exports[0].is_wildcard);
        assert_eq!(decoded.re_exports[0].names.len(), 2);
        assert_eq!(
            decoded.re_exports[0].names[0].alias,
            Some("renamed".to_string())
        );
    }

    #[test]
    fn test_default_export() {
        let mut module = Module::new("src/component.ts");
        module.add_export(Export {
            name: "default".to_string(),
            original_name: Some("MyComponent".to_string()),
            signature: Signature::Class(ClassSig {
                name: "MyComponent".to_string(),
                type_parameters: vec![],
                extends: None,
                implements: vec![],
                properties: vec![],
                methods: vec![],
                constructor: None,
                is_abstract: false,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: true,
        });

        let encoded = encode(&module).expect("should encode default export");
        let decoded: Module = decode(&encoded).expect("should decode default export");
        assert!(decoded.exports[0].is_default);
        assert_eq!(
            decoded.exports[0].original_name,
            Some("MyComponent".to_string())
        );
    }

    #[test]
    fn test_entrypoint_with_conditions() {
        use crate::Entrypoint;

        let mut interface = PublicInterface::new(Metadata {
            name: "pkg".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            license: None,
            language: None,
        });

        interface.add_entrypoint(Entrypoint {
            name: ".".to_string(),
            module_path: "dist/index.mjs".to_string(),
            conditions: vec![
                "import".to_string(),
                "node".to_string(),
                "types".to_string(),
            ],
        });

        interface.add_entrypoint(Entrypoint {
            name: "./browser".to_string(),
            module_path: "dist/browser.js".to_string(),
            conditions: vec!["browser".to_string(), "default".to_string()],
        });

        let encoded = encode(&interface).expect("should encode entrypoints");
        let decoded: PublicInterface = decode(&encoded).expect("should decode entrypoints");
        assert_eq!(decoded.entrypoints.len(), 2);
        assert_eq!(decoded.entrypoints[0].conditions.len(), 3);
    }

    #[test]
    fn test_asset() {
        use crate::Asset;

        let mut interface = PublicInterface::new(Metadata {
            name: "pkg".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            license: None,
            language: None,
        });

        interface.add_asset(Asset {
            path: "styles/main.css".to_string(),
            extension: Some("css".to_string()),
        });
        interface.add_asset(Asset {
            path: "images/logo.png".to_string(),
            extension: Some("png".to_string()),
        });
        interface.add_asset(Asset {
            path: "data/config".to_string(),
            extension: None,
        });

        let encoded = encode(&interface).expect("should encode assets");
        let decoded: PublicInterface = decode(&encoded).expect("should decode assets");
        assert_eq!(decoded.assets.len(), 3);
        assert_eq!(decoded.assets[0].extension, Some("css".to_string()));
        assert_eq!(decoded.assets[2].extension, None);
    }

    #[test]
    fn test_function_type_info() {
        let func_type = TypeInfo {
            kind: TypeKind::Function,
            repr: "(x: string, y: number) => boolean".to_string(),
            type_arguments: vec![],
            constituents: vec![],
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: vec![TypeInfo::primitive("string"), TypeInfo::primitive("number")],
            return_type: Some(Box::new(TypeInfo::primitive("boolean"))),
            nullable: false,
        };

        let encoded = encode(&func_type).expect("should encode function type");
        let decoded: TypeInfo = decode(&encoded).expect("should decode function type");
        assert_eq!(decoded.kind, TypeKind::Function);
        assert_eq!(decoded.param_types.len(), 2);
        assert!(decoded.return_type.is_some());
    }

    #[test]
    fn test_union_type_with_constituents() {
        let union_type = TypeInfo {
            kind: TypeKind::Union,
            repr: "string | number | null".to_string(),
            type_arguments: vec![],
            constituents: vec![
                TypeInfo::primitive("string"),
                TypeInfo::primitive("number"),
                TypeInfo {
                    kind: TypeKind::Literal,
                    repr: "null".to_string(),
                    type_arguments: vec![],
                    constituents: vec![],
                    element_type: None,
                    key_type: None,
                    value_type: None,
                    param_types: vec![],
                    return_type: None,
                    nullable: true,
                },
            ],
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: vec![],
            return_type: None,
            nullable: true,
        };

        let encoded = encode(&union_type).expect("should encode union type");
        let decoded: TypeInfo = decode(&encoded).expect("should decode union type");
        assert_eq!(decoded.kind, TypeKind::Union);
        assert_eq!(decoded.constituents.len(), 3);
        assert!(decoded.nullable);
    }

    #[test]
    fn test_intersection_type() {
        let intersection_type = TypeInfo {
            kind: TypeKind::Intersection,
            repr: "A & B & C".to_string(),
            type_arguments: vec![],
            constituents: vec![
                TypeInfo::reference("A"),
                TypeInfo::reference("B"),
                TypeInfo::reference("C"),
            ],
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: vec![],
            return_type: None,
            nullable: false,
        };

        let encoded = encode(&intersection_type).expect("should encode intersection");
        let decoded: TypeInfo = decode(&encoded).expect("should decode intersection");
        assert_eq!(decoded.kind, TypeKind::Intersection);
        assert_eq!(decoded.constituents.len(), 3);
    }

    #[test]
    fn test_tuple_type() {
        let tuple_type = TypeInfo {
            kind: TypeKind::Tuple,
            repr: "[string, number, boolean]".to_string(),
            type_arguments: vec![
                TypeInfo::primitive("string"),
                TypeInfo::primitive("number"),
                TypeInfo::primitive("boolean"),
            ],
            constituents: vec![],
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: vec![],
            return_type: None,
            nullable: false,
        };

        let encoded = encode(&tuple_type).expect("should encode tuple");
        let decoded: TypeInfo = decode(&encoded).expect("should decode tuple");
        assert_eq!(decoded.kind, TypeKind::Tuple);
        assert_eq!(decoded.type_arguments.len(), 3);
    }

    #[test]
    fn test_generic_type_with_arguments() {
        let generic_type = TypeInfo {
            kind: TypeKind::Generic,
            repr: "Promise<Result<T, E>>".to_string(),
            type_arguments: vec![TypeInfo {
                kind: TypeKind::Generic,
                repr: "Result<T, E>".to_string(),
                type_arguments: vec![TypeInfo::reference("T"), TypeInfo::reference("E")],
                constituents: vec![],
                element_type: None,
                key_type: None,
                value_type: None,
                param_types: vec![],
                return_type: None,
                nullable: false,
            }],
            constituents: vec![],
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: vec![],
            return_type: None,
            nullable: false,
        };

        let encoded = encode(&generic_type).expect("should encode generic");
        let decoded: TypeInfo = decode(&encoded).expect("should decode generic");
        assert_eq!(decoded.kind, TypeKind::Generic);
        assert_eq!(decoded.type_arguments.len(), 1);
        assert_eq!(decoded.type_arguments[0].type_arguments.len(), 2);
    }

    #[test]
    fn test_set_type() {
        let set_type = TypeInfo {
            kind: TypeKind::Set,
            repr: "Set<string>".to_string(),
            type_arguments: vec![TypeInfo::primitive("string")],
            constituents: vec![],
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: vec![],
            return_type: None,
            nullable: false,
        };

        let encoded = encode(&set_type).expect("should encode set type");
        let decoded: TypeInfo = decode(&encoded).expect("should decode set type");
        assert_eq!(decoded.kind, TypeKind::Set);
    }

    #[test]
    fn test_async_type() {
        let async_type = TypeInfo {
            kind: TypeKind::Async,
            repr: "Promise<void>".to_string(),
            type_arguments: vec![TypeInfo::void()],
            constituents: vec![],
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: vec![],
            return_type: None,
            nullable: false,
        };

        let encoded = encode(&async_type).expect("should encode async type");
        let decoded: TypeInfo = decode(&encoded).expect("should decode async type");
        assert_eq!(decoded.kind, TypeKind::Async);
    }

    #[test]
    fn test_never_type() {
        let never_type = TypeInfo {
            kind: TypeKind::Never,
            repr: "never".to_string(),
            type_arguments: vec![],
            constituents: vec![],
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: vec![],
            return_type: None,
            nullable: false,
        };

        let encoded = encode(&never_type).expect("should encode never type");
        let decoded: TypeInfo = decode(&encoded).expect("should decode never type");
        assert_eq!(decoded.kind, TypeKind::Never);
    }

    #[test]
    fn test_unknown_type() {
        let unknown_type = TypeInfo {
            kind: TypeKind::Unknown,
            repr: "unknown".to_string(),
            type_arguments: vec![],
            constituents: vec![],
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: vec![],
            return_type: None,
            nullable: false,
        };

        let encoded = encode(&unknown_type).expect("should encode unknown type");
        let decoded: TypeInfo = decode(&encoded).expect("should decode unknown type");
        assert_eq!(decoded.kind, TypeKind::Unknown);
    }

    #[test]
    fn test_literal_type() {
        let literal_type = TypeInfo {
            kind: TypeKind::Literal,
            repr: "\"success\"".to_string(),
            type_arguments: vec![],
            constituents: vec![],
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: vec![],
            return_type: None,
            nullable: false,
        };

        let encoded = encode(&literal_type).expect("should encode literal type");
        let decoded: TypeInfo = decode(&encoded).expect("should decode literal type");
        assert_eq!(decoded.kind, TypeKind::Literal);
    }

    #[test]
    fn test_object_type() {
        let object_type = TypeInfo {
            kind: TypeKind::Object,
            repr: "{ name: string; age: number }".to_string(),
            type_arguments: vec![],
            constituents: vec![],
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: vec![],
            return_type: None,
            nullable: false,
        };

        let encoded = encode(&object_type).expect("should encode object type");
        let decoded: TypeInfo = decode(&encoded).expect("should decode object type");
        assert_eq!(decoded.kind, TypeKind::Object);
    }

    // ==================== Error Scenario Tests ====================

    #[test]
    fn test_corrupted_payload_single_byte_flip() {
        let interface = sample_interface();
        let mut data = encode(&interface).unwrap();

        // Flip a single bit in the middle of the payload
        let mid = HEADER_SIZE + (data.len() - HEADER_SIZE) / 2;
        data[mid] ^= 0x01;

        assert!(!is_valid_format(&data));
        assert!(matches!(
            decode::<PublicInterface>(&data),
            Err(BinaryError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn test_truncated_payload() {
        let interface = sample_interface();
        let data = encode(&interface).unwrap();

        // Truncate to just the header
        let truncated = &data[..HEADER_SIZE];

        // Should still have valid header but empty/invalid payload
        // The checksum won't match an empty payload
        assert!(!is_valid_format(truncated));
    }

    #[test]
    fn test_extra_bytes_at_end() {
        let interface = sample_interface();
        let mut data = encode(&interface).unwrap();

        // Append extra bytes
        data.extend_from_slice(b"EXTRA_GARBAGE_DATA");

        // Checksum only covers original payload, so this should fail
        assert!(!is_valid_format(&data));
    }

    #[test]
    fn test_all_zero_bytes() {
        let zeros = vec![0u8; 100];
        assert!(!is_valid_format(&zeros));
        assert!(matches!(
            decode::<PublicInterface>(&zeros),
            Err(BinaryError::InvalidMagic(_))
        ));
    }

    #[test]
    fn test_header_only_no_payload() {
        // Valid header but checksum for empty payload
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.extend_from_slice(&FORMAT_VERSION.to_le_bytes());

        // CRC32 of empty data
        let empty_checksum = crc32fast::hash(&[]);
        data.extend_from_slice(&empty_checksum.to_le_bytes());

        // This should now have valid format
        assert!(is_valid_format(&data));

        // But deserializing to PublicInterface should fail (empty MessagePack)
        let result: Result<PublicInterface> = decode(&data);
        assert!(matches!(result, Err(BinaryError::Deserialize(_))));
    }

    #[test]
    fn test_version_zero() {
        let mut data = encode(&sample_interface()).unwrap();
        // Set version to 0
        data[4] = 0;
        data[5] = 0;

        assert!(!is_valid_format(&data));
        assert!(matches!(
            decode::<PublicInterface>(&data),
            Err(BinaryError::UnsupportedVersion(0))
        ));
    }

    #[test]
    fn test_version_max() {
        let mut data = encode(&sample_interface()).unwrap();
        // Set version to u16::MAX
        data[4] = 0xFF;
        data[5] = 0xFF;

        assert!(!is_valid_format(&data));
        assert!(matches!(
            decode::<PublicInterface>(&data),
            Err(BinaryError::UnsupportedVersion(65535))
        ));
    }

    #[test]
    fn test_magic_partial_match() {
        let mut data = encode(&sample_interface()).unwrap();
        // Only first 2 bytes match
        data[2] = b'X';
        data[3] = b'Y';

        assert!(!is_valid_format(&data));
        let result: Result<PublicInterface> = decode(&data);
        assert!(matches!(result, Err(BinaryError::InvalidMagic(magic)) if magic[2] == b'X'));
    }

    #[test]
    fn test_checksum_zeroed() {
        let mut data = encode(&sample_interface()).unwrap();
        // Zero out the checksum
        data[6] = 0;
        data[7] = 0;
        data[8] = 0;
        data[9] = 0;

        // Unless payload happens to have CRC32 of 0, this should fail
        assert!(!is_valid_format(&data));
        assert!(matches!(
            decode::<PublicInterface>(&data),
            Err(BinaryError::ChecksumMismatch { .. })
        ));
    }

    // ==================== Format Info Tests ====================

    #[test]
    fn test_format_info_invalid_magic() {
        let data = vec![b'X', b'Y', b'Z', b'W', 0, 1, 0, 0, 0, 0];
        assert!(get_format_info(&data).is_none());
    }

    #[test]
    fn test_format_info_short_data() {
        let data = vec![b'I', b'L', b'M', b'J', 0, 1];
        assert!(get_format_info(&data).is_none());
    }

    #[test]
    fn test_format_info_exact_header() {
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.extend_from_slice(&42u16.to_le_bytes()); // version 42
        data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // checksum

        let info = get_format_info(&data).unwrap();
        assert_eq!(info.version, 42);
        assert_eq!(info.checksum, 0xDEADBEEF);
        assert_eq!(info.payload_size, 0);
        assert_eq!(info.total_size, HEADER_SIZE);
    }

    // ==================== Deterministic Encoding Test ====================

    #[test]
    fn test_deterministic_encoding() {
        let interface = sample_interface();

        let encoded1 = encode(&interface).unwrap();
        let encoded2 = encode(&interface).unwrap();

        assert_eq!(encoded1, encoded2, "Encoding should be deterministic");
    }

    // ==================== Special Characters Tests ====================

    #[test]
    fn test_special_characters_in_strings() {
        let metadata = Metadata {
            name: "test\n\t\r\0package".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Contains \"quotes\" and 'apostrophes'".to_string()),
            repository: Some("https://example.com/path?query=value&other=123".to_string()),
            license: Some("MIT/Apache-2.0".to_string()),
            language: None,
        };

        let interface = PublicInterface::new(metadata);
        let encoded = encode(&interface).expect("should encode special chars");
        let decoded: PublicInterface = decode(&encoded).expect("should decode special chars");
        assert_eq!(interface, decoded);
    }

    #[test]
    fn test_json_like_strings() {
        // Test strings that look like JSON
        let func = FunctionSig {
            name: "parseJson".to_string(),
            type_parameters: vec![],
            parameters: vec![],
            return_type: TypeInfo::primitive("object"),
            is_async: false,
            is_generator: false,
            visibility: Visibility::Public,
            doc: Some(r#"{"key": "value", "nested": {"a": [1,2,3]}}"#.to_string()),
        };

        let encoded = encode(&func).expect("should encode JSON-like doc");
        let decoded: FunctionSig = decode(&encoded).expect("should decode JSON-like doc");
        assert_eq!(func, decoded);
    }

    // ==================== Namespace with Nested Signatures ====================

    #[test]
    fn test_nested_namespace() {
        let inner_namespace = NamespaceSig {
            name: "Inner".to_string(),
            exports: vec![Signature::Variable(VariableSig {
                name: "innerVar".to_string(),
                type_info: TypeInfo::primitive("number"),
                is_const: true,
                visibility: Visibility::Public,
                doc: None,
            })],
            visibility: Visibility::Public,
            doc: None,
        };

        let outer_namespace = NamespaceSig {
            name: "Outer".to_string(),
            exports: vec![
                Signature::Namespace(inner_namespace),
                Signature::Function(FunctionSig {
                    name: "outerFunc".to_string(),
                    type_parameters: vec![],
                    parameters: vec![],
                    return_type: TypeInfo::void(),
                    is_async: false,
                    is_generator: false,
                    visibility: Visibility::Public,
                    doc: None,
                }),
            ],
            visibility: Visibility::Public,
            doc: None,
        };

        let encoded = encode(&outer_namespace).expect("should encode nested namespace");
        let decoded: NamespaceSig = decode(&encoded).expect("should decode nested namespace");
        assert_eq!(outer_namespace, decoded);
    }

    // ==================== Complete Class with All Features ====================

    #[test]
    fn test_complete_class() {
        let class = ClassSig {
            name: "CompleteClass".to_string(),
            type_parameters: vec![
                TypeParameter {
                    name: "T".to_string(),
                    constraint: Some(Box::new(TypeInfo::reference("BaseType"))),
                    default: None,
                },
                TypeParameter {
                    name: "U".to_string(),
                    constraint: None,
                    default: Some(Box::new(TypeInfo::primitive("string"))),
                },
            ],
            extends: Some(TypeInfo::reference("ParentClass")),
            implements: vec![
                TypeInfo::reference("Interface1"),
                TypeInfo::reference("Interface2"),
            ],
            properties: vec![
                Property {
                    name: "publicProp".to_string(),
                    type_info: TypeInfo::primitive("string"),
                    optional: false,
                    readonly: false,
                    is_static: false,
                    visibility: Visibility::Public,
                    doc: Some("A public property".to_string()),
                },
                Property {
                    name: "privateProp".to_string(),
                    type_info: TypeInfo::primitive("number"),
                    optional: true,
                    readonly: true,
                    is_static: false,
                    visibility: Visibility::Private,
                    doc: None,
                },
                Property {
                    name: "staticProp".to_string(),
                    type_info: TypeInfo::primitive("boolean"),
                    optional: false,
                    readonly: false,
                    is_static: true,
                    visibility: Visibility::Public,
                    doc: None,
                },
            ],
            methods: vec![
                FunctionSig {
                    name: "publicMethod".to_string(),
                    type_parameters: vec![],
                    parameters: vec![Parameter {
                        name: "arg".to_string(),
                        type_info: TypeInfo::reference("T"),
                        optional: false,
                        rest: false,
                        default_value: None,
                    }],
                    return_type: TypeInfo::reference("U"),
                    is_async: false,
                    is_generator: false,
                    visibility: Visibility::Public,
                    doc: None,
                },
                FunctionSig {
                    name: "asyncMethod".to_string(),
                    type_parameters: vec![],
                    parameters: vec![],
                    return_type: TypeInfo {
                        kind: TypeKind::Async,
                        repr: "Promise<void>".to_string(),
                        type_arguments: vec![],
                        constituents: vec![],
                        element_type: None,
                        key_type: None,
                        value_type: None,
                        param_types: vec![],
                        return_type: None,
                        nullable: false,
                    },
                    is_async: true,
                    is_generator: false,
                    visibility: Visibility::Protected,
                    doc: None,
                },
            ],
            constructor: Some(Box::new(FunctionSig {
                name: "constructor".to_string(),
                type_parameters: vec![],
                parameters: vec![Parameter {
                    name: "config".to_string(),
                    type_info: TypeInfo::reference("Config"),
                    optional: true,
                    rest: false,
                    default_value: Some("{}".to_string()),
                }],
                return_type: TypeInfo::void(),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: Some("Create a new instance".to_string()),
            })),
            is_abstract: false,
            visibility: Visibility::Public,
            doc: Some("A complete class with all features".to_string()),
        };

        let encoded = encode(&class).expect("should encode complete class");
        let decoded: ClassSig = decode(&encoded).expect("should decode complete class");
        assert_eq!(class, decoded);
    }

    // ==================== Module with All Export Types ====================

    #[test]
    fn test_module_with_all_export_types() {
        use crate::{Entrypoint, ReExport, ReExportName};

        let mut interface = PublicInterface::new(Metadata {
            name: "complete-module".to_string(),
            version: "2.0.0".to_string(),
            description: Some("A module with all export types".to_string()),
            repository: Some("https://github.com/test/complete".to_string()),
            license: Some("MIT".to_string()),
            language: Some("typescript".to_string()),
        });

        interface.add_entrypoint(Entrypoint {
            name: ".".to_string(),
            module_path: "src/index.ts".to_string(),
            conditions: vec!["import".to_string(), "types".to_string()],
        });

        let mut module = Module::new("src/index.ts");
        module.doc = Some("Main module documentation".to_string());

        // Add all signature types as exports
        module.add_export(Export {
            name: "myFunction".to_string(),
            original_name: None,
            signature: Signature::Function(FunctionSig {
                name: "myFunction".to_string(),
                type_parameters: vec![],
                parameters: vec![],
                return_type: TypeInfo::void(),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        module.add_export(Export {
            name: "MyClass".to_string(),
            original_name: None,
            signature: Signature::Class(ClassSig {
                name: "MyClass".to_string(),
                type_parameters: vec![],
                extends: None,
                implements: vec![],
                properties: vec![],
                methods: vec![],
                constructor: None,
                is_abstract: false,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        module.add_export(Export {
            name: "MY_VAR".to_string(),
            original_name: None,
            signature: Signature::Variable(VariableSig {
                name: "MY_VAR".to_string(),
                type_info: TypeInfo::primitive("string"),
                is_const: true,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        module.add_export(Export {
            name: "IMyInterface".to_string(),
            original_name: None,
            signature: Signature::Interface(InterfaceSig {
                name: "IMyInterface".to_string(),
                type_parameters: vec![],
                extends: vec![],
                properties: vec![],
                methods: vec![],
                index_signature: None,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        module.add_export(Export {
            name: "MyEnum".to_string(),
            original_name: None,
            signature: Signature::Enum(EnumSig {
                name: "MyEnum".to_string(),
                members: vec![EnumMember {
                    name: "Value".to_string(),
                    value: Some("0".to_string()),
                    doc: None,
                }],
                is_const: false,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        module.add_export(Export {
            name: "MyType".to_string(),
            original_name: None,
            signature: Signature::TypeAlias(TypeAliasSig {
                name: "MyType".to_string(),
                type_parameters: vec![],
                aliased_type: TypeInfo::primitive("string"),
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        module.add_export(Export {
            name: "MyNamespace".to_string(),
            original_name: None,
            signature: Signature::Namespace(NamespaceSig {
                name: "MyNamespace".to_string(),
                exports: vec![],
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });

        module.add_export(Export {
            name: "default".to_string(),
            original_name: Some("DefaultExport".to_string()),
            signature: Signature::Variable(VariableSig {
                name: "DefaultExport".to_string(),
                type_info: TypeInfo::any(),
                is_const: true,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: true,
        });

        // Add re-exports
        module.add_re_export(ReExport {
            source: "./utils".to_string(),
            names: vec![
                ReExportName {
                    name: "util1".to_string(),
                    alias: None,
                },
                ReExportName {
                    name: "util2".to_string(),
                    alias: Some("renamedUtil".to_string()),
                },
            ],
            is_wildcard: false,
        });

        module.add_re_export(ReExport {
            source: "./helpers".to_string(),
            names: vec![],
            is_wildcard: true,
        });

        interface.add_module(module);

        let encoded = encode(&interface).expect("should encode complete module");
        let decoded: PublicInterface = decode(&encoded).expect("should decode complete module");
        assert_eq!(interface, decoded);
        assert_eq!(decoded.modules[0].exports.len(), 8);
        assert_eq!(decoded.modules[0].re_exports.len(), 2);
    }
}
