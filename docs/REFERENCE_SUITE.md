# Reference Suite — Curation Procedure

The reference suite (Phase 25 INT-01) is the v0.3 milestone-close gate. It runs Interlint against pinned versions of 5 published packages and asserts that the (exit_code, sorted_rule_ids) output matches a curated `expected-findings.json` file per package. CI gates on this suite passing for every PR to `main`.

## Packages

| Package | from version | to version | Coverage stress-test |
|---------|--------------|------------|----------------------|
| react | 19.2.4 | 19.2.5 | Multiple subpath exports, CJS+ESM dual-publish, `.d.ts` |
| zod | 4.3.5 | 4.3.6 | Heavy generics, discriminated unions (BEH-06), `satisfies` (TYPE-01) |
| @prisma/client | 7.7.0 | 7.8.0 | Subpath exports, conditional exports, `.d.ts` ambient declarations |
| @tanstack/query-core | 5.100.4 | 5.100.5 | Class surface, `imports` field (EXP-14) |
| @trpc/server | 11.15.0 | 11.16.0 | `infer T extends C` (TYPE-02), multi-condition exports map |

Latest versions verified 2026-04-25; refresh when authoring future updates.

Note: @prisma/client `from` version is 7.7.0 (not 7.7.1 as in the original pin table — 7.7.1 does not exist on npm; 7.7.0 is the nearest stable minor).

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
