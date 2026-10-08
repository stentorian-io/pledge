# Reference suite

The reference suite compares pinned versions of published packages and checks
an exact `(exit_code, sorted_rule_ids)` result against each package's curated
`expected-findings.json`. It currently runs three packages. Zod and Prisma
fixtures are retained, but their cases remain disabled because type parsing
can exhaust memory on their generated declarations.

## Pinned packages

| Package | From | To | Status | Coverage |
| --- | --- | --- | --- | --- |
| react | 19.2.4 | 19.2.5 | Active | Subpaths, CommonJS/ESM, declaration files |
| @tanstack/query-core | 5.100.4 | 5.100.5 | Active | Class surface and imports mappings |
| @trpc/server | 11.15.2 | 11.16.0 | Active | Constrained infer types and conditional exports |
| zod | 4.3.5 | 4.3.6 | Deferred | Generics, discriminated unions, and satisfies |
| @prisma/client | 7.7.0 | 7.8.0 | Deferred | Subpaths and generated ambient declarations |

These versions describe the committed fixtures, not the latest registry
releases. See [the harness](../tests/reference_suite.rs) for active cases.

## Running the suite

```sh
make reference-suite
```

The suite is also included in `make check` through the default release tests.
Checks run locally. See [contributing](../CONTRIBUTING.md) for the full setup
and verification commands.

Tarballs are committed as `from.tgz` and `to.tgz` so tests do not fetch mutable
inputs from the registry. The harness passes `lsp_was_available=false` to the
comparison engine, giving deterministic results without a language server.
Informational `limitation: lsp-required` findings can therefore be expected.

## Updating expected findings

After an intentional detection change:

1. Run `make reference-suite-regen PKG=react`, substituting the fixture key.
   Omit `PKG` to regenerate the three active packages.
2. Inspect the changed `expected-findings.json` files with `git diff`.
3. Compare every added or removed rule ID with the package's release notes and
   the intended analyzer change. Add a focused regression test when needed.
4. Run `make reference-suite`, then commit the reviewed baseline with the
   implementation change.

Regeneration is a manual maintenance step. Do not run it from a verification
hook or accept a new baseline solely because a check failed. Keep the assertion
an exact match, and do not include free-form finding messages in baselines.

## Adding or refreshing a package

Choose a small version change whose expected compatibility impact can be
reviewed. Pack each published version with npm and store the tarballs under
`tests/fixtures/reference-suite/<key>/{from,to}.tgz`.

Add the fixture key to the package list in `scripts/regen-reference-suite.sh`
and a corresponding `run_reference_case("<key>")` test in the harness. Generate
and review its baseline, then update the table above. Existing versions remain
available in Git history; additional archived tarballs are unnecessary.

## Deferred packages

Zod and Prisma contain many complex generated aliases. Previous investigation
found excessive allocation in `parse_type_string` during structural comparison;
see [the parser's diagnostic notes](../src/types/parser.rs) and the disabled
cases in [the harness](../tests/reference_suite.rs).

Keep their fixtures for work on the allocation issue. Enable their cases after
verifying bounded memory use and reviewing the expected findings. The
regeneration script accepts `zod` or `prisma` explicitly for that work, but the
default regeneration command skips both.
