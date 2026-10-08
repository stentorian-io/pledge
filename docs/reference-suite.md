# Reference suite

The reference suite compares pinned published packages. Each reviewed snapshot
records every finding's severity, rule, file, position, and message, plus export
counts for each public subpath, condition, and symbol space. It also retains
discovery warnings and whether they indicate incomplete analysis. Losing one
finding is observable even when another finding has the same rule.

## Pinned packages

| Package | From | To | Exercises |
| --- | --- | --- | --- |
| react | 19.2.4 | 19.2.5 | Conditional CommonJS wrappers and discovery limitations |
| @tanstack/query-core | 5.100.4 | 5.100.5 | Class surfaces, imports mappings, and deprecation |
| @trpc/server | 11.15.2 | 11.16.0 | Generated types and conditional exports |
| zod | 4.3.5 | 4.3.6 | Large barrel graphs, generics, and namespaces |
| @prisma/client | 7.7.0 | 7.8.0 | Generated declarations, subpaths, and source limits |

These are the committed fixture versions, not the latest registry releases.
All five cases run locally in the default suite and `make reference-suite`.
Tarballs are never fetched during reference verification.

## Analysis boundaries

References deliberately omit LSP for reproducibility. Their analysis mode is
`structural-only` and exit code is **3**, matching incomplete CLI analysis.
The separate `breaking` field records whether any Major findings were produced.
A snapshot with no findings does not establish compatibility.

The React fixture currently yields no discovered symbols from its conditional
CommonJS wrappers. Generated type expansion and minified/oversized sources also
produce explicit incomplete evidence in these packages. Snapshots preserve
those gaps; they are regression checks, not ecosystem-completeness claims.

The opt-in `consumer_compatibility` suite independently compiles TypeScript and
executes Node consumers against old/new package pairs. Other LSP suites test
real enrichment, managed toolchain installation, failed requests, and crashes.

## Updating expected findings

After an intentional detection change:

1. Run `make reference-suite-regen PKG=react`, substituting the fixture key.
   Omit `PKG` to generate candidates for all five packages.
2. Review each changed finding, severity, public-surface count, and warning
   against the package sources and intended analyzer behavior. The source
   positions and messages identify the affected declaration and public context.
3. Add `_curation_notes` explaining the reviewed result and its limitations.
   Add a focused regression when a snapshot reveals an analyzer defect.
4. Run `make reference-suite`, then commit the reviewed baseline with the change.

Regeneration is a manual maintenance step. Verification never updates snapshots.
Do not accept a candidate solely because a test failed. The generator shares
its pipeline implementation with the test harness and respects Cargo's target
directory configuration.

## Adding or refreshing a package

Choose a small version change whose compatibility impact can be reviewed. Store
published tarballs under `tests/fixtures/reference-suite/<key>/{from,to}.tgz`,
add the key to the regeneration script and reference harness, then review its
snapshot and update the table above.
