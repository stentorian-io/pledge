.PHONY: setup reference-suite reference-suite-regen

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
