.PHONY: setup

setup:
	git config core.hooksPath scripts/git-hooks
	@echo "Done. Git hooks activated."
