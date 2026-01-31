# interlint-api

A high-performance REST API service for detecting breaking changes in public interfaces. Built with Rust and Axum for maximum performance and reliability.

## Overview

`interlint-api` provides HTTP endpoints for comparing two versions of a package's public interface and detecting breaking changes. It works with the language-agnostic `interlint-format` to support multiple programming languages including TypeScript, JavaScript, Rust, Python, and Go.

The API analyzes changes between interface versions and provides:

- **Breaking change detection** with severity classification
- **Semantic versioning recommendations** based on detected changes
- **Detailed change reports** with migration suggestions
- **Support for all export types**: functions, classes, interfaces, enums, type aliases, and variables

## Features

- **High Performance**: Built with Axum and Tokio for async, non-blocking I/O
- **Binary MessagePack Format**: All compare requests use binary MessagePack for efficient serialization
- **Language Agnostic**: Works with any language that can export to `interlint-format`
- **Comprehensive Detection**: Identifies 35+ types of breaking changes
- **Severity Classification**: Critical, Major, Minor, and Warning levels
- **CORS Enabled**: Ready for browser-based integrations
- **Request Tracing**: Built-in HTTP request tracing for observability

## Binary Format

The `/v1/compare` endpoint accepts **only binary MessagePack format**. JSON requests are not supported and will return `415 Unsupported Media Type`.

### Why Binary MessagePack?

- **Compact**: 30-50% smaller than JSON for typical payloads
- **Fast**: Faster serialization/deserialization than text JSON
- **Type-safe**: Better handling of binary data and complex types

### Content-Type Requirements

| Endpoint | Request Format | Response Format |
|----------|----------------|-----------------|
| `GET /health` | N/A | JSON |
| `POST /v1/compare` | Binary MessagePack | Binary MessagePack |

### Accepted Content-Types for `/v1/compare`

- `application/octet-stream` (preferred)
- `application/msgpack`

Any other Content-Type (including `application/json`) will be rejected with `415 Unsupported Media Type`.

## API Endpoints

### Health Check

```
GET /health
```

Returns the service status and version.

**Response:**

```json
{
  "status": "healthy",
  "version": "0.1.0"
}
```

### Compare Interfaces

```
POST /v1/compare
Content-Type: application/octet-stream
```

Compares two public interfaces and returns detected breaking changes.

**Important**: This endpoint accepts **only binary MessagePack format**. JSON requests will be rejected with `415 Unsupported Media Type`.

**Request Body (MessagePack-encoded):**

```
{
  "previous": { /* PublicInterface */ },
  "current": { /* PublicInterface */ },
  "min_severity": "major",       // Optional: filter by minimum severity
  "include_additions": false     // Optional: include non-breaking additions
}
```

**Response (MessagePack-encoded):**

The response is returned with `Content-Type: application/octet-stream`. When decoded, the structure is:

```
{
  "changes": [
    {
      "change_type": "export_removed",
      "severity": "critical",
      "path": "index.ts#myFunction",
      "name": "myFunction",
      "message": "Export 'myFunction' was removed from 'index.ts'",
      "previous": "function",
      "suggestion": "Remove usages of 'myFunction' or provide a migration path"
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

**Error Response (415 Unsupported Media Type):**

Returned when sending JSON or other non-MessagePack content:

```json
{
  "error": "Expected binary MessagePack format (Content-Type: application/octet-stream or application/msgpack), got: application/json",
  "code": "INVALID_CONTENT_TYPE"
}
```

**Error Response (400 Bad Request, MessagePack-encoded):**

```
{
  "error": "Package names do not match: 'package-a' vs 'package-b'",
  "code": "PACKAGE_NAME_MISMATCH"
}
```

## Breaking Change Types

The API detects the following categories of breaking changes:

### Critical (Severity: 4)
- `export_removed` - An export was removed from the public interface
- `module_removed` - A module was removed
- `entrypoint_removed` - An entrypoint was removed
- `parameter_added` - A required parameter was added to a function
- `parameter_became_required` - An optional parameter became required
- `property_became_required` - An optional property became required
- `export_kind_changed` - Export kind changed (e.g., function to class)

### Major (Severity: 3)
- `signature_changed` - Function signature changed
- `parameter_removed` - A parameter was removed
- `parameter_type_changed` - Parameter type changed
- `return_type_changed` - Return type changed
- `type_changed` - Type definition changed
- `property_removed` - Property removed from interface/class
- `property_type_changed` - Property type changed
- `method_removed` - Method removed from class/interface
- `method_signature_changed` - Method signature changed
- `enum_member_removed` - Enum member removed
- `enum_member_value_changed` - Enum member value changed
- `type_parameter_removed` - Type parameter removed
- `type_parameter_constraint_changed` - Type parameter constraint changed
- `visibility_changed` - Visibility changed (e.g., public to private)
- `type_arguments_changed` - Generic type arguments changed
- `default_export_changed` - Default export status changed
- `asset_removed` - Asset file removed from package

### Minor (Severity: 2)
- `async_modifier_changed` - Async modifier changed

### Warning (Severity: 1)
- `export_added` - An export was added (informational)
- `module_added` - A module was added (informational)
- `entrypoint_added` - An entrypoint was added (informational)
- `parameter_became_optional` - A parameter became optional
- `property_became_optional` - A property became optional
- `property_added` - Property added (optional properties)
- `method_added` - Method added to class/interface
- `enum_member_added` - Enum member added
- `type_parameter_added` - Type parameter added
- `asset_added` - Asset file added to package

## Suggested Version Bump

Based on detected changes, the API suggests a semantic version bump:

| Highest Severity | Suggested Bump |
|------------------|----------------|
| Critical or Major | `major` (X.0.0) |
| Minor | `minor` (x.X.0) |
| Warning only | `patch` (x.x.X) |
| No changes | `none` |

## Architecture

```
interlint-api/
├── src/
│   ├── main.rs          # Server entry point and initialization
│   ├── config.rs        # Configuration from environment variables
│   ├── state.rs         # Shared application state
│   ├── routes.rs        # Router setup and middleware configuration
│   ├── response.rs      # Request/response types and breaking change definitions
│   ├── diff.rs          # Core diff detection algorithm
│   ├── binary.rs        # Binary MessagePack extractor and response types
│   └── handlers/
│       ├── mod.rs       # Handler module exports
│       ├── health.rs    # Health check handler (JSON)
│       └── compare.rs   # Compare endpoint handler (binary MessagePack)
└── Cargo.toml
```

### Components

- **Config**: Loads server configuration from environment variables
- **AppState**: Shared state across request handlers (extensible for database connections, caches, etc.)
- **Routes**: Sets up Axum router with CORS and tracing middleware
- **Handlers**: HTTP request handlers for each endpoint
- **Diff Engine**: Recursive comparison of interfaces, modules, exports, and signatures

## Running the Server

### Prerequisites

- Rust 1.70+ (2021 edition)
- Cargo

### Development

```bash
# Run with default settings (port 3000)
cargo run

# Run with custom port
PORT=8080 cargo run

# Run with custom host binding
HOST=127.0.0.1 PORT=8080 cargo run
```

### Production

```bash
# Build release binary
cargo build --release

# Run the binary
PORT=3000 HOST=0.0.0.0 ./target/release/interlint-api
```

## Configuration

Configuration is loaded from environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `PORT` | Server port | `3000` |
| `HOST` | Host address to bind | `0.0.0.0` |

## Example Usage

### Health Check (JSON)

```bash
curl http://localhost:3000/health
```

Response:
```json
{
  "status": "healthy",
  "version": "0.1.0"
}
```

### Compare Interfaces (Binary MessagePack)

Since the compare endpoint requires binary MessagePack format, you cannot use plain curl with JSON. You need to use a client library that supports MessagePack serialization.

#### Using Rust

```rust
use rmp_serde;
use reqwest;

let request = CompareRequest {
    previous: /* PublicInterface */,
    current: /* PublicInterface */,
    min_severity: None,
    include_additions: false,
};

// Serialize to MessagePack
let body = rmp_serde::to_vec_named(&request)?;

// Send request
let response = reqwest::Client::new()
    .post("http://localhost:3000/v1/compare")
    .header("Content-Type", "application/octet-stream")
    .body(body)
    .send()
    .await?;

// Deserialize response from MessagePack
let bytes = response.bytes().await?;
let result: CompareResponse = rmp_serde::from_slice(&bytes)?;
```

#### Using Node.js

```javascript
const msgpack = require('@msgpack/msgpack');

const request = {
  previous: { /* PublicInterface */ },
  current: { /* PublicInterface */ },
  min_severity: null,
  include_additions: false
};

// Serialize to MessagePack
const body = msgpack.encode(request);

// Send request
const response = await fetch('http://localhost:3000/v1/compare', {
  method: 'POST',
  headers: { 'Content-Type': 'application/octet-stream' },
  body: body
});

// Deserialize response from MessagePack
const buffer = await response.arrayBuffer();
const result = msgpack.decode(new Uint8Array(buffer));
```

#### Using Python

```python
import msgpack
import requests

request = {
    "previous": { ... },  # PublicInterface
    "current": { ... },   # PublicInterface
    "min_severity": None,
    "include_additions": False
}

# Serialize to MessagePack
body = msgpack.packb(request)

# Send request
response = requests.post(
    'http://localhost:3000/v1/compare',
    headers={'Content-Type': 'application/octet-stream'},
    data=body
)

# Deserialize response from MessagePack
result = msgpack.unpackb(response.content)
```

### Example Response (decoded from MessagePack)

```json
{
  "changes": [{
    "change_type": "export_removed",
    "severity": "critical",
    "path": "index.ts#greet",
    "name": "greet",
    "message": "Export 'greet' was removed from 'index.ts'",
    "previous": "function",
    "suggestion": "Remove usages of 'greet' or provide a migration path"
  }],
  "summary": {
    "total_breaking_changes": 1,
    "critical_count": 1,
    "major_count": 0,
    "minor_count": 0,
    "warning_count": 0,
    "has_breaking_changes": true,
    "suggested_bump": "major",
    "previous_export_count": 1,
    "current_export_count": 0
  },
  "previous_version": "1.0.0",
  "current_version": "2.0.0",
  "package_name": "my-package"
}
```

## Testing

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_compare_with_breaking_change
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `axum` | Web framework |
| `tokio` | Async runtime |
| `serde` / `serde_json` | Serialization framework |
| `rmp-serde` | MessagePack serialization |
| `tower-http` | HTTP middleware (CORS, tracing) |
| `tracing` / `tracing-subscriber` | Structured logging |
| `interlint-format` | Public interface data structures |

## License

See the repository root for license information.
