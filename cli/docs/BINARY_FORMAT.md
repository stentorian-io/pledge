# Breakpoint CLI - Binary Format Documentation

Technical documentation on the binary format used for API communication between the Breakpoint CLI and Interlint API.

---

## Overview

The Breakpoint CLI uses the **Interlint binary format** for all communication with the API server. This format provides:

- **Efficiency:** 30-50% smaller than JSON
- **Performance:** Faster serialization/deserialization
- **Type safety:** Better handling of complex types

---

## Wire Format

### Message Structure

All messages use the following structure:

```
[8-byte header] [MessagePack data] [4-byte checksum]
Total: header + data + checksum
```

### Header

The 8-byte header contains:

```
Bytes 0-3: Magic number (0x494C4D4A - "ILMJ")
Bytes 4-7: Payload length (big-endian u32)
```

Example header for 1024-byte payload:
```
494C4D4A 00000400
```

### MessagePack Data

The payload is serialized using **MessagePack** format:
- Compact binary format for structured data
- Supports strings, numbers, arrays, maps, binaries
- Full support for nested structures

### Checksum

CRC32 checksum of the entire message (header + data):

```
CRC32(header || payload)
```

Used for integrity verification on receipt.

---

## File Format (.ilmj)

The Interlint binary format can be saved to files with the `.ilmj` extension:

```
public_interface.ilmj     # Binary format file
```

These files contain the complete `PublicInterface` structure serialized in the Interlint binary format.

---

## Content Types

When communicating with the API:

| Endpoint | Request Content-Type | Response Content-Type |
|----------|-------------------|----------------------|
| `GET /health` | N/A | `application/json` |
| `POST /v1/compare` | `application/octet-stream` | `application/octet-stream` |

### Accepted Content-Types

For `/v1/compare` requests:
- `application/octet-stream` (preferred)
- `application/msgpack` (alternative)

**Any other Content-Type (including `application/json`) will be rejected with `415 Unsupported Media Type`.**

---

## Request/Response Format

### Compare Request

**Content-Type:** `application/octet-stream`

MessagePack structure:

```
{
  "previous": PublicInterface,
  "current": PublicInterface,
  "min_severity": "major",           // Optional
  "include_additions": false         // Optional
}
```

### Compare Response

**Content-Type:** `application/octet-stream`

MessagePack structure:

```
{
  "changes": [
    {
      "change_type": "export_removed",
      "severity": "critical",
      "path": "index.ts#myFunc",
      "name": "myFunc",
      "message": "Export 'myFunc' was removed",
      "previous": "function",
      "suggestion": "Update consumers or provide migration path"
    }
  ],
  "summary": {
    "total_breaking_changes": 1,
    "critical_count": 1,
    "major_count": 0,
    "minor_count": 0,
    "warning_count": 0,
    "has_breaking_changes": true,
    "suggested_bump": "major",
    "previous_export_count": 10,
    "current_export_count": 9
  },
  "previous_version": "1.0.0",
  "current_version": "2.0.0",
  "package_name": "my-package"
}
```

---

## PublicInterface Structure

The core data structure exchanged between CLI and API:

```
PublicInterface {
  metadata: PackageConfig {
    name: string,
    version: string,
    description?: string
  },
  modules: Module[],
  entrypoints: Entrypoint[],
  assets: Asset[]?
}
```

### Module

```
Module {
  path: string,           // e.g., "src/index.ts"
  exports: Export[]
}
```

### Export

```
Export {
  name: string,           // e.g., "myFunction"
  kind: ExportKind,       // Function, Class, Interface, etc.
  signature: Signature,   // Type information
  location: Location?     // Optional source location
}
```

### Export Kinds

- `Function`
- `Class`
- `Interface`
- `TypeAlias`
- `Enum`
- `Variable`
- `Namespace`
- `Default`

### Signature Types

- `FunctionSig` - Function signature
- `ClassSig` - Class structure
- `InterfaceSig` - Interface structure
- `EnumSig` - Enum members and values
- `TypeAliasSig` - Aliased type
- `VariableSig` - Variable type

---

## Serialization Details

### Type Representation

All types are represented as language-agnostic structures:

```
TypeInfo {
  kind: "primitive" | "object" | "array" | "union" | "generic" | "ref",
  name?: string,
  properties?: Map<string, TypeInfo>,
  items?: TypeInfo,
  types?: TypeInfo[],
  parameters?: TypeInfo[],
  constraints?: TypeInfo[],
  ref?: string  // For type references
}
```

### Parameter Information

```
Parameter {
  name: string,
  type: TypeInfo,
  optional: bool,
  variadic: bool,
  default?: string
}
```

### Function Signature

```
FunctionSig {
  parameters: Parameter[],
  return_type: TypeInfo,
  type_parameters: TypeParameter[],
  is_async: bool,
  is_generator: bool
}
```

---

## MessagePack Format Details

### Why MessagePack?

1. **Compact encoding:** Each type uses minimal bytes
   - Strings: 1-4 bytes overhead
   - Numbers: 1-9 bytes depending on value
   - Arrays: 1-5 bytes overhead

2. **Fast parsing:** Single-pass decoding
   - No need for escape sequence parsing
   - Direct type detection from first byte

3. **Type safety:** All types explicitly encoded
   - Numbers preserve precision
   - Binary data separate from strings
   - No ambiguity in deserialization

### MessagePack Type Mapping

| JSON | MessagePack | Bytes |
|------|------------|-------|
| null | nil | 1 |
| true/false | bool | 1 |
| 0-127 | positive fixint | 1 |
| 128-65535 | uint 16 | 3 |
| -32 to -1 | negative fixint | 1 |
| "hello" (5 chars) | fixstr | 6 |
| [1,2,3] | fixarray | 4 |
| {a:1} | fixmap | 4 |

---

## Error Format

### Binary Error Response

When an error occurs, the response is still in binary format:

```
{
  "error": "Error message",
  "code": "ERROR_CODE"
}
```

### Common Error Codes

| Code | Meaning |
|------|---------|
| `INVALID_CONTENT_TYPE` | Wrong Content-Type header |
| `INVALID_BINARY_FORMAT` | Malformed binary data |
| `PACKAGE_NAME_MISMATCH` | Previous and current package names don't match |
| `PARSE_ERROR` | Failed to parse binary data |
| `INTERNAL_ERROR` | Server error during analysis |

---

## Client Implementation

### Rust Example

Using the `interlint-format` crate:

```rust
use interlint_format::{binary, PublicInterface, CompareRequest};
use reqwest::Client;

async fn send_compare_request(
    previous: PublicInterface,
    current: PublicInterface,
) -> Result<CompareResponse> {
    let request = CompareRequest {
        previous,
        current,
        min_severity: None,
        include_additions: false,
    };

    // Serialize to binary
    let body = binary::serialize(&request)?;

    // Send request
    let client = Client::new();
    let response = client
        .post("http://localhost:3000/v1/compare")
        .header("Content-Type", "application/octet-stream")
        .body(body)
        .send()
        .await?;

    // Deserialize response
    let bytes = response.bytes().await?;
    let result = binary::deserialize(&bytes)?;
    Ok(result)
}
```

### JavaScript/Node.js Example

Using the `msgpack5` library:

```javascript
const msgpack = require('msgpack5')();
const fetch = require('node-fetch');

async function sendCompareRequest(previous, current) {
  const request = {
    previous,
    current,
    min_severity: null,
    include_additions: false
  };

  // Serialize to binary
  const body = msgpack.encode(request);

  // Send request
  const response = await fetch('http://localhost:3000/v1/compare', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/octet-stream'
    },
    body
  });

  // Deserialize response
  const buffer = await response.arrayBuffer();
  const result = msgpack.decode(Buffer.from(buffer));
  return result;
}
```

### Python Example

Using the `msgpack` library:

```python
import msgpack
import requests

def send_compare_request(previous, current):
    request = {
        'previous': previous,
        'current': current,
        'min_severity': None,
        'include_additions': False
    }

    # Serialize to binary
    body = msgpack.packb(request, use_bin_type=True)

    # Send request
    response = requests.post(
        'http://localhost:3000/v1/compare',
        headers={'Content-Type': 'application/octet-stream'},
        data=body
    )

    # Deserialize response
    result = msgpack.unpackb(response.content, raw=False)
    return result
```

---

## Performance Characteristics

### Serialization Overhead

Typical package comparison:

```
Operation                Time        Notes
Serialize previous      2-5ms       10-100 exports
Serialize current       2-5ms       10-100 exports
Send over network      10-50ms      Depends on connection
API processing        50-200ms      Analysis time
Serialize response      2-5ms       Results
Deserialize response    2-5ms       Results
```

**Total:** 70-270ms for typical packages

### Payload Sizes

Typical comparison:

```
Small package (10 exports)
- Previous: 2-5 KB
- Current:  2-5 KB
- Response: 5-10 KB

Medium package (100 exports)
- Previous: 20-50 KB
- Current:  20-50 KB
- Response: 10-50 KB

Large package (1000+ exports)
- Previous: 200-500 KB
- Current:  200-500 KB
- Response: 50-200 KB
```

---

## Debugging Binary Format

### Dump Binary Data

To inspect binary format (useful for debugging):

```bash
# Rust - using msgpack-cli
echo "Binary file content" | msgpack-cli dump file.ilmj

# Python
python3 << 'EOF'
import msgpack
with open('file.ilmj', 'rb') as f:
  data = msgpack.unpack(f)
  print(data)
EOF
```

### Verify Message Integrity

```bash
# Check CRC32 checksum
python3 << 'EOF'
import struct
import binascii

with open('file.ilmj', 'rb') as f:
  data = f.read()
  # Last 4 bytes are checksum
  stored_crc = struct.unpack('>I', data[-4:])[0]
  # Calculate CRC of data (excluding stored checksum)
  calculated_crc = binascii.crc32(data[:-4]) & 0xffffffff
  print(f"Stored:     {stored_crc:08x}")
  print(f"Calculated: {calculated_crc:08x}")
  print(f"Valid: {stored_crc == calculated_crc}")
EOF
```

---

## Compatibility

### Versioning

The binary format includes no explicit version field. Compatibility is ensured by:

1. **Stable MessagePack format** - Widely standardized
2. **Field matching** - Only required fields are encoded
3. **API version** - Use consistent versions of API and CLI

### Forward/Backward Compatibility

- **Adding optional fields:** Backward compatible
- **Removing fields:** Only if optional
- **Changing field types:** Breaking change (requires version bump)
- **Reordering fields:** MessagePack-safe (fields are keyed)

---

## References

- [MessagePack Specification](https://github.com/msgpack/msgpack/blob/master/spec.md)
- [interlint-format Crate Documentation](../../../format/README.md)
- [API Server Documentation](../../../api/README.md)

---

## Summary

The Interlint binary format provides:

1. **Efficiency:** 30-50% smaller than JSON
2. **Type safety:** Explicit type encoding
3. **Fast parsing:** Single-pass deserialization
4. **Integrity:** CRC32 checksums on all messages
5. **Portability:** Well-standardized MessagePack format

For CLI/API communication, always use binary format with appropriate Content-Type headers.
