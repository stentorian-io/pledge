# Breakpoint CLI - Best Practices

Best practices for using Breakpoint CLI in your development workflow and CI/CD pipelines.

---

## Local Development Workflow

### 1. Validate Before Publishing

Always run Breakpoint locally before publishing to npm:

```bash
# Navigate to your package
cd my-package

# Start API server (in another terminal)
cd ../api && cargo run

# Run analysis (from your package directory)
cd ../my-package
breakpoint .

# Check exit code
echo $?  # Should be 0 for no breaking changes
```

### 2. Compare Specific Versions

For testing changes before committing:

```bash
# Compare against your main branch
git checkout main
cd ../main-version
npm pack  # or pnpm pack

# Go back to your branch
git checkout feature-branch
cd ../feature-version

# Compare
breakpoint --previous ../main-version ../feature-version
```

### 3. Use Local Comparison for Faster Iteration

Local comparison is faster and doesn't require registry lookup:

```bash
# Build your package first
npm run build

# Fast local comparison (no registry call)
breakpoint --previous ./dist-v1 ./dist-v2

# This is great for:
# - Testing during development
# - CI/CD pipelines (no external dependencies)
# - Private packages
# - Offline environments
```

---

## CI/CD Integration

### GitHub Actions

#### Basic Setup

```yaml
name: Breaking Change Detection

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

      # Checkout main branch for comparison
      - name: Checkout main
        run: git fetch origin main:refs/remotes/origin/main

      # Setup Rust toolchain
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.70

      # Build API server
      - name: Build Interlint API
        run: |
          cd api
          cargo build --release
          ./target/release/interlint-api &
          sleep 2  # Wait for server to start

      # Build CLI
      - name: Build Breakpoint CLI
        run: |
          cd cli
          cargo build --release

      # Build the package
      - name: Build package
        run: npm ci && npm run build

      # Run breaking change check
      - name: Check for breaking changes
        run: |
          cd cli
          ./target/release/breakpoint --previous <(git show origin/main:package.json) ../

      # Fail if breaking changes found
      - name: Process results
        if: failure()
        run: echo "Breaking changes detected - PR must be reviewed"
```

#### Advanced Setup with Caching

```yaml
name: Breaking Change Detection (Cached)

on:
  pull_request:
    branches: [main]

jobs:
  check-breaking-changes:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      # Cache Rust toolchain and build artifacts
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: |
            api
            cli

      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.70

      - name: Start API server
        run: |
          cd api
          cargo build --release
          ./target/release/interlint-api &
          echo $! > /tmp/api.pid
          sleep 2

      - name: Build and check package
        run: |
          cd cli
          cargo build --release
          npm ci && npm run build
          ./target/release/breakpoint .

      - name: Stop API server
        if: always()
        run: kill $(cat /tmp/api.pid) 2>/dev/null || true

      - name: Comment results
        if: failure()
        uses: actions/github-script@v6
        with:
          script: |
            github.rest.issues.createComment({
              issue_number: context.issue.number,
              owner: context.repo.owner,
              repo: context.repo.repo,
              body: '❌ Breaking changes detected in this PR. Please review the changes.'
            })
```

### GitLab CI

```yaml
breaking-changes:
  stage: test
  image: rust:1.70

  script:
    # Start API server
    - cd api && cargo build --release && ./target/release/interlint-api &
    - sleep 2

    # Check package
    - cd ../cli
    - cargo build --release
    - cd ..
    - npm ci && npm run build
    - ./cli/target/release/breakpoint .

  artifacts:
    reports:
      junit: results.xml
```

### Jenkins

```groovy
pipeline {
    agent any

    stages {
        stage('Setup') {
            steps {
                checkout scm
                sh '''
                    rustc --version
                    cargo --version
                '''
            }
        }

        stage('Start API') {
            steps {
                sh '''
                    cd api
                    cargo build --release
                    nohup ./target/release/interlint-api > /tmp/api.log 2>&1 &
                    sleep 2
                    curl http://localhost:3000/health
                '''
            }
        }

        stage('Build CLI') {
            steps {
                sh '''
                    cd cli
                    cargo build --release
                    npm ci
                    npm run build
                '''
            }
        }

        stage('Check Breaking Changes') {
            steps {
                sh '''
                    cd cli
                    ./target/release/breakpoint .
                '''
            }
        }
    }

    post {
        always {
            sh 'pkill -f interlint-api || true'
        }
    }
}
```

---

## Package.json Configuration

### Ensure Proper Entry Points

Breakpoint analyzes entry points from package.json. Configure them correctly:

```json
{
  "name": "my-package",
  "version": "1.0.0",

  "main": "./dist/index.js",
  "types": "./dist/index.d.ts",

  "exports": {
    ".": {
      "import": "./dist/index.esm.js",
      "require": "./dist/index.js",
      "types": "./dist/index.d.ts"
    },
    "./utils": {
      "import": "./dist/utils.esm.js",
      "require": "./dist/utils.js"
    }
  },

  "browser": "./dist/browser.js",
  "bin": {
    "my-cli": "./bin/cli.js"
  }
}
```

### Best Practices for Entry Points

1. **Always include `types` field** - Helps TypeScript consumers
   ```json
   {
     "main": "./dist/index.js",
     "types": "./dist/index.d.ts"
   }
   ```

2. **Use conditional exports** for ESM/CJS
   ```json
   {
     "exports": {
       ".": {
         "import": "./dist/index.esm.js",
         "require": "./dist/index.js"
       }
     }
   }
   ```

3. **Specify browser entry point** if different
   ```json
   {
     "browser": "./dist/browser.js"
   }
   ```

---

## Pre-release Workflow

### Version Management

Before publishing a new version:

1. **Update package.json version:**
   ```json
   {
     "version": "2.0.0"  // Increment based on changes
   }
   ```

2. **Build the package:**
   ```bash
   npm run build
   ```

3. **Run Breakpoint:**
   ```bash
   breakpoint .
   ```

4. **Review changes:**
   - Check if suggested version bump matches your intent
   - Verify all exports are correctly detected
   - Look for unexpected breaking changes

5. **Publish:**
   ```bash
   npm publish
   ```

### Semantic Versioning Guide

Based on Breakpoint results:

- **No breaking changes found:** Use `patch` (x.x.X)
- **Minor changes (async modifier):** Use `minor` (x.X.0)
- **Major changes (signature, property removed):** Use `major` (X.0.0)
- **Critical changes (export removed):** Use `major` (X.0.0)

---

## Testing Strategies

### Test Different Export Types

Create test fixtures to verify Breakpoint handles all your export types:

```typescript
// v1
export function simpleFunc(x: number): string {}
export class MyClass {}
export interface MyInterface {}
export type MyType = string | number
export enum MyEnum { A, B, C }
export const MY_CONSTANT = 42

// v2 (with breaking changes)
export function simpleFunc(x: number, y?: number): string {}  // Added optional param (OK)
export class MyClass {}  // OK
// export interface MyInterface {}  // Removed (BREAKING)
export type MyType = string | number | boolean  // Type union expanded (NOT BREAKING in many cases)
```

Run analysis:
```bash
breakpoint --previous ./v1 ./v2
```

### Test Re-exports

Verify re-exports are handled correctly:

```typescript
// index.ts
export { MyClass } from './my-class'
export * from './utils'
export { default as defaultExport } from './default'
```

---

## Performance Optimization

### Large Package Analysis

For packages with many exports:

1. **Use release build of API server:**
   ```bash
   cargo build --release
   PORT=3000 ./target/release/interlint-api
   ```

2. **Use local comparison** (faster):
   ```bash
   breakpoint --previous ./prev ./curr
   # vs
   breakpoint .  # Requires registry lookup
   ```

3. **Consider package splitting** if very large:
   - Large packages are slower to analyze
   - Splitting improves CLI startup time
   - Easier for consumers to understand

### Caching Strategies

In CI/CD, cache build artifacts:

```bash
# Cache the API and CLI binaries
- name: Cache binaries
  uses: actions/cache@v3
  with:
    path: |
      api/target/release/interlint-api
      cli/target/release/breakpoint
    key: ${{ runner.os }}-interlint-${{ hashFiles('Cargo.lock') }}
```

---

## Error Handling in CI/CD

### Fail on Breaking Changes

```bash
set -e  # Exit on any error

# Run analysis
breakpoint . || {
  echo "❌ Breaking changes detected"
  exit 1
}

echo "✅ No breaking changes"
```

### Conditional Failure

Allow breaking changes in certain cases:

```bash
# Get exit code
breakpoint .
exit_code=$?

case $exit_code in
  0)
    echo "✅ No breaking changes"
    exit 0
    ;;
  1)
    # Check if this is an approved breaking change
    if [[ "$CI_COMMIT_MESSAGE" == *"[BREAKING]"* ]]; then
      echo "⚠️  Breaking changes found but approved via commit message"
      exit 0
    fi
    echo "❌ Breaking changes detected"
    exit 1
    ;;
esac
```

### Logging and Reporting

```bash
# Save output to file for review
breakpoint . > breaking-changes.log 2>&1
exit_code=$?

# Upload as artifact
if [ $exit_code -eq 1 ]; then
  echo "Breaking changes log:"
  cat breaking-changes.log
fi

exit $exit_code
```

---

## Monitoring and Alerting

### Tracking Breaking Changes Over Time

```bash
# Run regularly and log results
cat >> breaking-changes-history.log << EOF
Date: $(date)
Package: $(npm pkg get name)
Version: $(npm pkg get version)
Exit Code: $(breakpoint . ; echo $?)
---
EOF
```

### Alert on Unexpected Changes

```bash
# If analysis detects changes when none expected
if breakpoint . > /dev/null 2>&1; then
  echo "No changes found - OK"
else
  # Send alert
  curl -X POST https://your-monitoring.example.com/alert \
    -d "Breaking changes detected unexpectedly"
  exit 1
fi
```

---

## Documentation Updates

### Keep Docs in Sync

After running Breakpoint:

1. **Update CHANGELOG:**
   ```markdown
   ## [2.0.0] - 2024-01-15

   ### Breaking Changes
   - Removed `oldFunction` export
   - Changed `Config` interface
   ```

2. **Update API documentation:**
   - Document removed exports
   - Provide migration guides
   - Update examples

3. **Add migration guide** if major breaking changes:
   ```markdown
   # Migration Guide v1.x → v2.x

   ## Removed exports
   - `oldFunction` → use `newFunction` instead

   ## Changed interfaces
   - `Config.timeout` now required (was optional)
   ```

---

## Troubleshooting CI/CD Issues

### API Server Won't Start

```bash
# Check port conflicts
lsof -i :3000

# Use different port if needed
PORT=3001 cargo run

# In CI script
PORT=3001 ./target/release/interlint-api &
export INTERLINT_API_URL=http://localhost:3001
breakpoint .
```

### Build Cache Issues

```bash
# Clear cache and rebuild
cargo clean
cargo build --release
```

### Timeout in CI

If analysis times out:

1. Increase timeout (if supported by CI platform)
2. Use local comparison instead of registry lookup
3. Split large packages into smaller modules
4. Check if API server is running properly

---

## Examples

### Complete Pre-publish Script

```bash
#!/bin/bash
set -e

echo "🔍 Running pre-publish checks..."

# Build package
echo "📦 Building package..."
npm run build

# Start API server
echo "🚀 Starting API server..."
cd api
cargo build --release
./target/release/interlint-api > /tmp/api.log 2>&1 &
API_PID=$!
sleep 2
cd ..

# Run Breakpoint
echo "🔬 Checking for breaking changes..."
cd cli
./target/release/breakpoint . || {
  kill $API_PID
  echo "❌ Breaking changes detected!"
  exit 1
}
cd ..

# Cleanup
kill $API_PID

echo "✅ All checks passed!"
echo "Ready to publish:"
npm publish --dry-run
```

### GitHub Action Status Badge

Add to your README:

```markdown
[![Breaking Changes Check](https://github.com/YOUR_ORG/YOUR_REPO/actions/workflows/breaking-changes.yml/badge.svg)](https://github.com/YOUR_ORG/YOUR_REPO/actions/workflows/breaking-changes.yml)
```

---

## Summary

Key points for best practices:

1. **Always run locally before publishing**
2. **Use CI/CD automation** for all pull requests
3. **Cache builds** to improve performance
4. **Document breaking changes** in CHANGELOG
5. **Test with different export types**
6. **Monitor results** over time
7. **Handle errors gracefully** in scripts
8. **Keep documentation updated** with changes
