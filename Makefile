.PHONY: setup reference-suite reference-suite-regen check

setup:
	git config core.hooksPath scripts/git-hooks
	@echo "Done. Git hooks activated."

# Run the v0.3 reference validation suite (plan 25-03 INT-01).
# Forces release mode + LSP-degraded behaviour for CI parity.
reference-suite:
	cargo test --release --test reference_suite

# Regenerate expected-findings.json baselines for one or all reference
# packages. Pass PKG=<name> to target a single package, otherwise all
# active packages are regenerated. NEVER run from CI — the regenerated
# baselines must be reviewed against package release notes before commit
# (see docs/REFERENCE_SUITE.md).
reference-suite-regen:
	@if [ -n "$(PKG)" ]; then \
		./scripts/regen-reference-suite.sh $(PKG); \
	else \
		./scripts/regen-reference-suite.sh; \
	fi

# Pre-publish quality gate — Phase 26 INT-01 SC5 closure (local gating story).
# Runs the four cargo commands in sequence: fmt-check, clippy with -D warnings
# across all targets, the full release-mode test suite, and the reference-suite
# integration test. Any non-zero exit halts the chain because make stops on the
# first failing recipe line by default. This is the canonical pre-`cargo
# publish` gate; CI is permanently out of scope for the v0.3 milestone (see
# .planning/phases/25-reference-suite-documented-limitations/25-VERIFICATION.md
# §Deviations §2 for the rationale).
check:
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings
	cargo test --release
	cargo test --release --test reference_suite
