# Reference Suite — Curation Procedure

The reference suite (Phase 25 INT-01) runs Interlint against pinned versions of published packages and asserts that the (exit_code, sorted_rule_ids) output matches a curated `expected-findings.json` file per package. v0.3 ships with **3 active packages**; `zod` and `@prisma/client` are scaffolded but their tests are deferred pending backlog 999.1 (parse_type_string OOM mitigation). See `tests/reference_suite.rs` for the disabled-test comment and `src/types/parser.rs::parse_type_string` for the in-source bisect data.

## Packages

| Package | from version | to version | Status | Coverage stress-test |
|---------|--------------|------------|--------|----------------------|
| react | 19.2.4 | 19.2.5 | ✅ active | Multiple subpath exports, CJS+ESM dual-publish, `.d.ts` |
| @tanstack/query-core | 5.100.4 | 5.100.5 | ✅ active | Class surface, `imports` field (EXP-14) |
| @trpc/server | 11.15.2 | 11.16.0 | ✅ active | `infer T extends C` (TYPE-02), multi-condition exports map |
| zod | 4.3.5 | 4.3.6 | ⏸ deferred (999.1) | Heavy generics, discriminated unions (BEH-06), `satisfies` (TYPE-01) |
| @prisma/client | 7.7.0 | 7.8.0 | ⏸ deferred (999.1) | Subpath exports, conditional exports, `.d.ts` ambient declarations |

Latest versions verified 2026-04-25; refresh when authoring future updates.

Note: @prisma/client `from` version is 7.7.0 (not 7.7.1 as in the original pin table — 7.7.1 does not exist on npm; 7.7.0 is the nearest stable minor). @trpc/server uses 11.15.2 → 11.16.0 (a clean patch-bump-then-minor).

### Why zod and @prisma/client are deferred

Both packages ship `.d.ts` files with many medium-complex generated type aliases (zod's `types.ts` is 160 KB; prisma's `client.d.ts` declares 120+ aliases including 1–1.5 KB recursive conditional types). Running the suite against them currently triggers an OOM in `parse_type_string` — bisected to ~7 GB resident across the per-symbol loop in `compare::compare_structural_diff`. Their fixture tarballs and scaffolded `expected-findings.json` files remain committed under `tests/fixtures/reference-suite/{zod,prisma}/` so re-enabling is a one-line change to `tests/reference_suite.rs` once the parse-allocator fix lands. Full bisect details: see backlog 999.1 in ROADMAP.md and the doc comment on `parse_type_string` in `src/types/parser.rs`.

## Pre-publish Gate

The canonical pre-publish gate for this project is:

```sh
make check
```

`make check` runs four cargo commands in sequence — `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --release`, and `cargo test --release --test reference_suite` — and exits non-zero on the first failure. The fourth command is the milestone-close reference suite gate; running it as part of `make check` ensures the suite always passes before publish.

**Why no CI?** A GitHub Actions workflow is permanently out of scope for the v0.3 milestone — `make check` is the gating story. This is a deliberate project constraint, not a deferred TODO; the rationale is recorded in `.planning/phases/25-reference-suite-documented-limitations/25-VERIFICATION.md` §Deviations §2. If you fork this project and want CI, you can mirror the four `make check` commands in any standard CI runner; nothing in the codebase blocks it.

## Why pinned + degraded

- **Pinned tarballs** (`from.tgz` / `to.tgz` committed to git): npm registry promises immutability, but reference fixtures live in this repo for reproducibility and offline-capable test runs.
- **LSP-degraded forced**: The test harness passes `lsp_was_available=false` to `compare_exports`. CI runners may not have `typescript-language-server` installed; forcing degraded mode produces deterministic findings on every machine. Limitation findings tagged `limitation: lsp-required` are EXPECTED in the curated output.

## Adding or updating a package's expected-findings

When an intentional Interlint behavior change lands (e.g., a new phase ships a new rule-ID), the curated expected-findings file must be updated:

1. Run `./scripts/regen-reference-suite.sh <pkg>` to regenerate the baseline for one package, or `./scripts/regen-reference-suite.sh` for all 5.
2. Run `git diff tests/fixtures/reference-suite/<pkg>/expected-findings.json` and inspect the rule-ID changes.
3. **Manually review** each added/removed rule-ID against:
   - The package's release notes / GitHub Releases / CHANGELOG.md for the from→to range
   - The intentional Interlint behavior change's PLAN.md and SUMMARY.md
4. If the diff aligns with intent, commit. If unexpected rule-IDs appear, file a regression bug — DO NOT mask the regression by accepting the new baseline blindly (RESEARCH Anti-Patterns).

## Adding a NEW reference package

This is rare (not done in v0.3 — milestone-close scope). If needed in v0.4+:

1. Add a row to the table above with version pin rationale.
2. `npm pack <pkg>@<from-version>` and `npm pack <pkg>@<to-version>`; place under `tests/fixtures/reference-suite/<key>/{from,to}.tgz`.
3. Add `#[test] fn reference_<key>() { run_reference_case("<key>"); }` to `tests/reference_suite.rs`.
4. Run `./scripts/regen-reference-suite.sh <key>` to generate the baseline.
5. Manually review and commit.

## Anti-patterns

- **DO NOT** auto-regenerate expected-findings in CI. The curated file is the contract; auto-regen defeats regression detection.
- **DO NOT** soften the rule-ID assertion to "expected ⊆ actual". The shape contract is exact match.
- **DO NOT** include free-form message text in expected-findings. Wording is internal-detail churn.
- **DO NOT** fetch tarballs from npm registry at test time. Pinned-and-committed only.

## Bumping pinned versions

When the pinned `to` version becomes >12 months old (cosmetic — the reference suite still works on old fixtures), bump to a fresher pair:

1. Pick a `from`/`to` patch-bump pair from the package's recent releases (RESEARCH §Pitfall 3: small diff is curation-friendly).
2. Update Step 1-5 of "Adding a NEW reference package" above.
3. Update the `from version` / `to version` columns in the table.
4. The previous version pair can be archived in git history; no need to keep stale tarballs.
