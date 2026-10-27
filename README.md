# Pledge (`interlint`)

Catch breaking changes in JavaScript and TypeScript packages before publishing.
Pledge compares the package you would publish with a version from your npm
registry and reports changes to exports, types, package metadata, and distributed
assets. The Cargo package and executable are currently named `interlint`.

## Install from source

You need Rust and a native build toolchain (a C/C++ compiler and CMake) to install,
and Node.js with npm to analyze packages. Use Node.js 22.12 or newer. Bun is only
needed for this repository's development scripts.

```sh
git clone https://github.com/stentorian-io/pledge.git
cd pledge
cargo install --locked --path .
interlint --version
```

Cargo installs the executable into `~/.cargo/bin`; keep that directory on `PATH`.
The repository is public and can be cloned without GitHub credentials. There is
no published npm wrapper or prebuilt binary distribution yet.

Type analysis uses the JavaScript TypeScript compiler from your project or
`PATH`. If neither is available, interlint downloads and caches a pinned
TypeScript 5.9.3 toolchain. For CI, install TypeScript in advance through your
project's dev dependencies or `npm install --global typescript@5.9.3`. A language
server is not required for normal analysis.

## Usage

Run this from the package you want to check:

```sh
npm ci
npm run build                 # when the package needs a build
interlint
```

Interlint packs the current package, compares it with the registry's `latest`
version, and exits nonzero for breaking changes, tool failures, or incomplete
analysis. It analyzes the files included by `npm pack`, so build JavaScript and
declarations first. Packing and dependency preparation disable lifecycle scripts
and use temporary directories; they do not overwrite your tarballs or lockfiles.

```sh
interlint ./packages/core              # one package in a workspace
interlint --against 1.4.2              # pin the published baseline
interlint --against next               # compare with a release channel
interlint --against latest --format json > ../interlint-report.json
interlint --color never
interlint --help
```

Use `--against latest` in CI to require an existing baseline. Without `--against`,
a registry 404 is treated as a first publish and the report lists new exports.
Some registries also return 404 for inaccessible packages, so explicit baseline
selection prevents this case from silently passing your CI check.

Registry lookup, downloads, and dependency installation use npm configuration,
including scoped registries and credentials. Configure authentication before
running interlint. Each invocation checks one package; pass workspace package
paths individually. Yarn, pnpm, and Bun projects can be analyzed after building,
but acquisition and dependency preparation use npm. Packed dependencies must be
resolvable by npm; unpublished workspace/file references are not substituted.

See [CI integration](docs/ci.md) for reproducible installation, JSON output,
exit codes, authentication, and troubleshooting.

## Rules

Interlint emits findings tagged with stable rule-IDs. Severity maps to CLI exit code:

| Severity | Exit code | Meaning |
|----------|-----------|---------|
| Major | 1 | Breaking change — consumers will observe broken behavior |
| Warning | 0 | Observable behavior change — exhaustive-switch breaks, deprecation. Reported but does not break the build. |
| Incomplete analysis | 3 | Analysis could not complete cleanly (compiler unavailable, unresolved types, parse failure) |
| Minor | 0 | Non-breaking change worth noting |
| Patch | 0 | Cosmetic / non-observable change |
| Info | 0 | Documented limitation — analysis acknowledged but couldn't verify |

Exit code 2 indicates a tool or invocation error (for example, invalid arguments,
invalid package metadata, I/O failure, or an unavailable registry). It is not a
finding severity. Incomplete analysis is a run status, not a finding severity.

Incomplete analysis takes precedence over breaking findings and exits 3. This
includes failed or empty type requests, exhausted analysis budgets, unreadable
public sources, and files skipped by source processing limits. Findings from
completed analysis are still reported; an incomplete result does not establish
compatibility.

### Directives and decorators

| Rule ID | Severity | Trigger |
|---------|----------|---------|
| `directive-added` | Major | A `/// <reference ... />`, `/// <amd-module ... />`, or `/// <amd-dependency ... />` directive is added at the top of a source file |
| `directive-removed` | Major | A triple-slash directive present in the old version is dropped in the new version |
| `directive-changed` | Major | A directive's attribute value changes (including the TS 5.5+ `preserve="true"` toggle) |
| `directive-reordered` | Minor | The same set of directives appears in both versions in a different document order |
| `decorator-added` | Major | A stage-3 decorator (class, method, field, or parameter) is added to an exported target |
| `decorator-removed` | Major | A stage-3 decorator is removed from an exported target |
| `decorator-arguments-changed` | Major | A decorator factory's call arguments change (e.g., `@Route('/users')` → `@Route('/people')`) |

TSDoc tags (`@deprecated`, `@internal`, `@alpha`, `@beta`, and `@public`) are detected across default exports, named and star re-exports, destructured pairs, compound-assignment members, and bundler CommonJS output. Deprecation produces an `export-deprecated` warning; internal, alpha, and beta exports are excluded from public API comparison.

See `docs/rules/directive.md` for INF-05 (triple-slash directives) details and the TypeScript compiler position rule.

### TypeScript type contracts

| Rule ID | Severity | Trigger |
|---------|----------|---------|
| `type-narrowing-widened` | Minor | A `satisfies T` clause is removed and the public value type widens, or type evidence is unavailable |
| `infer-extends-constraint-changed` | Major | The `C` in `T extends infer U extends C ? ... : ...` on an exported conditional type is changed |
| `generic-inference-constraint-added` | Major | A `NoInfer<T>` wrapper blocks previously accepted argument inference |
| `generic-inference-constraint-removed` | Minor | A `NoInfer<T>` wrapper is removed from a function parameter position or generic constraint on an exported signature |
| `union-variant-added` | Warning | A variant is added to an exported discriminated (tagged) union — may break exhaustive `switch`/`match` consumers |
| `index-signature-key-pattern-changed` | Major (narrowing) / Minor (widening) | The template-literal key pattern of an exported index signature (e.g. `` [key: `on${string}`]: T ``) is narrowed or widened |

When type enrichment is unavailable, findings identify missing evidence:

- `limitation: satisfies-narrowed-type-not-verified` — TYPE-01 degraded mode (type evaluation unavailable); rule still fires syntactically but narrowing direction is not confirmed.

See `docs/rules/type-narrowing.md`, `docs/rules/infer-extends.md`, `docs/rules/noinfer-wrapper.md`, `docs/rules/union-variant-added.md`, and `docs/rules/template-index-signature.md` for per-rule detail.

### Class members

| Rule ID | Severity | Trigger |
|---------|----------|---------|
| `class-field-made-private` | Major | CLS-11 — public `foo` on an exported class becomes private `#foo` (ES2022); consumers referencing the member will break |
| `class-field-made-public` | Minor | CLS-11 — private `#foo` on an exported class becomes public `foo`; previously-hidden surface is now observable |

A field rename within the private space (`#foo` → `#bar`) is deliberately silent — consumers cannot observe private fields, so a private-space rename is not an API change. A class that legally has BOTH public `foo` AND private `#foo` on both sides also fires no CLS-11 findings (the collision guard detects that both bare names exist on both sides).

See `docs/rules/class-field-visibility-transition.md` for matching heuristic, limitations, and decorator interaction.

CLS-12 (class static initialization block member extraction) reuses the existing `class-method-removed` rule-ID for removed static-block assignments (`class Foo { static { Foo.bar = 1 } }` — removing `Foo.bar` fires Major `class-method-removed` with the `static property` noun) and resolves literal, constant, template, and symbol keys. Unresolved runtime keys carry `limitation: dynamic-static-member-key` and make analysis incomplete. See [docs/rules/class-static-block.md](docs/rules/class-static-block.md).

### Limitations (first-class `limitation:` rule-IDs)

Missing compiler evidence, unresolved inputs, and processing limits produce incomplete
analysis (exit 3). Informational limitation findings explain the affected rule;
they do not establish that a change is compatible or that a feature is impossible
to implement.

| Emitted limitation ID | Trigger |
| --- | --- |
| `limitation: lsp-required` | Type resolution was unavailable for a `typeof import` reference. |
| `limitation: satisfies-narrowed-type-not-verified` | A `satisfies` removal was detected without resolved narrowed-type evidence. |
| `limitation: dynamic-static-member-key` | A computed class key or static assignment depends on unresolved runtime evidence; paired with `analysis-incomplete`. |
| `limitation: proxy-dispatched-exports` | An exported proxy or reflective construction has unresolved dispatch; paired with `analysis-incomplete` on either side of the comparison. |
| `limitation: decorator-runtime-effect-opaque` | A decorator change was detected; its arbitrary runtime effects are not proven. |
| `limitation: private-brand-access-not-checkable` | A public/private field transition was detected; actual consumer reliance is unknown. |

Local `NoInfer` aliases and installed external type references are resolved by the
compiler. Symbol-keyed exports include all standard well-known symbols and
constant `Symbol.for` registry keys, which have specified identity semantics.
See [computed properties](docs/rules/computed-properties.md) for coverage and
unresolved-key reporting.

Fresh empty proxy handlers and direct `Reflect` trap forwarding preserve the
target's property surface. Ordinary `Reflect.construct` calls retain the
constructed type. Arbitrary traps, mutable handler aliases, and custom
construction prototypes require runtime evidence. These checks describe the
property surface; they do not prove arbitrary method behavior or private-brand
checks through a proxy.

See [the rule reference](docs/rules/README.md) for individual rules.

## Development

```sh
git clone https://github.com/stentorian-io/pledge.git
cd pledge
make setup            # activate local Git hooks (requires Bun)
make check            # formatting, clippy, and default release tests
make check-lsp        # real language server tests
make check-all        # tooling, default tests, LSP tests, and coverage
```

The Cargo package and executable are still named `interlint`. Checks run
locally; there are no hosted CI workflows or pipelines yet. See
[CONTRIBUTING.md](./CONTRIBUTING.md) for prerequisites and conventions, and
[tests/README.md](./tests/README.md) for focused test commands.

See [the documentation index](docs/README.md) for architecture, rule, and
reference-suite guides.

## Licensing

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.

Third-party package fixtures retain their respective licenses.
