# Breakpoint CLI - Quick Reference

Fast reference guide for common Breakpoint CLI tasks.

---

## Installation & Setup

```bash
# Clone repository
git clone https://github.com/interlint/interlint.git
cd interlint

# Build API server (Terminal 1)
cd api
cargo build --release
./target/release/interlint-api

# Build CLI (Terminal 2)
cd cli
cargo build --release
```

---

## Basic Usage

```bash
# Compare against npm registry (requires API running)
breakpoint /path/to/package

# Compare two local versions
breakpoint --previous ./v1 ./v2

# With custom API URL
INTERLINT_API_URL=http://api.example.com:3000 breakpoint /path/to/package
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | No breaking changes found |
| 1 | Breaking changes detected or error occurred |

```bash
breakpoint .
if [ $? -eq 0 ]; then
  echo "✅ Safe to publish"
else
  echo "❌ Breaking changes detected"
fi
```

---

## Common Tasks

### Check Before Publishing

```bash
cd my-package
npm run build
breakpoint .
# Check exit code
echo $?
```

### Setup API Server

```bash
# Development
cd api && cargo run

# Production with systemd
sudo systemctl start interlint-api
sudo systemctl status interlint-api
```

### Health Check API

```bash
curl http://localhost:3000/health
# Response: {"status":"healthy","version":"0.1.0"}
```

### Test with Examples

```bash
cd cli
./target/release/breakpoint --previous ./examples/v1 ./examples/v2-breaking
```

---

## Environment Variables

| Variable | Default | Example |
|----------|---------|---------|
| `INTERLINT_API_URL` | `http://localhost:3000` | `http://api.example.com:3000` |

```bash
export INTERLINT_API_URL=http://localhost:8080
breakpoint .
```

---

## CI/CD Integration

### GitHub Actions

```yaml
- name: Check breaking changes
  run: |
    cd api && cargo build --release && ./target/release/interlint-api &
    cd ../cli && cargo build --release && ./target/release/breakpoint .
```

### Local Testing

```bash
# Fast local comparison
breakpoint --previous ./main ./feature-branch
```

---

## Troubleshooting

### Connection Error

```bash
# Start API server
cd api && cargo run

# Verify it's running
curl http://localhost:3000/health
```

### No Exports Found

```bash
# Check package.json
cat package.json | jq .main,.exports

# Verify exports exist
grep "^export" src/index.ts
```

### Pack Failed

```bash
# Run pack manually to see error
npm pack
pnpm pack
yarn pack
bun pm pack
```

### Registry Error

```bash
# Use local comparison instead
breakpoint --previous ./v1 ./v2

# Or check registry connectivity
curl https://registry.npmjs.org/my-package
```

---

## Output Format

### No Breaking Changes

```
Loading current package...
Detected npm from lockfile: package-lock.json
Checking registry for previous version of 'my-package'...

No breaking changes detected.

Analysis completed in 0.89s
```

### Breaking Changes Found

```
Breaking Changes Detected:
===========================

CRITICAL (1):
  - Export 'VERSION' was removed

MAJOR (2):
  - Parameter 'role' was added to 'createUser' (required)
  - Property 'timeout' changed from optional to required

Summary:
--------
Total breaking changes: 3
Previous exports: 15 → Current exports: 14
```

### First Publish

```
⚠️  No previous version found for 'my-new-package'.
   This appears to be the first publish of this package.
   Skipping breaking change comparison.

Analyzing package: my-new-package v1.0.0
Package has 8 exports across 2 modules

Analysis completed in 0.45s
```

---

## Common Errors

| Error | Solution |
|-------|----------|
| `Connection refused` | Start API: `cd api && cargo run` |
| `Failed to load package` | Check `package.json` is valid: `npm validate` |
| `No exports found` | Check files use `export` keyword |
| `No previous version` | Normal for first publish, use `--previous` flag |
| `Pack failed` | Run manually: `npm pack` |

See [ERROR_HANDLING.md](./ERROR_HANDLING.md) for full reference.

---

## File Locations

| File | Location |
|------|----------|
| CLI binary (debug) | `cli/target/debug/breakpoint` |
| CLI binary (release) | `cli/target/release/breakpoint` |
| API binary (debug) | `api/target/debug/interlint-api` |
| API binary (release) | `api/target/release/interlint-api` |
| Test examples | `cli/examples/` |

---

## Useful Commands

```bash
# Build CLI
cd cli && cargo build --release

# Run tests
cargo test

# Format code
cargo fmt

# Lint code
cargo clippy

# Clean build artifacts
cargo clean

# Check compilation without building
cargo check

# Run with logging
RUST_LOG=debug cargo run
```

---

## Documentation Quick Links

| Topic | Document |
|-------|----------|
| Setup & Install | [README.md](../README.md) |
| Common Errors | [ERROR_HANDLING.md](./ERROR_HANDLING.md) |
| Troubleshooting | [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) |
| Server Setup | [SERVER_REQUIREMENTS.md](./SERVER_REQUIREMENTS.md) |
| Best Practices | [BEST_PRACTICES.md](./BEST_PRACTICES.md) |
| Binary Format | [BINARY_FORMAT.md](./BINARY_FORMAT.md) |

---

## API Usage

### Check Health

```bash
curl http://localhost:3000/health
```

### Compare Packages (Rust)

```rust
use interlint_format::{binary, CompareRequest};

let request = CompareRequest {
    previous: prev_interface,
    current: curr_interface,
    min_severity: None,
    include_additions: false,
};

let body = binary::serialize(&request)?;
let response = client
    .post("http://localhost:3000/v1/compare")
    .header("Content-Type", "application/octet-stream")
    .body(body)
    .send()
    .await?;
```

---

## Semantic Versioning

Based on breaking changes detected:

| Changes | Suggested Version |
|---------|------------------|
| None | Patch (x.x.X) |
| Only warnings | Patch (x.x.X) |
| Minor changes | Minor (x.X.0) |
| Major/Critical | Major (X.0.0) |

---

## Development

### Project Structure

```
interlint/
├── format/      # Core data types
├── api/         # REST API server
└── cli/         # CLI tool
    ├── src/
    │   ├── main.rs
    │   ├── api.rs
    │   ├── converter.rs
    │   ├── ecma/
    │   ├── fs/
    │   ├── pkg/
    │   └── parser/
    ├── examples/
    └── docs/
```

### Key Files

| File | Purpose |
|------|---------|
| `cli/src/main.rs` | CLI entry point |
| `cli/src/api.rs` | API client |
| `api/src/main.rs` | API server entry |
| `format/lib.rs` | Data structures |

---

## Performance Tips

```bash
# Use release build (much faster)
cargo build --release

# For large packages, use local comparison
breakpoint --previous ./v1 ./v2

# Run API server in background
cargo run > api.log 2>&1 &
```

---

## Debugging

```bash
# Enable verbose logging
RUST_LOG=debug breakpoint /path/to/package

# Check API logs
tail -f api.log

# Verify API is running
ps aux | grep interlint-api

# Test API endpoint
curl -v http://localhost:3000/health

# Check port is listening
lsof -i :3000
```

---

## Resources

- **GitHub:** https://github.com/interlint/interlint
- **Tree-sitter:** https://tree-sitter.github.io/tree-sitter/
- **MessagePack:** https://msgpack.org/
- **Rust Book:** https://doc.rust-lang.org/book/

---

## Tips & Tricks

### Pre-publish Checklist

```bash
✓ Run tests: npm test
✓ Build package: npm run build
✓ Check breaking changes: breakpoint .
✓ Update CHANGELOG: vim CHANGELOG.md
✓ Commit changes: git commit -am "..."
✓ Tag version: git tag v2.0.0
✓ Publish: npm publish
```

### CI/CD Template

```bash
#!/bin/bash
set -e

# Start API
cd api && cargo build --release && ./target/release/interlint-api &
sleep 2

# Build and test
cd ../cli
cargo build --release
npm run build

# Check for breaking changes
./target/release/breakpoint . || {
  echo "Breaking changes detected"
  exit 1
}

echo "✅ All checks passed"
```

### Quick Test

```bash
# Create minimal test package
mkdir test && cd test
echo '{"name":"test","version":"1.0.0","main":"index.js"}' > package.json
echo 'export function foo() {}' > index.js

# Run analysis
breakpoint .
```

---

## Getting Help

1. Check [ERROR_HANDLING.md](./ERROR_HANDLING.md) for your error
2. Review [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) for detailed solutions
3. See [BEST_PRACTICES.md](./BEST_PRACTICES.md) for integration examples
4. Read [SERVER_REQUIREMENTS.md](./SERVER_REQUIREMENTS.md) for setup

---

## Version Info

- **Breakpoint CLI:** 0.1.0
- **Interlint API:** 0.1.0
- **Minimum Rust:** 1.70
- **Last Updated:** January 2026

---

For complete documentation, see the docs directory.
