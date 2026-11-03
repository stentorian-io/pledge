#!/usr/bin/env bash
# scripts/regen-reference-suite.sh
#
# Regenerates tests/fixtures/node/reference/<pkg>/expected-findings.json
# for one or all reference packages by running the harness pipeline
# and writing candidate findings, public-surface counts, and incomplete evidence.
#
# Usage:
#   ./scripts/regen-reference-suite.sh           # regenerate active packages
#   ./scripts/regen-reference-suite.sh react     # regenerate ONLY react
#
# After regeneration, MANUALLY review the diff vs the previous expected-findings.json
# against the package's release notes / changelog. Commit only when the diff aligns
# with intentional changes to Pledge detection behavior.
#
# Keep regeneration manual; verification must compare the reviewed baseline.

set -euo pipefail

PKGS=("react" "tanstack-query" "trpc" "zod" "prisma")
if [[ $# -gt 1 ]]; then
    echo "Usage: $0 [react|tanstack-query|trpc|zod|prisma]" >&2
    exit 1
fi
if [[ $# -eq 1 ]]; then
    case "$1" in
        react|tanstack-query|trpc|zod|prisma) PKGS=("$1") ;;
        *) echo "Unknown reference package: $1" >&2; exit 1 ;;
    esac
fi

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

# Build the snapshot generator explicitly registered in Cargo.toml.
cargo build --locked --release --example reference_suite_dump

for PKG in "${PKGS[@]}"; do
    OUT="tests/fixtures/node/reference/$PKG/expected-findings.json"
    echo "[regen] Generating $OUT ..."
    cargo run --quiet --locked --release --example reference_suite_dump -- "$PKG" > "$OUT.tmp"
    # Validate JSON syntax before swap.
    if ! python3 -c "import json, sys; json.load(sys.stdin)" < "$OUT.tmp" 2>/dev/null; then
        echo "[regen] FAIL: generated $OUT.tmp is not valid JSON" >&2
        exit 1
    fi
    mv "$OUT.tmp" "$OUT"
    echo "[regen] OK: $OUT"
done

echo
echo "[regen] Done. MANUALLY review the diff:"
echo "  git diff tests/fixtures/node/reference/*/expected-findings.json"
echo "Then commit if the changes align with intentional behavior shifts."
