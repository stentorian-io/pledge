.PHONY: setup check check-all check-lsp coverage test-tools language-coverage language-coverage-update reference-suite reference-suite-regen

setup:
	bun scripts/hooks/install.ts

# The default test suite already includes the pinned reference packages.
check:
	bun scripts/check.ts

check-all:
	bun scripts/check.ts all

check-lsp:
	bun scripts/check.ts lsp

coverage:
	bun scripts/check.ts coverage

test-tools:
	bun scripts/check.ts tools

language-coverage:
	bun scripts/conformance/check.ts

language-coverage-update:
	bun scripts/conformance/check.ts --write

reference-suite:
	cargo test --locked --release --test node reference::

# Regenerated findings need manual review; see docs/reference-suite.md.
reference-suite-regen:
	./scripts/regen-reference-suite.sh $(if $(PKG),"$(PKG)")
