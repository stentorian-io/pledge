# Breakpoint

A fast, intelligent CLI tool for detecting breaking changes in JavaScript and TypeScript packages. Breakpoint analyzes your package's public API surface and compares it against previous versions to identify potential breaking changes before they reach production.

## Overview

Breakpoint is part of the Interlint ecosystem and serves as the command-line interface for breaking change detection. It parses TypeScript/JavaScript source files, extracts the public API (exports, types, interfaces, classes, functions), and sends them to the Interlint API for semantic comparison.

### Key Use Cases

- **Pre-publish validation**: Catch breaking changes before publishing to npm
- **CI/CD integration**: Automatically flag breaking changes in pull requests
- **Version bump guidance**: Get recommendations for semantic versioning based on changes
- **API documentation**: Understand what's exported from your package

## Features

- **Native pack integration**: Uses your package manager's `pack` command (npm, pnpm, yarn, bun) for accurate file inclusion
- **Package manager detection**: Automatically detects your package manager from lockfiles
- **First-publish detection**: Gracefully handles first-time publishes with no previous version to compare
- **Automatic registry comparison**: Fetches the latest published version from npm for comparison
- **Local comparison mode**: Compare two local package directories directly
- **TypeScript-first**: Full support for TypeScript syntax including generics, interfaces, and type aliases
- **Comprehensive detection**: Identifies 30+ types of breaking changes
- **Severity classification**: Changes are categorized as Critical, Major, Minor, or Warning
- **Smart exit codes**: Returns non-zero exit code when breaking changes are detected (CI-friendly)

### Supported Breaking Change Types

| Category | Examples |
|----------|----------|
| **Exports** | Export removed, module removed, entrypoint removed |
| **Functions** | Required parameter added, parameter type changed, return type changed |
| **Interfaces/Types** | Property removed, property type changed, optional became required |
| **Classes** | Method removed, constructor signature changed, visibility changed |
| **Enums** | Member removed, member value changed |
| **Generics** | Type parameter added/removed, constraint changed |

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/interlint/interlint.git
cd interlint/cli

# Build in release mode
cargo build --release

# The binary will be at ./target/release/breakpoint
```

### Prerequisites

- Rust 1.70+ (for building from source)
- Interlint API server running (default: `http://localhost:3000`)
- One of the following package managers installed:
  - npm (included with Node.js)
  - pnpm
  - yarn
  - bun

## Usage

### Basic Usage

```bash
# Compare local package against its latest published version on npm
breakpoint <package-path>

# Compare two local package directories
breakpoint --previous <previous-path> <current-path>
```

### Examples

#### Detect Breaking Changes Against npm

```bash
# Navigate to your package and run breakpoint
cd my-npm-package
breakpoint .

# Or specify the path directly
breakpoint /path/to/my-npm-package
```

This mode:
1. Detects your package manager (npm, pnpm, yarn, bun) from lockfiles
2. Packs your package using the native `pack` command
3. Checks the npm registry for a previous version
4. If found, downloads and extracts the previous version tarball
5. If not found (first publish), shows a summary and skips comparison
6. Parses both versions and extracts public APIs
7. Sends to Interlint API for comparison
8. Reports any breaking changes found

#### Compare Two Local Versions

```bash
# Compare v1 against v2
breakpoint --previous ./v1 ./v2

# Useful for testing changes before committing
breakpoint --previous ./main-branch ./feature-branch
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `INTERLINT_API_URL` | URL of the Interlint API server | `http://localhost:3000` |

**Setting Environment Variables:**

```bash
# Bash/Zsh
export INTERLINT_API_URL=http://custom-api.example.com:3000
breakpoint /path/to/package

# PowerShell (Windows)
$env:INTERLINT_API_URL = "http://custom-api.example.com:3000"
.\target\release\breakpoint.exe C:\path\to\package

# Inline (one-time use)
INTERLINT_API_URL=http://localhost:8080 breakpoint /path/to/package
```

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | No breaking changes detected |
| `1` | Breaking changes detected |
| `1` | Error occurred (see stderr for details) |

This makes it easy to integrate with CI/CD pipelines:

```bash
# In your CI script
breakpoint .
if [ $? -eq 0 ]; then
  echo "✅ No breaking changes"
else
  echo "❌ Breaking changes detected"
  exit 1
fi

# Or short form
breakpoint . || { echo "Breaking changes found"; exit 1; }
```

### Common Errors

If breakpoint fails, check the error message:

```
Error: Failed to connect to http://localhost:3000
  → Start the API server: cd api && cargo run

Error: Failed to load and pack current package
  → Check package.json validity: npm validate

Error: Failed to parse file
  → Check for TypeScript/JavaScript syntax errors: npx tsc --noEmit

Error: No previous version found
  → This is normal for first-time publishes
```

For detailed troubleshooting, see [TROUBLESHOOTING.md](./docs/TROUBLESHOOTING.md).

## Supported Languages and File Types

### JavaScript/TypeScript Extensions

| Extension | Description |
|-----------|-------------|
| `.js` | JavaScript |
| `.jsx` | JavaScript with JSX |
| `.cjs` | CommonJS module |
| `.mjs` | ES module |
| `.ts` | TypeScript |
| `.tsx` | TypeScript with JSX |
| `.cts` | CommonJS TypeScript |
| `.mts` | ES module TypeScript |

### Package.json Fields

Breakpoint analyzes exports from these `package.json` fields:

- `main` - Main entry point
- `exports` - Conditional exports (Node.js 12+)
- `browser` - Browser-specific entry points
- `bin` - CLI entry points

### Supported Export Types

| Export Type | Example |
|-------------|---------|
| Functions | `export function createUser(name: string): User` |
| Classes | `export class UserService { ... }` |
| Interfaces | `export interface Config { ... }` |
| Type Aliases | `export type Status = 'active' \| 'inactive'` |
| Enums | `export enum HttpStatus { OK = 200 }` |
| Constants | `export const VERSION = '1.0.0'` |
| Default exports | `export default function() { ... }` |
| Re-exports | `export { foo } from './module'` |
| Namespace exports | `export * as utils from './utils'` |

## Architecture

```
breakpoint/
├── src/
│   ├── main.rs              # CLI entry point and argument parsing
│   ├── lib.rs               # Library exports
│   ├── api.rs               # Interlint API client
│   ├── converter.rs         # AST to PublicInterface conversion
│   ├── ecma/                # ECMAScript/TypeScript parsing
│   │   ├── parser.rs        # Tree-sitter based parser
│   │   ├── walker.rs        # AST traversal and export extraction
│   │   └── entity.rs        # Declaration types (Func, Class, etc.)
│   ├── fs/                  # File system utilities
│   │   ├── file.rs          # File extension handling
│   │   └── path.rs          # Path matching and glob patterns
│   └── pkg/                 # Package handling
│       ├── mod.rs           # Package struct and config parsing
│       ├── pack.rs          # Package manager detection and native packing
│       ├── registry.rs      # npm registry integration
│       ├── registry_client.rs # HTTP client for registry operations
│       ├── entries.rs       # Package entry point resolution
│       ├── contents.rs      # File inclusion/exclusion (files, .npmignore)
│       └── tarball.rs       # Tarball download and extraction
```

### How It Works

1. **Package Manager Detection**: Detects npm/pnpm/yarn/bun from lockfiles or `packageManager` field in package.json

2. **Native Packing**: Executes the package manager's `pack` command to create a tarball, ensuring accurate file inclusion

3. **Registry Check**: Makes an HTTP request to check if the package exists in the npm registry

4. **Previous Version**: If the package exists, downloads the latest published tarball with SHA-512 integrity verification. If not (first publish), displays a summary and skips comparison

5. **Package Loading**: Reads `package.json` and resolves entry points based on `main`, `exports`, `browser`, and `bin` fields

6. **Parsing**: Uses [tree-sitter](https://tree-sitter.github.io/) to parse TypeScript/JavaScript into an AST

7. **Export Extraction**: Walks the AST to find all exported declarations, following re-exports and namespace exports

8. **Conversion**: Transforms the AST types into the language-agnostic `interlint-format` types

9. **API Communication**: Serializes interfaces using the interlint binary format (MessagePack with header and checksum) and sends to the Interlint API

10. **Result Display**: Prints breaking changes grouped by severity with suggestions for fixes

### Dependencies

- **swc_ecma_parser**: Fast TypeScript/JavaScript parser
- **reqwest**: HTTP client for API communication and npm registry
- **interlint-format**: Shared format types and binary serialization (MessagePack)
- **tar/flate2**: Tarball handling for npm package downloads
- **glob**: Pattern matching for file inclusion/exclusion

### Binary Format

The CLI uses the interlint binary format for all API communication:

- **Wire format**: MessagePack with 8-byte header and CRC32 checksum
- **Content-Type**: `application/x-interlint`
- **File extension**: `.ilmj` (for saved interface files)

This binary format is more compact and efficient than JSON, reducing bandwidth and improving parsing performance. There is no JSON output option for interface data.

## Example Output

### Breaking Changes Detected

```
Will use https://registry.npmjs.org/ as registry.
Loading previous version from: ./examples/v1
Converting packages to format...
Previous: 15 exports across 1 modules
Current: 14 exports across 1 modules

Sending to Interlint API for analysis...

  Breaking Changes Detected:
  ===========================

  CRITICAL (3):
    - Export 'VERSION' was removed at 'src/index.ts#VERSION'
    - Required parameter 'role' was added to function at 'src/index.ts#createUser'
      Previous: createUser(name: string, email: string): User
      Current: createUser(name: string, email: string, role: Role): User
    - Property 'timeout' changed from optional to required at 'src/index.ts#Config'

  MAJOR (2):
    - Parameter type changed for 'id' in method 'getUser' at 'src/index.ts#UserService'
      Previous: number
      Current: string
    - Type union expanded at 'src/index.ts#Status'
      Previous: 'active' | 'inactive'
      Current: 'active' | 'inactive' | 'pending'

  Summary:
  ---------
  Total breaking changes: 5
  Previous exports: 15 -> Current exports: 14

Analysis completed in 1.23s
```

### No Breaking Changes

```
Loading current package...
Detected npm from lockfile: package-lock.json
Using npm 10.8.2 for packing
Packing with npm 10.8.2
Checking registry for previous version of 'my-package'...
Found package in registry, fetching latest version...
Downloaded my-package v1.0.0
Converting packages to format...
Previous: 15 exports across 1 modules
Current: 17 exports across 1 modules

Sending to Interlint API for analysis...

  No breaking changes detected.

Analysis completed in 0.89s
```

### First Publish (No Previous Version)

```
Loading current package...
Detected pnpm from lockfile: pnpm-lock.yaml
Using pnpm 8.15.0 for packing
Packing with pnpm 8.15.0
Checking registry for previous version of 'my-new-package'...

⚠️  No previous version found for 'my-new-package'.
   This appears to be the first publish of this package.
   Skipping breaking change comparison.

Analyzing package: my-new-package v1.0.0
Package has 8 exports across 2 modules

Analysis completed in 0.45s

✅ Ready to publish my-new-package v1.0.0
```

## Testing

The `examples/` directory contains test fixtures for verifying breaking change detection:

```bash
# Build the CLI
cargo build --release

# Test breaking changes detection (should find breaking changes)
./target/release/breakpoint --previous ./examples/v1 ./examples/v2-breaking

# Test compatible changes (should find no breaking changes)
./target/release/breakpoint --previous ./examples/v1 ./examples/v2-compatible
```

### Running Unit Tests

```bash
cargo test
```

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Breaking Change Check

on:
  pull_request:
    branches: [main]

jobs:
  check-breaking-changes:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Checkout main branch
        run: git worktree add ../main main

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Build breakpoint
        run: cargo build --release
        working-directory: ./interlint/cli

      - name: Check for breaking changes
        run: |
          ./interlint/cli/target/release/breakpoint \
            --previous ../main/my-package \
            ./my-package
```

## Documentation

Additional documentation is available in the `docs/` directory:

### Getting Started & Troubleshooting
- [Troubleshooting Guide](./docs/TROUBLESHOOTING.md) - Common issues and solutions
- [Server Requirements](./docs/SERVER_REQUIREMENTS.md) - API server setup and configuration

### Technical Documentation
- [Query Patterns Reference](./docs/query-patterns-reference.md) - Tree-sitter query patterns for JS/TS
- [Parser Interface Design](./docs/parser-interface-design.md) - Language-agnostic parser architecture
- [Package Manager Research](./docs/package-manager-research.md) - npm/pnpm/yarn/bun pack commands and registry API

## Related Projects

- **interlint-format**: Shared types for representing public interfaces
- **interlint-api**: Server-side breaking change analysis engine

## License

MIT
