#!/usr/bin/env bash
# scripts/regen-reference-suite.sh
#
# Phase 25 INT-01 helper — regenerates tests/fixtures/reference-suite/<pkg>/expected-findings.json
# for one or all 5 reference packages by running the actual harness pipeline
# in dump-baseline mode and writing the (exit_code, sorted_unique_rule_ids) result.
#
# Usage:
#   ./scripts/regen-reference-suite.sh           # regenerate ALL packages
#   ./scripts/regen-reference-suite.sh react     # regenerate ONLY react
#
# After regeneration, MANUALLY review the diff vs the previous expected-findings.json
# against the package's release notes / changelog. Commit only when the diff aligns
# with intentional Interlint behavior changes (e.g., new rule-IDs from a phase landing).
#
# DO NOT auto-run from CI — this would defeat the regression-check purpose of the suite.

set -euo pipefail

# Active packages — zod and prisma are deferred pending backlog 999.1
# (parse_type_string OOM mitigation). Pass them explicitly as $1 if you
# want to regenerate them anyway (e.g., after the OOM fix lands).
PKGS=("react" "tanstack-query" "trpc")
if [[ $# -ge 1 ]]; then
    PKGS=("$1")
fi

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

# Build the dump binary once (defined as a cargo example).
cargo build --release --example reference_suite_dump

for PKG in "${PKGS[@]}"; do
    OUT="tests/fixtures/reference-suite/$PKG/expected-findings.json"
    echo "[regen] Generating $OUT ..."
    ./target/release/examples/reference_suite_dump "$PKG" > "$OUT.tmp"
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
echo "  git diff tests/fixtures/reference-suite/*/expected-findings.json"
echo "Then commit if the changes align with intentional behavior shifts."
