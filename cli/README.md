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
1. Reads `package.json` to get the package name and version
2. Fetches the latest published version from the npm registry
3. Parses both versions and extracts public APIs
4. Sends to Interlint API for comparison
5. Reports any breaking changes found

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

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | No breaking changes detected |
| `1` | Breaking changes detected |

This makes it easy to integrate with CI/CD pipelines:

```bash
# In your CI script
breakpoint . || echo "Breaking changes detected!"
```

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
│   ├── main.rs          # CLI entry point and argument parsing
│   ├── lib.rs           # Library exports
│   ├── api.rs           # Interlint API client
│   ├── converter.rs     # AST to PublicInterface conversion
│   ├── ecma/            # ECMAScript/TypeScript parsing
│   │   ├── parser.rs    # SWC-based parser
│   │   ├── walker.rs    # AST traversal and export extraction
│   │   └── entity.rs    # Declaration types (Func, Class, etc.)
│   ├── fs/              # File system utilities
│   │   ├── file.rs      # File extension handling
│   │   └── path.rs      # Path matching and glob patterns
│   └── pkg/             # Package handling
│       ├── mod.rs       # Package struct and config parsing
│       ├── registry.rs  # npm registry integration
│       ├── entries.rs   # Package entry point resolution
│       ├── contents.rs  # File inclusion/exclusion (files, .npmignore)
│       └── tarball.rs   # Tarball download and extraction
```

### How It Works

1. **Package Loading**: Reads `package.json` and resolves entry points based on `main`, `exports`, `browser`, and `bin` fields

2. **File Discovery**: Respects `files` field and `.npmignore` patterns to determine which files are part of the package

3. **Parsing**: Uses [SWC](https://swc.rs/) (Speedy Web Compiler) to parse TypeScript/JavaScript into an AST

4. **Export Extraction**: Walks the AST to find all exported declarations, following re-exports and namespace exports

5. **Conversion**: Transforms SWC's AST types into the language-agnostic `interlint-format` types

6. **API Communication**: Sends the previous and current `PublicInterface` to the Interlint API for semantic comparison

7. **Result Display**: Prints breaking changes grouped by severity with suggestions for fixes

### Dependencies

- **swc_ecma_parser**: Fast TypeScript/JavaScript parser
- **reqwest**: HTTP client for API communication and npm registry
- **interlint-format**: Shared format types for public interface representation
- **tar/flate2**: Tarball handling for npm package downloads
- **glob**: Pattern matching for file inclusion/exclusion

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
Will use https://registry.npmjs.org/ as registry.
Loading previous version from: ./examples/v1
Converting packages to format...
Previous: 15 exports across 1 modules
Current: 17 exports across 1 modules

Sending to Interlint API for analysis...

  No breaking changes detected.

Analysis completed in 0.89s
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

## Related Projects

- **interlint-format**: Shared types for representing public interfaces
- **interlint-api**: Server-side breaking change analysis engine

## License

MIT
