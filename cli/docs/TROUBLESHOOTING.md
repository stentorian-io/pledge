# Breakpoint CLI - Troubleshooting Guide

Common issues and solutions for the Breakpoint CLI tool.

## Connection Issues

### "Connection refused" / "Failed to connect to API"

**Error:**
```
Error: Failed to connect to http://localhost:3000
Connection refused (os error 111)
```

**Causes:**
- API server is not running
- API server is running on a different port or host
- Network connectivity issue

**Solutions:**

1. **Start the API server** (if not running):
   ```bash
   cd ../api
   cargo run
   # Server will start at http://localhost:3000
   ```

2. **Check if API is listening on the expected port:**
   ```bash
   # macOS/Linux
   lsof -i :3000

   # Windows (PowerShell)
   netstat -ano | findstr :3000
   ```

3. **Use a custom API URL** if running on different host/port:
   ```bash
   export INTERLINT_API_URL=http://example.com:8080
   breakpoint /path/to/package
   ```

4. **Verify the API server is healthy:**
   ```bash
   curl http://localhost:3000/health
   # Should return: {"status":"healthy","version":"0.1.0"}
   ```

### Timeout errors

**Error:**
```
Error: Request timeout (operation timed out)
```

**Solutions:**
- The API server may be overloaded or slow
- Check API server performance and logs
- Verify network latency with: `ping -c 3 localhost`
- API requests have a 30-second timeout - check if packages are very large

### Wrong Content-Type error from API

**Error:**
```
Error: Expected binary MessagePack format (Content-Type: application/octet-stream or application/msgpack), got: application/json
```

**Solution:**
- This indicates the API received a request with incorrect Content-Type header
- The CLI should automatically send the correct content type
- Check if there's a proxy or firewall modifying request headers
- Ensure API version is compatible with CLI version (both should be v0.1.0+)

---

## Package Loading Issues

### "Failed to load and pack current package"

**Error:**
```
Error: Failed to load and pack current package.
```

**Causes:**
- Package manager not installed
- Invalid package structure (missing package.json)
- File permission issues

**Solutions:**

1. **Verify package manager is installed:**
   ```bash
   npm --version    # Check npm
   pnpm --version   # Check pnpm
   yarn --version   # Check yarn
   bun --version    # Check bun
   ```

2. **Verify package.json exists and is valid:**
   ```bash
   cat package.json | json_pp  # Pretty-print JSON to check validity
   ```

3. **Check file permissions:**
   ```bash
   ls -la package.json
   chmod 644 package.json  # Ensure readable
   ```

4. **Verify package manager is detected correctly:**
   ```bash
   # Check for lockfiles in the package directory
   ls -la package-lock.json pnpm-lock.yaml yarn.lock bun.lockb 2>/dev/null || echo "No lockfiles found"
   ```

### "Failed to detect package manager"

**Solutions:**
- Ensure at least one lockfile exists (package-lock.json, pnpm-lock.yaml, yarn.lock, or bun.lockb)
- Or ensure `packageManager` field is set in package.json:
  ```json
  {
    "packageManager": "npm@10.0.0"
  }
  ```

### "Pack command failed"

**Error:**
```
Error: Pack command failed with status code 1
```

**Solutions:**

1. **Run pack manually to see the error:**
   ```bash
   npm pack      # For npm
   pnpm pack     # For pnpm
   yarn pack     # For yarn
   bun pm pack   # For bun
   ```

2. **Common pack issues:**
   - Invalid package.json (check with `npm validate`)
   - Missing main entry point
   - Issues with package.json fields (main, exports, browser, bin)

3. **Verify package.json has valid entry points:**
   ```json
   {
     "name": "my-package",
     "version": "1.0.0",
     "main": "./dist/index.js",
     "exports": {
       ".": "./dist/index.js"
     }
   }
   ```

---

## Registry Issues

### "Failed to check registry" or registry lookup fails

**Error:**
```
Warning: Could not check registry: Connection error
Skipping breaking change comparison.
```

**Causes:**
- Network connectivity issue
- npm registry is down
- Configured registry is unavailable
- Package name doesn't exist

**Solutions:**

1. **Check network connectivity:**
   ```bash
   ping registry.npmjs.org
   curl -I https://registry.npmjs.org
   ```

2. **Verify npm registry configuration:**
   ```bash
   npm config get registry
   # Should output: https://registry.npmjs.org/
   ```

3. **For private packages**, configure auth:
   ```bash
   npm login
   npm config set @myorg:registry https://private-registry.example.com
   ```

4. **Use local comparison mode** if registry is unavailable:
   ```bash
   breakpoint --previous ./v1 ./v2
   ```

### "No previous version found" (First publish)

**Message:**
```
⚠️  No previous version found for 'my-new-package'.
   This appears to be the first publish of this package.
   Skipping breaking change comparison.
```

**Solution:**
- This is expected for new packages
- The CLI will analyze the current package structure and exit cleanly
- On subsequent publishes, it will compare against the previous version

---

## Parsing Issues

### "Failed to parse TypeScript/JavaScript files"

**Error:**
```
Error: Failed to parse file: src/index.ts
```

**Causes:**
- Syntax errors in source files
- Unsupported TypeScript features
- File encoding issues (must be UTF-8)

**Solutions:**

1. **Check for syntax errors:**
   ```bash
   npx tsc --noEmit  # For TypeScript projects
   node -c src/index.js  # For JavaScript files (check syntax only)
   ```

2. **Verify file encoding is UTF-8:**
   ```bash
   file -i src/index.ts
   # Should show: charset=utf-8
   ```

3. **Check for unsupported syntax:**
   - Ensure TypeScript/JavaScript syntax is valid
   - Some experimental features may not be supported
   - Try compiling with `tsc` to identify issues

### Exports not being detected

**Symptoms:**
- "0 exports found" when package clearly has exports
- Specific exports missing from the analysis

**Causes:**
- Exports are not using the `export` keyword
- Using CommonJS exports instead of ES modules
- Conditional exports not being detected
- Dynamic imports/exports

**Solutions:**

1. **Use `export` keyword** for ES modules:
   ```typescript
   // ✅ Detected
   export function myFunc() {}

   // ❌ Not detected (CommonJS)
   module.exports = { myFunc };
   ```

2. **Check entry points** are configured correctly in package.json:
   ```json
   {
     "main": "./dist/index.js",
     "exports": {
       ".": "./dist/index.js",
       "./utils": "./dist/utils.js"
     }
   }
   ```

3. **For CommonJS packages**, ensure distribution files contain ES module syntax:
   - The parser looks for `export` statements
   - If package.json `main` points to CommonJS, conversion may be needed

---

## Output and Analysis Issues

### "Breaking changes detected" but changes seem incorrect

**Solutions:**

1. **Verify the previous version was fetched correctly:**
   ```bash
   # Check exact version being compared
   breakpoint --previous /path/to/v1 /path/to/v2
   # This compares local directories for precise control
   ```

2. **Run verbose analysis** to understand the comparison:
   - Check if both versions are using the correct entry points
   - Verify package names and versions match

3. **Check export paths** are correctly reported:
   - Format: `path/to/file.ts#exportName`
   - This helps identify exactly which export changed

### Exit code is 0 but changes were reported

**Expected behavior:**
- Exit code 0 means no breaking changes detected
- Exit code 1 means breaking changes were found

If you see reported changes but exit code 0:
- Ensure you're checking the `$?` value correctly
- Some shell configurations may modify exit codes

---

## File System Issues

### Permission denied errors

**Error:**
```
Error: Permission denied (os error 13)
```

**Solutions:**

1. **Check file permissions:**
   ```bash
   ls -la /path/to/package
   chmod -R u+r /path/to/package  # Make readable
   ```

2. **Run with appropriate privileges:**
   ```bash
   # If needed (not generally recommended)
   sudo breakpoint /path/to/package
   ```

3. **Check directory is accessible:**
   ```bash
   cd /path/to/package && pwd
   ```

### Disk space issues

**Error:**
```
Error: No space left on device
```

**Solutions:**
- Temporary files are extracted to `/tmp` (or equivalent)
- Ensure adequate disk space: `df -h`
- Large packages (>500MB) may need more space
- Clean up old tarball downloads if storing locally

---

## API Server Issues

### API is running but returns unexpected errors

**Solutions:**

1. **Check API server health:**
   ```bash
   curl -v http://localhost:3000/health
   ```

2. **View API logs** to debug issues:
   ```bash
   # The API should output logs to stdout
   # Check for any error messages about parsing or comparison
   ```

3. **Verify API and CLI versions match:**
   ```bash
   # Both should be v0.1.0 or compatible versions
   cargo --version  # Verify Rust 1.70+
   ```

### "Invalid binary format" error

**Solutions:**
- The CLI should automatically use correct binary format
- If error persists, verify:
  - API and CLI were built from same source
  - No data corruption during transfer
  - API and CLI versions are compatible

---

## Environment Variable Issues

### Custom API URL not working

**Solution:**
1. **Verify environment variable is set:**
   ```bash
   echo $INTERLINT_API_URL
   ```

2. **Set it correctly for your shell:**
   ```bash
   # Bash/Zsh
   export INTERLINT_API_URL=http://example.com:3000

   # PowerShell (Windows)
   $env:INTERLINT_API_URL = "http://example.com:3000"
   ```

3. **Pass as command-line argument** (if supported):
   ```bash
   INTERLINT_API_URL=http://custom-api:3000 breakpoint /path/to/package
   ```

---

## Performance Issues

### Analysis is slow

**Causes:**
- Large packages with many exports
- Slow network connection to API server
- API server is overloaded

**Solutions:**

1. **Check API server performance:**
   - Monitor CPU/memory: `top` or Task Manager
   - Check if it's handling requests quickly

2. **Use local comparison** for faster iteration:
   ```bash
   breakpoint --previous ./v1 ./v2
   # This skips registry lookup and packing
   ```

3. **For very large packages:**
   - Consider splitting into multiple packages
   - Or run analysis during off-peak hours

---

## Platform-Specific Issues

### macOS

**Common issues:**
- Rosetta translation if running on Apple Silicon with x86 binary
- File system case-sensitivity in temporary directories

**Solutions:**
```bash
# Build native binary for your architecture
cargo build --release

# For Apple Silicon
cargo build --release --target aarch64-apple-darwin
```

### Linux

**Package manager detection may fail** if lockfiles don't exist:
```bash
# Ensure package manager is installed
which npm pnpm yarn bun
```

### Windows

**Path handling issues:**
```bash
# Use backslashes or forward slashes
breakpoint "C:\path\to\package"
# or
breakpoint C:/path/to/package
```

---

## Getting Help

If you encounter an issue not covered here:

1. **Check the main README:** `/path/to/interlint/cli/README.md`
2. **View source code documentation:** Rust doc comments in `src/` files
3. **Review example packages:** `cli/examples/` directory
4. **Check related documentation:**
   - `docs/query-patterns-reference.md` - Parser patterns
   - `docs/parser-interface-design.md` - Architecture
   - `docs/package-manager-research.md` - Package manager details

---

## Reporting Bugs

When reporting issues, please include:

1. **Your environment:**
   ```bash
   rustc --version
   cargo --version
   breakpoint --version  # If available
   uname -a  # System info
   ```

2. **Exact command run:**
   ```bash
   breakpoint /path/to/package
   ```

3. **Full error output:**
   - All error messages
   - Exit code (`echo $?`)

4. **Package details:**
   - Package name and version
   - Previous version (if using registry)
   - Entry points in package.json

This helps us debug and fix issues more quickly.
