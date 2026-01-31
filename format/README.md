# interlint-format

A language-agnostic binary format for describing code signatures and exports.

## Overview

`interlint-format` provides data structures for representing the public interface of a package or module, including functions, classes, interfaces, enums, type aliases, and variables. It uses a compact binary format with MessagePack serialization for efficient storage and transmission.

## Features

- **Language-agnostic**: Works with JavaScript, TypeScript, Rust, Python, Go, and any other language
- **Package-manager agnostic**: Not tied to npm, cargo, pip, or any specific ecosystem
- **Binary format**: Compact MessagePack serialization with integrity verification
- **Type-safe**: Strong typing for all signature components
- **Complete type system**: Supports primitives, generics, unions, intersections, arrays, maps, and more

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
interlint-format = "0.1.0"
```

## Binary Format

The crate uses a custom binary format with the following structure:

### ILMJ Header Structure

```text
+--------+--------+--------+--------+--------+--------+--------+--...--+
| MAGIC (4 bytes) | VER(2) | CRC32 (4 bytes) |   MessagePack Data     |
+--------+--------+--------+--------+--------+--------+--------+--...--+
| 'I' 'L' 'M' 'J' | u16 LE |    u32 LE       |   Variable length      |
+--------+--------+--------+--------+--------+--------+--------+--...--+
```

| Field | Size | Description |
|-------|------|-------------|
| Magic | 4 bytes | `ILMJ` (InterLint MessagePack JSON) - identifies the format |
| Version | 2 bytes | Format version (little-endian u16), currently `1` |
| CRC32 | 4 bytes | Checksum of the MessagePack payload (little-endian u32) |
| Payload | Variable | MessagePack-encoded data |

**Header size**: 10 bytes total

### Integrity Verification

The CRC32 checksum ensures data integrity:
- Computed over the MessagePack payload only (not the header)
- Uses the crc32fast algorithm
- Verified automatically during decoding

## Public API

### Core Functions

```rust
use interlint_format::binary;

// Encode any serializable value to binary format
let bytes: Vec<u8> = binary::encode(&value)?;

// Decode binary data back to a typed value
let value: MyType = binary::decode(&bytes)?;

// Quick validation without full deserialization
let valid: bool = binary::is_valid_format(&bytes);
```

### API Reference

| Function | Description |
|----------|-------------|
| `encode<T: Serialize>(value: &T) -> Result<Vec<u8>>` | Serialize to binary format with header and CRC32 |
| `decode<T: DeserializeOwned>(data: &[u8]) -> Result<T>` | Deserialize from binary format with validation |
| `is_valid_format(data: &[u8]) -> bool` | Check if data has valid header, version, and checksum |
| `get_format_info(data: &[u8]) -> Option<FormatInfo>` | Extract format metadata without deserialization |

### Error Types

```rust
use interlint_format::BinaryError;

match binary::decode::<PublicInterface>(&data) {
    Ok(interface) => { /* use interface */ }
    Err(BinaryError::DataTooShort(len)) => { /* data smaller than 10 bytes */ }
    Err(BinaryError::InvalidMagic(magic)) => { /* not ILMJ format */ }
    Err(BinaryError::UnsupportedVersion(v)) => { /* unknown version */ }
    Err(BinaryError::ChecksumMismatch { expected, actual }) => { /* corrupted data */ }
    Err(BinaryError::Serialize(e)) => { /* MessagePack encoding error */ }
    Err(BinaryError::Deserialize(e)) => { /* MessagePack decoding error */ }
}
```

### Format Inspection

```rust
use interlint_format::binary::{get_format_info, FormatInfo};

if let Some(info) = get_format_info(&data) {
    println!("Version: {}", info.version);
    println!("Checksum: {:#010x}", info.checksum);
    println!("Payload size: {} bytes", info.payload_size);
    println!("Total size: {} bytes", info.total_size);
}
```

### Constants

```rust
use interlint_format::{MAGIC, FORMAT_VERSION};

assert_eq!(MAGIC, b"ILMJ");
assert_eq!(FORMAT_VERSION, 1);
```

## Data Structures

### Core Types

| Type | Description |
|------|-------------|
| `PublicInterface` | The complete public interface of a package |
| `Metadata` | Package metadata (name, version, description, license, language) |
| `Module` | A single source file with its exports |
| `Export` | An exported item from a module |
| `ReExport` | A re-export from another module |
| `Entrypoint` | A package entrypoint (main, exports field, etc.) |
| `Asset` | Non-code files included in distribution |

### Signature Types

| Type | Description |
|------|-------------|
| `Signature` | Enum representing any exportable code construct |
| `FunctionSig` | Function or method signature |
| `ClassSig` | Class or struct signature |
| `InterfaceSig` | Interface, trait, or protocol signature |
| `VariableSig` | Variable or constant signature |
| `EnumSig` | Enumeration signature |
| `TypeAliasSig` | Type alias signature |
| `NamespaceSig` | Nested namespace signature |

### Type System

| Type | Description |
|------|-------------|
| `TypeInfo` | Complete type information |
| `TypeKind` | The kind of type (Primitive, Array, Object, Function, Union, etc.) |
| `TypeParameter` | Generic type parameter with optional constraint and default |
| `Parameter` | Function parameter with type, optional flag, and default value |
| `Property` | Class/interface property with visibility and modifiers |
| `Visibility` | Access level (Public, Protected, Private, Internal) |

### TypeKind Variants

The `TypeKind` enum supports 17 type categories:

- `Primitive` - Basic types (string, number, boolean)
- `Array` - Array/list types
- `Tuple` - Tuple types
- `Object` - Object/struct/record types
- `Function` - Callable types
- `Union` - Union types (A | B)
- `Intersection` - Intersection types (A & B)
- `Generic` - Parameterized types
- `Reference` - Type references by name
- `Literal` - Literal/constant types
- `Optional` - Optional/nullable types
- `Map` - Dictionary/map types
- `Set` - Set types
- `Async` - Promise/future types
- `Void` - Void/unit types
- `Never` - Never/bottom types
- `Any` / `Unknown` - Dynamic types

## Usage Examples

### Encoding and Decoding a Public Interface

```rust
use interlint_format::{
    PublicInterface, Metadata, Module, Export, Entrypoint,
    Signature, FunctionSig, TypeInfo, Visibility, binary,
};

// Create package metadata
let metadata = Metadata {
    name: "my-package".to_string(),
    version: "1.0.0".to_string(),
    description: Some("A sample package".to_string()),
    repository: None,
    license: Some("MIT".to_string()),
    language: Some("typescript".to_string()),
};

// Create a public interface
let mut interface = PublicInterface::new(metadata);

// Add an entrypoint
interface.add_entrypoint(Entrypoint {
    name: ".".to_string(),
    module_path: "src/index.ts".to_string(),
    conditions: vec!["import".to_string(), "types".to_string()],
});

// Create a module with exports
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

// Encode to binary format
let bytes = binary::encode(&interface).expect("encoding failed");

// Validate format
assert!(binary::is_valid_format(&bytes));

// Decode back
let decoded: PublicInterface = binary::decode(&bytes).expect("decoding failed");
assert_eq!(decoded.metadata.name, "my-package");
```

### Adding Function Exports

```rust
use interlint_format::{
    Module, Export, Signature, FunctionSig, Parameter, TypeInfo, Visibility,
};

let mut module = Module::new("src/index.ts");

// Add a function export
module.add_export(Export {
    name: "greet".to_string(),
    original_name: None,
    signature: Signature::Function(FunctionSig {
        name: "greet".to_string(),
        type_parameters: vec![],
        parameters: vec![
            Parameter {
                name: "name".to_string(),
                type_info: TypeInfo::primitive("string"),
                optional: false,
                rest: false,
                default_value: None,
            },
        ],
        return_type: TypeInfo::primitive("string"),
        is_async: false,
        is_generator: false,
        visibility: Visibility::Public,
        doc: Some("Returns a greeting message".to_string()),
    }),
    is_default: false,
});
```

### Adding Class Exports

```rust
use interlint_format::{
    Export, Signature, ClassSig, FunctionSig, Property, TypeInfo, Visibility,
};

let class_export = Export {
    name: "UserService".to_string(),
    original_name: None,
    signature: Signature::Class(ClassSig {
        name: "UserService".to_string(),
        type_parameters: vec![],
        extends: None,
        implements: vec![TypeInfo::reference("IUserService")],
        properties: vec![
            Property {
                name: "users".to_string(),
                type_info: TypeInfo::array(TypeInfo::reference("User")),
                optional: false,
                readonly: true,
                is_static: false,
                visibility: Visibility::Private,
                doc: None,
            },
        ],
        methods: vec![
            FunctionSig {
                name: "getUser".to_string(),
                type_parameters: vec![],
                parameters: vec![],
                return_type: TypeInfo::reference("User"),
                is_async: true,
                is_generator: false,
                visibility: Visibility::Public,
                doc: Some("Fetches a user by ID".to_string()),
            },
        ],
        constructor: None,
        is_abstract: false,
        visibility: Visibility::Public,
        doc: Some("Service for managing users".to_string()),
    }),
    is_default: false,
};
```

### Adding Interface Exports

```rust
use interlint_format::{
    Export, Signature, InterfaceSig, Property, TypeInfo, Visibility,
};

let interface_export = Export {
    name: "Config".to_string(),
    original_name: None,
    signature: Signature::Interface(InterfaceSig {
        name: "Config".to_string(),
        type_parameters: vec![],
        extends: vec![],
        properties: vec![
            Property {
                name: "apiUrl".to_string(),
                type_info: TypeInfo::primitive("string"),
                optional: false,
                readonly: true,
                is_static: false,
                visibility: Visibility::Public,
                doc: None,
            },
            Property {
                name: "timeout".to_string(),
                type_info: TypeInfo::primitive("number"),
                optional: true,
                readonly: false,
                is_static: false,
                visibility: Visibility::Public,
                doc: None,
            },
        ],
        methods: vec![],
        index_signature: None,
        visibility: Visibility::Public,
        doc: Some("Application configuration".to_string()),
    }),
    is_default: false,
};
```

### Working with Re-exports

```rust
use interlint_format::{Module, ReExport, ReExportName};

let mut module = Module::new("src/index.ts");

// Wildcard re-export: export * from './utils'
module.add_re_export(ReExport {
    source: "./utils".to_string(),
    names: vec![],
    is_wildcard: true,
});

// Named re-export with alias: export { foo as bar } from './helpers'
module.add_re_export(ReExport {
    source: "./helpers".to_string(),
    names: vec![
        ReExportName {
            name: "foo".to_string(),
            alias: Some("bar".to_string()),
        },
    ],
    is_wildcard: false,
});
```

### Type Helpers

```rust
use interlint_format::TypeInfo;

// Primitive types
let string_type = TypeInfo::primitive("string");
let number_type = TypeInfo::primitive("number");

// Reference types (named types)
let user_type = TypeInfo::reference("User");

// Array types
let string_array = TypeInfo::array(TypeInfo::primitive("string"));

// Void and any types
let void_type = TypeInfo::void();
let any_type = TypeInfo::any();
```

### Querying the Interface

```rust
use interlint_format::PublicInterface;

fn analyze_interface(interface: &PublicInterface) {
    // Get total export count
    println!("Total exports: {}", interface.export_count());

    // Find a specific module
    if let Some(module) = interface.find_module("src/index.ts") {
        println!("Module has {} exports", module.exports.len());

        // List export names
        for name in module.export_names() {
            println!("  - {}", name);
        }
    }

    // Iterate all exports across all modules
    for (module, export) in interface.all_exports() {
        println!("{}: {} ({})",
            module.path,
            export.name,
            export.signature.name()
        );
    }
}
```

## API Reference

### PublicInterface Methods

| Method | Description |
|--------|-------------|
| `new(metadata)` | Create a new interface with metadata |
| `add_entrypoint(entrypoint)` | Add a package entrypoint |
| `add_module(module)` | Add a module to the interface |
| `add_asset(asset)` | Add a non-code asset |
| `find_module(path)` | Find a module by path |
| `find_module_mut(path)` | Find a module by path (mutable) |
| `all_exports()` | Iterate all exports across modules |
| `export_count()` | Get total number of exports |

### Module Methods

| Method | Description |
|--------|-------------|
| `new(path)` | Create a new module with the given path |
| `add_export(export)` | Add an export to the module |
| `add_re_export(re_export)` | Add a re-export to the module |
| `export_names()` | Iterate over export names |

### Signature Methods

| Method | Description |
|--------|-------------|
| `name()` | Get the name of the signature |
| `visibility()` | Get the visibility level |
| `doc()` | Get the documentation comment (if any) |

### TypeInfo Constructors

| Method | Description |
|--------|-------------|
| `primitive(repr)` | Create a primitive type |
| `reference(name)` | Create a type reference |
| `array(element)` | Create an array type |
| `void()` | Create a void type |
| `any()` | Create an any type |

## License

MIT
