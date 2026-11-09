#!/usr/bin/env bash
# Raise the baseline from the last successful full-suite coverage measurement.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
exec bun scripts/coverage/update-baseline.ts
