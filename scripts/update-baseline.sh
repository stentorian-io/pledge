#!/usr/bin/env bash
# scripts/update-baseline.sh -- Update the coverage baseline to current measured value
# Run this after intentionally improving test coverage to raise the threshold.
set -euo pipefail

if ! command -v cargo-llvm-cov &>/dev/null; then
    echo "ERROR: cargo-llvm-cov is not installed."
    echo "  Install with:"
    echo "    rustup component add llvm-tools"
    echo "    cargo install cargo-llvm-cov"
    exit 1
fi

echo "Measuring current coverage with cargo-llvm-cov..."
NEW_COV=$(cargo llvm-cov --json --summary-only | \
    python3 -c "import sys,json; d=json.load(sys.stdin); print(int(d['data'][0]['totals']['lines']['percent']))")

OLD_COV="(none)"
if [ -f .coverage-baseline ]; then
    OLD_COV=$(cat .coverage-baseline)
fi

echo "$NEW_COV" > .coverage-baseline
echo "Coverage baseline updated: ${OLD_COV}% -> ${NEW_COV}%"
