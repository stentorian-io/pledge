# Interlint CLI Test Examples

This directory contains example packages for testing the interlint CLI's breaking change detection.

## Directory Structure

```
examples/
├── v1/                    # "Previous" version (baseline)
│   ├── package.json
│   └── src/
│       └── index.ts
├── v2-breaking/           # "Current" version WITH breaking changes
│   ├── package.json
│   └── src/
│       └── index.ts
└── v2-compatible/         # "Current" version WITHOUT breaking changes
    ├── package.json
    └── src/
        └── index.ts
```

## What Each Version Contains

### v1 (Baseline)
- `createUser(name: string, email: string): User` - Function
- `class UserService { getUser(id: number): User }` - Class with method
- `interface Config { apiKey: string; timeout?: number }` - Interface with optional property
- `type Status = 'active' | 'inactive'` - Union type
- `const VERSION: string` - Constant export

### v2-breaking (Breaking Changes)
| Export | Change | Severity |
|--------|--------|----------|
| `createUser` | Added required `role` parameter | **MAJOR** |
| `UserService.getUser` | Changed param type `number` -> `string` | **MAJOR** |
| `Config.timeout` | Optional became required | **MAJOR** |
| `Status` | Added `'pending'` to union | **MINOR** |
| `VERSION` | Removed entirely | **MAJOR** |

### v2-compatible (No Breaking Changes)
| Export | Change | Severity |
|--------|--------|----------|
| `createUser` | Added optional `role?` parameter | **MINOR** |
| `UserService.getUser` | Same signature | - |
| `Config` | Added optional `debug?` property | **MINOR** |
| `Status` | Same type | - |
| `VERSION` | Same value | - |
| `formatUser` | New function added | **MINOR** |

## Testing Instructions

### Prerequisites

1. Start the interlint API server:
   ```bash
   # From the api directory
   cd ../api
   cargo run
   ```
   The API should be running on `http://localhost:3000`

2. Build the CLI:
   ```bash
   # From the cli directory
   cd ..
   cargo build --release
   ```

### CLI Usage

```bash
# Compare two local directories
breakpoint --previous <previous-path> <current-path>

# Compare against npm registry (fetches previous version automatically)
breakpoint <current-path>
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `INTERLINT_API_URL` | API server URL | `http://localhost:3000` |

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | No breaking changes detected |
| 1 | Breaking changes detected |

### Manual Testing

#### Test Breaking Changes Detection

Compare v1 against v2-breaking (should detect breaking changes):

```bash
# Using the built CLI
./target/release/breakpoint --previous ./examples/v1 ./examples/v2-breaking

# Or using cargo run
cargo run -- --previous ./examples/v1 ./examples/v2-breaking
```

**Expected output:** 6 breaking changes detected:
- `VERSION`: Export removed (CRITICAL)
- `createUser`: Required parameter added (CRITICAL)
- `Config.timeout`: Optional became required (CRITICAL)
- `UserService.users`: Type changed (MAJOR)
- `UserService.getUser`: Parameter type changed (MAJOR)
- `Status`: Union type expanded (MAJOR)

#### Test Compatible Changes

Compare v1 against v2-compatible (should NOT detect breaking changes):

```bash
cargo run -- --previous ./examples/v1 ./examples/v2-compatible
```

**Expected output:** No breaking changes (only minor/patch changes)

### Quick Test Script

```bash
#!/bin/bash
set -e

echo "=== Building CLI ==="
cargo build --release

echo ""
echo "=== Test 1: Breaking Changes (v1 -> v2-breaking) ==="
./target/release/breakpoint --previous ./examples/v1 ./examples/v2-breaking || true

echo ""
echo "=== Test 2: Compatible Changes (v1 -> v2-compatible) ==="
./target/release/breakpoint --previous ./examples/v1 ./examples/v2-compatible

echo ""
echo "=== All tests completed ==="
```

### Using with npm Registry (Alternative)

If you want to test with real npm packages, the CLI can fetch the previous version from npm:

```bash
# The CLI will fetch the latest published version as "previous"
cargo run -- ./path/to/local/package
```

## API Interaction

The CLI sends requests to the interlint API at `POST /v1/compare`:

```json
{
  "previous": { /* PublicInterface */ },
  "current": { /* PublicInterface */ }
}
```

Response includes breaking changes with severity levels and suggested version bump.

## Adding More Test Cases

To add new test scenarios:

1. Create a new version directory (e.g., `v3-type-widening/`)
2. Add `package.json` with appropriate name and version
3. Add `src/index.ts` with the exports to test
4. Document the expected behavior in this README

## Common Test Scenarios

| Scenario | Directory | Expected Result |
|----------|-----------|-----------------|
| Breaking: required param added | v2-breaking | MAJOR |
| Breaking: param type changed | v2-breaking | MAJOR |
| Breaking: optional -> required | v2-breaking | MAJOR |
| Breaking: export removed | v2-breaking | MAJOR |
| Minor: optional param added | v2-compatible | MINOR |
| Minor: union type expanded | v2-breaking | MINOR |
| Minor: new export added | v2-compatible | MINOR |
