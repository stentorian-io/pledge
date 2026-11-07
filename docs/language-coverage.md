# JavaScript and TypeScript coverage

Use this page to distinguish a tested public contract from an unsupported or
unverified language behavior. Pledge compares package interfaces; it does not
implement a JavaScript engine or prove that arbitrary program behavior remains
equivalent. A clean comparison means no breaking change was found within its
supported checks. See [reading results](results.md#incomplete-analysis-and-limitations).

## What the inventory establishes

The checked-in baseline indexes every section and named grammar definition in
the pinned ECMAScript source, every Test262 feature flag, and every TypeScript
conformance and compiler-regression fixture, SyntaxKind member, and compiler option in the selected
source regions. Imported ECMAScript tables are pinned alongside the specification.
Each indexed unit has exactly one assessment; CI rejects omissions, duplicates,
missing evidence, invalid documentation links, and a stale report.
Assessments name their reviewed source revisions; changing a baseline requires
explicit reassessment even when its section or test identifiers stay unchanged.

This is exhaustive accounting of those extracted units, not exhaustive semantic
conformance. A section can contain many algorithm steps and interactions; those
are not separate coverage units. TypeScript's standard-library declaration
members, language-service tests, and every possible
compiler-option combination are not exhaustively indexed. The baseline and
these exclusions must be reviewed when expanding the claim.
Node coverage indexes the selected ESM/package documentation; npm coverage
indexes its package-manifest documentation. Other host APIs are outside these
inventories. Test262's complete execution corpus is not imported or run.

The ECMAScript baseline is an edition snapshot. Test262 additionally includes
proposals and harness features; their presence does not make them standardized.
The TypeScript baseline is a compiler release, not a complete normative language
specification. Its [Handbook](https://www.typescriptlang.org/docs/handbook/intro)
explicitly distinguishes itself from one. Node/npm resolution, JSX, JSDoc,
browser APIs, and bundler conventions must retain their own scope; they are not
implicitly certified by an ECMAScript result.

## Reading a coverage status

| Status | Meaning |
| --- | --- |
| Verified | Evidence covers the explicitly stated contract, including breaking and compatible controls, in its listed profiles |
| Partial | Some relevant behavior has evidence; the stated limits and remaining upstream behavior are not certified |
| Unsupported | A reproduced unsupported analysis path has regression coverage for its incomplete result |
| Unverified | No sufficient evidence mapping has been established; this remains an open coverage gap |
| Excluded | The behavior is outside the stated scope, with a reason; it is not counted as supported |

An upstream test linked to an adaptation does not mean its full upstream suite
was executed. The adapted consumer must independently compile or run against the
old and new packages, and Pledge must agree on the claimed interface behavior.
Existing regression tests are reused when they already exercise the contract.
An incomplete-result assertion establishes the handling of missing evidence,
not support for the unresolved semantics. Rust line coverage is a separate metric.

## Known analysis limits

- [Computed properties](rules/computed-properties.md) documents unresolved keys,
  shadowed built-ins, factory results, post-await initialization, and symbol
  inventories introduced by arbitrary spreads or accessors.
- [Reachable types](rules/reachable-types.md) documents dependent generic
  inference, opaque types, nominal alignment, and time/depth/work budgets.
- [Module loading](rules/module-loading.md) describes the installed-Node scope,
  bounded condition graphs, unresolved re-exports, and deferred runtime work.
- [Export resolution](rules/export-resolution.md) distinguishes source discovery
  from runtime resolution and documents syntax and graph limits.
- [Distributed assets](rules/distributed-assets.md) covers CSS grammar and asset
  limits; browser-specific CSS behavior is a separate concern.

The compiler deliberately retains strict nullability, function variance, and
exact optional-property evidence even when author settings are looser. A consumer
that accepts a change under weaker settings does not invalidate a finding for
the stricter public contract. Execution profiles record these assumptions.
Passing the listed profiles does not certify unlisted versions or settings.

## Reproduce and maintain coverage

Run `make language-coverage` to validate the offline inventory and generated
region. Run `make language-coverage-update` after reviewing assessment changes.
`make test-tools` tests the validator in isolated temporary repositories.
The normal checks never fetch replacement upstream fixtures.

`bun scripts/conformance/run.ts --profile <id> --output <path>` lists registered
Rust tests before executing the profile's exact tests. It checks the actual Node
and TypeScript versions, fails on missing tests, and writes results for every
selected test. CI publishes these results separately from this coverage inventory.
An unavailable toolchain is a failed run, not a silently skipped passing cell.

`bun scripts/conformance/upstream.ts verify` re-downloads the pinned official
metadata and selected upstream cases, checks their hashes, and reproduces the
indexes. This explicit network audit is separate from the offline tests.
CI runs it in the pinned-reference job; it fetches only the recorded revisions,
not newer tests or replacement package fixtures. Upstream source licenses are
linked from the source locks, and adapted case attribution lives with its evidence.
To review a new revision, use `bun scripts/conformance/upstream.ts candidate
<source-id> <commit-sha> <output-directory>`. It writes a candidate index and
lists added, removed, and changed units without changing the baseline or its
assessments. Updating the baseline requires explicit classification of additions;
new references must not be silently assigned to a passing category. Review
changed source hashes even when no indexed identifiers changed, then update the
affected assessments' baseline revisions and re-run their consumer evidence.

## Coverage inventory

<!-- language-coverage:start -->

<!-- Generated by bun scripts/conformance/check.ts --write. -->

Every indexed reference has an explicit disposition. Counts are inventory units,
not equally weighted language features or a percentage of language correctness.

| Pinned source | Indexed units | Verified | Partial | Unsupported | Unverified | Excluded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| [ECMAScript 2026](https://github.com/tc39/ecma262/tree/0248456c758431e4bb8e5d26333ff1865123c9cd) | 2673 | 0 | 10 | 0 | 2658 | 5 |
| [Test262 feature taxonomy](https://github.com/tc39/test262/tree/e4fbf57745c6aae3d1f12f181ecef78b1665d812) | 199 | 0 | 6 | 0 | 191 | 2 |
| [TypeScript 6.0.3](https://github.com/microsoft/TypeScript/tree/050880ce59e30b356b686bd3144efe24f875ebc8) | 12969 | 0 | 30 | 0 | 12939 | 0 |
| [Node 26.7.0 module documentation](https://github.com/nodejs/node/tree/b4f23d3619c98bed09af93a21192f6080197a8c6) | 84 | 0 | 5 | 0 | 79 | 0 |
| [npm 11.19.0 manifest documentation](https://github.com/npm/cli/tree/bfacd33ccbcd908480610703b60455d2da5b57a9) | 46 | 0 | 0 | 0 | 46 | 0 |

### Assessed contracts

A partial row establishes only the behavior described below. It does not certify
the entire upstream clause, feature flag, syntax kind, option, or test case.

| Contract | Layer | Status | References | Evidence | Limits |
| --- | --- | --- | ---: | --- | --- |
| Mapped output write capabilities | types | partial | 3 | [mapped-consumers](../tests/languages/typescript/compiler/conformance.rs) | Adapted old/new consumer pairs cover the named operation and a compatible control, not every assertion in the upstream file. |
| Tuple rest call positions | types | partial | 3 | [rest-consumers](../tests/languages/typescript/compiler/conformance.rs) | Adapted old/new consumer pairs cover the named operation and a compatible control, not every assertion in the upstream file. |
| Concrete template union inputs | types | partial | 3 | [template-consumers](../tests/languages/typescript/compiler/conformance.rs) | Adapted old/new consumer pairs cover the named operation and a compatible control, not every assertion in the upstream file. |
| Export names, aliases, and Unicode spelling | exports | partial | 5 | [namespace-names](../tests/languages/javascript/runtime/consumer_contracts.rs) | Named removal and spelling controls are tested, including Unicode namespace names. This does not cover all export grammar, early errors, or module-linking semantics. |
| CommonJS literal property descriptors | runtime | partial | 2 | [namespace-names](../tests/languages/javascript/runtime/consumer_contracts.rs) | Literal setter, writability, and enumerability changes have consumers; dynamic descriptors, repeated definitions, and later mutations are not established. |
| Registered symbols and string-key separation | runtime | partial | 4 | [symbol-values](../tests/languages/javascript/runtime/contracts.rs), [factory-symbols](../tests/languages/javascript/runtime/consumer_contracts.rs), [unknown-symbols](../tests/languages/typescript/compiler/enrichment.rs) | Tests establish selected registry identities and symbol/string collisions. Dynamic keys, shadowed built-ins, and arbitrary symbol-index inventories retain incomplete evidence. |
| Compiler evidence for CommonJS factory assignments | types | partial | 2 | [factory-symbols](../tests/languages/javascript/runtime/consumer_contracts.rs), [factory-evidence](../tests/languages/typescript/compiler/enrichment.rs) | Concrete factory results are compared without executing package code; any/unknown inferred results retain missing evidence. This does not prove arbitrary call or constructor behavior. |
| CommonJS writes across async suspension | runtime | partial | 3 | [async-timing](../tests/languages/javascript/runtime/consumer_contracts.rs) | Immediate require consumers establish before/after suspension behavior. Later reads, microtask schedules, and arbitrary async initialization remain incomplete. |
| Opaque proxy dispatch evidence | runtime | partial | 2 | [proxy-limits](../tests/languages/javascript/runtime/contracts.rs) | Opaque dispatch is tested as incomplete; arbitrary trap effects and private-brand behavior are not proven. |
| Property spread versus whole-module forwarding | exports | partial | 3 | [spread-members](../tests/languages/javascript/modules/forwarding.rs) | Selected spreads preserve copied members and write capabilities. Arbitrary property enumeration, accessors with runtime effects, and full CopyDataProperties semantics are not certified. |
| Overload selection and dependent generics | types | partial | 2 | [overload-contracts](../tests/languages/typescript/compiler/contracts.rs) | Consumers cover selected input overlaps, result changes, and independent generic constraints. Dependent generic inference is explicitly incomplete. |
| Nominal relationships across exports | types | partial | 3 | [nominal-contracts](../tests/languages/typescript/compiler/contracts.rs) | Selected private/protected/unique-symbol input and output relations are tested; ambiguous alignments and budgets remain limitations. |
| Callable receivers and predicates | types | partial | 2 | [callable-contracts](../tests/languages/typescript/compiler/contracts.rs) | Selected receiver, predicate, and assertion contracts have consumer evidence; arbitrary control-flow and generic combinations remain unverified. |
| Generic public contracts | types | partial | 2 | [generic-contracts](../tests/languages/typescript/compiler/contracts.rs) | Selected mapped, conditional, binder, and variance contracts are tested. This is not complete type inference or assignability conformance. |
| Exact optional properties | types | partial | 2 | [optional-contracts](../tests/languages/typescript/compiler/contracts.rs) | Strict consumers distinguish absent properties from explicit undefined; weaker option combinations are not covered by this evidence. |
| Syntax, library, and inferred-predicate version boundaries | configuration | partial | 3 | [compiler-syntax-versions](../tests/languages/typescript/compiler/version_support.rs), [compiler-predicate-versions](../tests/languages/typescript/compiler/version_support.rs) | Selected version-sensitive cases run on all declared compilers; the compiler and runtime support of every other feature is unverified. |
| TypeScript path alias selection | configuration | partial | 2 | [alias-consumers](../tests/languages/typescript/modules/path_aliases.rs) | Exact and wildcard alias consumers cover tested precedence and substitution cases; this is not all compiler module-resolution behavior. |
| Installed Node synthetic CommonJS named imports | runtime | partial | 2 | [cjs-names](../tests/ecosystems/node/modules/commonjs_names.rs), [cjs-conditions](../tests/ecosystems/node/modules/commonjs_names.rs), [cjs-contexts](../tests/ecosystems/node/modules/commonjs_names.rs), [cjs-classification](../tests/ecosystems/node/modules/commonjs_names.rs), [cjs-combined](../tests/ecosystems/node/modules/commonjs_names.rs), [cjs-limits](../tests/ecosystems/node/modules/commonjs_names.rs) | Real installed-loader consumers cover selected rewrites, condition combinations, and explicit evidence limits. Results do not certify other Node releases, parsers, or custom loaders. |
| Node condition contexts and local re-exports | configuration | partial | 3 | [cjs-conditions](../tests/ecosystems/node/modules/commonjs_names.rs), [cjs-contexts](../tests/ecosystems/node/modules/commonjs_names.rs), [cjs-combined](../tests/ecosystems/node/modules/commonjs_names.rs) | Bounded combinations are tested. Exhaustive arbitrary condition graphs and every host-defined condition are not covered. |

### Unverified and excluded coverage

Unverified is an open coverage gap, including cases whose existing tests have
not yet been traced to this baseline. Excluded is an explicit scope decision;
neither is a passing conformance result.

| Area | Status | References | Reason |
| --- | --- | ---: | --- |
| Specification organization and test-harness capabilities | excluded | 7 | Reference structure and engine harness capabilities are not package interface comparison contracts. Their exclusion does not exclude associated language semantics. |
| ecma262 / clause | unverified | 2250 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| ecma262 / grammar | unverified | 405 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| ecma262 / imported-table | unverified | 3 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| node / host-documentation | unverified | 79 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| npm / host-documentation | unverified | 46 | The package-manifest documentation is indexed separately from language semantics. Existing manifest regression tests are not yet mapped to independent npm consumer evidence in this report. |
| test262 / feature | unverified | 174 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| test262 / proposal | unverified | 17 | Proposal flags in this pinned Test262 revision are not treated as standardized ECMAScript features. Pledge coverage is unverified unless separately mapped. |
| typescript / compiler-case | unverified | 6537 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / compiler-option | unverified | 124 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/Symbols | unverified | 8 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/additionalChecks | unverified | 1 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/ambient | unverified | 22 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/async | unverified | 185 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/asyncGenerators | unverified | 3 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/classes | unverified | 466 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/constEnums | unverified | 9 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/controlFlow | unverified | 57 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/declarationEmit | unverified | 23 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/decorators | unverified | 88 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/directives | unverified | 6 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/dynamicImport | unverified | 71 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/emitter | unverified | 13 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/enums | unverified | 14 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es2016 | unverified | 1 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es2017 | unverified | 12 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es2018 | unverified | 4 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es2019 | unverified | 13 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es2020 | unverified | 15 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es2021 | unverified | 12 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es2022 | unverified | 7 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es2023 | unverified | 2 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es2024 | unverified | 3 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es2025 | unverified | 4 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es5 | unverified | 1 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es6 | unverified | 1045 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/es7 | unverified | 45 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/esDecorators | unverified | 110 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/esnext | unverified | 2 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/expressions | unverified | 376 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/externalModules | unverified | 227 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/functions | unverified | 18 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/generators | unverified | 15 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/importAssertion | unverified | 5 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/importAttributes | unverified | 11 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/importDefer | unverified | 17 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/interfaces | unverified | 66 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/internalModules | unverified | 76 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/jsdoc | unverified | 341 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/jsx | unverified | 211 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/moduleResolution | unverified | 51 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/node | unverified | 94 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/nonjsExtensions | unverified | 5 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/override | unverified | 31 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/parser | unverified | 820 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/pedantic | unverified | 2 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/references | unverified | 15 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/root | unverified | 2 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/salsa | unverified | 191 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/scanner | unverified | 35 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/statements | unverified | 203 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/types | unverified | 842 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / conformance/typings | unverified | 9 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |
| typescript / syntax-kind | unverified | 373 | No sufficient Pledge evidence mapping has been established for these exact upstream units. Existing tests may exercise subsets; they are not credited without reviewed traceability. |

### Declared execution profiles

These are the exact environments selected by the coverage runner. A profile
does not imply support for every language feature or every option combination.

| Profile | Node versions | TypeScript versions | Options | Limits |
| --- | --- | --- | --- | --- |
| compiler-nodenext | 22.12.0, 24.0.0 | 4.9.5, 5.0.4, 5.2.2, 5.4.5, 5.7.3, 5.9.3, 6.0.3 | {"target":"ES2022","module":"NodeNext","moduleResolution":"NodeNext","strict":true,"exactOptionalPropertyTypes":"true for analyzer; per fixture for consumer"} | Only mapped evidence tests run in this profile. Library declarations are those bundled with the selected compiler. Other targets, libs, JSX transforms, loose consumer settings, bundler resolution, and combinations remain unverified. |
| compiler-node16 | 22.12.0, 24.0.0 | 4.9.5, 5.0.4, 5.2.2, 5.4.5, 5.7.3, 5.9.3, 6.0.3 | {"target":"ES2022","module":"Node16","moduleResolution":"Node16","strict":true,"exactOptionalPropertyTypes":true} | Only mapped evidence tests run in this profile. Library declarations are those bundled with the selected compiler. Other targets, libs, JSX transforms, loose consumer settings, bundler resolution, and combinations remain unverified. |
| compiler-commonjs | 22.12.0, 24.0.0 | 4.9.5, 5.0.4, 5.2.2, 5.4.5, 5.7.3, 5.9.3, 6.0.3 | {"target":"ES2022","module":"CommonJS","moduleResolution":"Node","strict":true,"exactOptionalPropertyTypes":true,"ignoreDeprecations":"6.0 on TypeScript 6 only"} | Legacy Node resolution is deprecated in TypeScript 6; this profile explicitly acknowledges that deprecation, and makes no TypeScript 7 claim. Only mapped evidence tests run in this profile. Library declarations are those bundled with the selected compiler. Other targets, libs, JSX transforms, loose consumer settings, bundler resolution, and combinations remain unverified. |
| node-runtime | 22.12.0, 24.0.0, 26.7.0 | none required | {} | Packed local ESM and CommonJS examples only; this does not cover every version in engines.node, other runtimes, custom loaders, or arbitrary package execution. |

The [machine-readable inventory](coverage/inventory.json) includes exact test names,
fixture cases, upstream provenance, and the complete reference assignments.
[Source locks](coverage/sources.json) record immutable revisions and checksums.

<!-- language-coverage:end -->
