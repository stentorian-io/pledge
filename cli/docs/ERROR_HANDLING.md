# Breakpoint CLI - Error Handling Guide

Comprehensive guide to understanding and handling errors in Breakpoint CLI.

---

## Error Categories

### 1. Connection Errors

**Cause:** Cannot communicate with API server

#### Error Messages

```
Error: Failed to connect to http://localhost:3000
   Connection refused (os error 111)
```

```
Error: Request timeout (operation timed out)
```

#### Solutions

1. **Verify API server is running:**
   ```bash
   curl http://localhost:3000/health
   # Should return: {"status":"healthy","version":"0.1.0"}
   ```

2. **Check port is accessible:**
   ```bash
   lsof -i :3000  # macOS/Linux
   netstat -ano | findstr :3000  # Windows
   ```

3. **Use correct API URL:**
   ```bash
   export INTERLINT_API_URL=http://localhost:3000
   # or
   INTERLINT_API_URL=http://custom-api.example.com:3000 breakpoint .
   ```

4. **Check network connectivity:**
   ```bash
   ping localhost  # Or your API server hostname
   curl -v http://localhost:3000/health
   ```

---

### 2. Package Loading Errors

**Cause:** Issues loading or packing the current package

#### Error Messages

```
Error: Failed to load and pack current package.
   No such file or directory (os error 2)
```

```
Error: Pack command failed with status code 1
```

#### Solutions

1. **Verify package.json exists:**
   ```bash
   ls -la package.json
   cat package.json | head -5  # Check it's valid JSON
   ```

2. **Check directory permissions:**
   ```bash
   ls -la /path/to/package
   chmod -R u+r /path/to/package  # Make readable
   ```

3. **Verify package manager is installed:**
   ```bash
   npm --version
   which npm pnpm yarn bun
   ```

4. **Run pack command manually:**
   ```bash
   npm pack  # See actual error from package manager
   ```

5. **Check package.json validity:**
   ```bash
   npm validate
   # or manually check with a JSON validator
   ```

---

### 3. Registry Errors

**Cause:** Cannot fetch previous version from npm registry

#### Error Messages

```
Warning: Could not check registry: Connection error
   Skipping breaking change comparison.
```

```
Error: Failed to fetch package from registry
   404 Not Found
```

#### Solutions

1. **Check network connectivity to registry:**
   ```bash
   ping registry.npmjs.org
   curl -I https://registry.npmjs.org
   ```

2. **Verify npm registry configuration:**
   ```bash
   npm config get registry
   # Should output: https://registry.npmjs.org/ (or your custom registry)
   ```

3. **For private packages:**
   ```bash
   npm login
   npm config set @myorg:registry https://private-registry.example.com
   npm config set //private-registry.example.com/:_authToken "your-token"
   ```

4. **Use local comparison if registry unavailable:**
   ```bash
   breakpoint --previous ./v1 ./v2
   # This doesn't require registry access
   ```

5. **Check package name exists:**
   ```bash
   npm search my-package --json
   # or
   curl https://registry.npmjs.org/my-package
   ```

---

### 4. Parsing Errors

**Cause:** Cannot parse TypeScript/JavaScript files

#### Error Messages

```
Error: Failed to parse file: src/index.ts
   Unexpected token '{'
```

```
Error: Failed to parse export
   Invalid syntax in file
```

#### Solutions

1. **Check for syntax errors:**
   ```bash
   npx tsc --noEmit           # TypeScript syntax check
   node -c src/index.js       # JavaScript syntax check
   ```

2. **Verify file encoding is UTF-8:**
   ```bash
   file -i src/index.ts
   # Should show: charset=utf-8
   ```

3. **Check for unsupported syntax:**
   - Ensure valid TypeScript/JavaScript syntax
   - Some experimental features may not be supported
   - Try compiling with current TypeScript version

4. **View the problematic file:**
   ```bash
   cat -n src/index.ts  # Show line numbers
   npx tsc --noEmit src/index.ts  # Get detailed error
   ```

---

### 5. Export Detection Errors

**Cause:** No exports found or exports missing

#### Error Messages

```
Message: Converting packages to format...
Previous: 0 exports across 0 modules
Current: 0 exports across 0 modules
```

#### Solutions

1. **Verify exports use `export` keyword:**
   ```typescript
   // ✅ Detected
   export function myFunc() {}
   export class MyClass {}
   export interface Config {}

   // ❌ Not detected (CommonJS)
   module.exports = { myFunc };
   ```

2. **Check entry points are configured:**
   ```json
   {
     "main": "./dist/index.js",
     "exports": {
       ".": "./dist/index.js"
     }
   }
   ```

3. **Verify distribution files exist:**
   ```bash
   ls -la dist/index.js
   npm run build  # Rebuild if needed
   ```

4. **Check for re-exports:**
   ```typescript
   // ✅ Re-exports are detected
   export { MyClass } from './my-class'
   export * from './utils'

   // Verify target files have exports
   grep "^export" src/my-class.ts
   ```

5. **Review entry point configuration:**
   ```bash
   # Check what breakpoint sees
   cat package.json | jq .main,.exports,.browser,.bin
   ```

---

### 6. Binary Format Errors

**Cause:** API received incorrect format

#### Error Messages

```
Error: Expected binary MessagePack format
   (Content-Type: application/octet-stream or application/msgpack),
   got: application/json
```

```
Error: Invalid binary format
   Checksum mismatch in received data
```

#### Solutions

1. **Verify API and CLI versions match:**
   ```bash
   # Check both are v0.1.0+
   cargo --version
   ```

2. **Check Content-Type header is correct:**
   ```bash
   # CLI should automatically send correct header
   # If using custom client, ensure:
   # Content-Type: application/octet-stream
   ```

3. **Verify binary data integrity:**
   ```bash
   # If using saved .ilmj files
   file my-interface.ilmj
   ls -la my-interface.ilmj  # Check file size is reasonable
   ```

4. **Test with health endpoint (JSON):**
   ```bash
   curl http://localhost:3000/health
   # Should return JSON successfully
   ```

---

### 7. API Error Responses

**Cause:** API returned an error

#### Error Messages (Binary-encoded)

```
{
  "error": "Package names do not match: 'package-a' vs 'package-b'",
  "code": "PACKAGE_NAME_MISMATCH"
}
```

#### Common API Errors

| Code | Cause | Solution |
|------|-------|----------|
| `INVALID_CONTENT_TYPE` | Wrong Content-Type header | Use `application/octet-stream` |
| `INVALID_BINARY_FORMAT` | Malformed MessagePack data | Check binary format, rebuild |
| `PACKAGE_NAME_MISMATCH` | Package names don't match | Ensure comparing same package |
| `PARSE_ERROR` | Invalid API request format | Verify serialization format |
| `INTERNAL_ERROR` | Server error during analysis | Check API server logs |

#### Solutions

1. **Check API server logs:**
   ```bash
   # If running interactively
   RUST_LOG=debug cargo run  # See debug output

   # If running as service
   journalctl -u interlint-api -f  # Linux systemd
   tail -f api.log                  # Other systems
   ```

2. **Verify package comparison:**
   ```bash
   # For PACKAGE_NAME_MISMATCH
   cat package.json | jq .name
   # Both previous and current should have same name
   ```

3. **Check binary data:**
   ```bash
   # Verify file isn't corrupted
   ls -lah *.ilmj
   md5 file.ilmj  # Check integrity if copied
   ```

---

### 8. Performance Issues

**Cause:** Analysis is slow

#### Error Messages

```
Error: Request timeout (operation timed out)
   Analysis took longer than 30 seconds
```

#### Solutions

1. **For very large packages:**
   - Use local comparison (no registry lookup)
   - Consider splitting package into modules
   - Run during off-peak hours

2. **Check API server performance:**
   ```bash
   # Monitor while running analysis
   top  # Or Task Manager on Windows
   ps aux | grep interlint-api
   ```

3. **Use release build of API:**
   ```bash
   cargo build --release
   ./target/release/interlint-api  # Much faster
   ```

4. **Skip registry lookup:**
   ```bash
   breakpoint --previous ./v1 ./v2
   # Faster than registry lookup
   ```

---

### 9. File System Errors

**Cause:** Disk or permission issues

#### Error Messages

```
Error: Permission denied (os error 13)
   Cannot read file
```

```
Error: No space left on device
```

#### Solutions

1. **Fix file permissions:**
   ```bash
   chmod -R u+r /path/to/package
   chmod +x ./target/release/breakpoint
   ```

2. **Check disk space:**
   ```bash
   df -h                    # macOS/Linux
   dir C:                   # Windows
   ```

3. **Clean up temporary files:**
   ```bash
   rm -rf /tmp/interlint-*  # Linux/macOS
   # Windows temp files auto-clean
   ```

---

### 10. Package Manager Errors

**Cause:** Package manager detection or execution failed

#### Error Messages

```
Error: Failed to detect package manager
   No lockfile found (package-lock.json, pnpm-lock.yaml, yarn.lock, bun.lockb)
```

```
Error: Package manager not found: npm
```

#### Solutions

1. **Ensure package manager is installed:**
   ```bash
   npm --version
   pnpm --version
   yarn --version
   bun --version
   ```

2. **Verify lockfile exists:**
   ```bash
   ls -la package-lock.json pnpm-lock.yaml yarn.lock bun.lockb 2>/dev/null
   ```

3. **Specify packageManager in package.json:**
   ```json
   {
     "packageManager": "npm@10.0.0"
   }
   ```

4. **Run in correct directory:**
   ```bash
   cd /path/to/package
   breakpoint .
   ```

---

## Exit Codes and Meanings

| Code | Meaning | Common Causes |
|------|---------|---------------|
| `0` | Success - No breaking changes | Package is compatible |
| `1` | Breaking changes detected OR error occurred | See stderr for details |

**Note:** Both breaking changes and errors return exit code 1. Check stderr to differentiate.

---

## Error Recovery Strategies

### 1. Automatic Retry Logic

```bash
#!/bin/bash

MAX_RETRIES=3
RETRY_DELAY=2

for i in $(seq 1 $MAX_RETRIES); do
  breakpoint . && exit 0

  if [ $i -lt $MAX_RETRIES ]; then
    echo "Attempt $i failed, retrying in ${RETRY_DELAY}s..."
    sleep $RETRY_DELAY
  fi
done

echo "Failed after $MAX_RETRIES attempts"
exit 1
```

### 2. Graceful Degradation

```bash
#!/bin/bash

# Try registry comparison first (fast if successful)
breakpoint . && exit 0

# Fall back to local comparison if registry fails
if [ $? -ne 0 ]; then
  echo "Registry comparison failed, trying local comparison..."
  breakpoint --previous ./main ./feature && exit 0
fi

exit 1
```

### 3. Detailed Error Logging

```bash
#!/bin/bash

{
  breakpoint .
  RESULT=$?
} 2>&1 | tee /tmp/breakpoint-$(date +%s).log

if [ $RESULT -ne 0 ]; then
  echo "Analysis failed. Details saved to:"
  ls -lt /tmp/breakpoint-*.log | head -1 | awk '{print $NF}'
fi

exit $RESULT
```

---

## Debugging Tips

### Enable Verbose Logging

```bash
# For API server
RUST_LOG=debug cargo run

# For CLI (if debugging from source)
RUST_LOG=debug cargo run -- /path/to/package
```

### Check All Components

```bash
echo "=== Checking environment ==="
rustc --version
cargo --version
node --version
npm --version

echo "=== Checking API server ==="
curl -v http://localhost:3000/health

echo "=== Checking package ==="
cat package.json | jq .name,.version

echo "=== Checking file system ==="
ls -la /path/to/package
du -sh /path/to/package
```

### Minimal Test Case

```bash
# Create minimal package to test
mkdir test-pkg
cd test-pkg
cat > package.json << 'EOF'
{
  "name": "test-pkg",
  "version": "1.0.0",
  "main": "index.js"
}
EOF

echo "export function test() {}" > index.js

# Run breakpoint
breakpoint .
```

---

## Getting Help

If you encounter an error not covered here:

1. **Check existing documentation:**
   - [TROUBLESHOOTING.md](./TROUBLESHOOTING.md)
   - [SERVER_REQUIREMENTS.md](./SERVER_REQUIREMENTS.md)

2. **Collect diagnostic information:**
   ```bash
   echo "=== Environment ==="
   rustc --version
   uname -a

   echo "=== Error output ==="
   breakpoint . 2>&1

   echo "=== API health ==="
   curl http://localhost:3000/health
   ```

3. **Create minimal reproduction:**
   - Small package that triggers the error
   - Exact commands run
   - Full error output

4. **Report with context:**
   - Your operating system and version
   - Rust and package manager versions
   - Exact error message (include full stack trace)
   - What you expected to happen

---

## Summary

Key error handling practices:

1. **Always check API server is running** - Most common issue
2. **Verify package.json validity** - Before running breakpoint
3. **Use local comparison for testing** - Faster, no external dependencies
4. **Implement retry logic** - Network issues can be transient
5. **Log everything** - Helps with troubleshooting
6. **Check exit codes** - 0 = no breaking changes, 1 = changes or error
7. **Read error messages carefully** - They contain useful debugging info
