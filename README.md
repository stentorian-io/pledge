# interlint

A CLI tool that catches breaking changes to public interfaces through static analysis. Compares the current version of a JS/TS package against its previously published version on npm and reports findings with semver-aligned severity -- like ESLint for API compatibility.

## Installation

```sh
cargo build --release
```

## Usage

```sh
interlint [path]
```

Analyzes the package at `path` (defaults to current directory), compares it against the previously published npm version, and reports breaking changes.

## Rules

Interlint emits findings tagged with stable rule-IDs. Severity maps to CLI exit code:

| Severity | Exit code | Meaning |
|----------|-----------|---------|
| Major | 1 | Breaking change — consumers will observe broken behavior |
| Warning | 0 | Observable behavior change — exhaustive-switch breaks, deprecation. Reported but does not break the build. |
| Degraded | 3 | Analysis could not complete cleanly (compiler unavailable, unresolved types, parse failure) |
| Minor | 0 | Non-breaking change worth noting |
| Patch | 0 | Cosmetic / non-observable change |
| Info | 0 | Documented limitation — analysis acknowledged but couldn't verify (Phase 25) |

Exit code 2 is reserved for tool-internal errors (e.g., I/O failure, registry unavailable). It is NOT used for any finding severity.

Incomplete analysis takes precedence over breaking findings and exits 3. This
includes failed or empty type requests, exhausted analysis budgets, unreadable
public sources, and files skipped by source processing limits. Findings from
completed analysis are still reported; an incomplete result does not establish
compatibility.

### Phase 19 rule-IDs (comment-scan pass)

| Rule ID | Severity | Trigger |
|---------|----------|---------|
| `directive-added` | Major | A `/// <reference ... />`, `/// <amd-module ... />`, or `/// <amd-dependency ... />` directive is added at the top of a source file |
| `directive-removed` | Major | A triple-slash directive present in the old version is dropped in the new version |
| `directive-changed` | Major | A directive's attribute value changes (including the TS 5.5+ `preserve="true"` toggle) |
| `directive-reordered` | Minor | The same set of directives appears in both versions in a different document order |
| `decorator-added` | Major | A stage-3 decorator (class, method, field, or parameter) is added to an exported target |
| `decorator-removed` | Major | A stage-3 decorator is removed from an exported target |
| `decorator-arguments-changed` | Major | A decorator factory's call arguments change (e.g., `@Route('/users')` → `@Route('/people')`) |

Phase 19 also extends the existing `export-deprecated` (Warning) rule: `@deprecated`, `@internal`, `@alpha`, `@beta`, and `@public` TSDoc tags are now detected uniformly across every `RawExport` construction site (default exports, named re-exports, destructured pairs, compound-assignment member exports, star re-exports, and all bundler-CJS variants).

See `docs/rules/directive.md` for INF-05 (triple-slash directives) details and the TypeScript compiler position rule.

### Phase 20 rule-IDs (TS type-system features)

| Rule ID | Severity | Trigger |
|---------|----------|---------|
| `type-narrowing-widened` | Minor | A `satisfies T` clause is removed and the public value type widens, or type evidence is unavailable |
| `infer-extends-constraint-changed` | Major | The `C` in `T extends infer U extends C ? ... : ...` on an exported conditional type is changed |
| `generic-inference-constraint-removed` | Minor | A `NoInfer<T>` wrapper is removed from a function parameter position or generic constraint on an exported signature |
| `union-variant-added` | Warning | A variant is added to an exported discriminated (tagged) union — may break exhaustive `switch`/`match` consumers |
| `index-signature-key-pattern-changed` | Major (narrowing) / Minor (widening) | The template-literal key pattern of an exported index signature (e.g. `` [key: `on${string}`]: T ``) is narrowed or widened |

When type enrichment is unavailable, findings identify missing evidence:

- `limitation: satisfies-narrowed-type-not-verified` — TYPE-01 degraded mode (type evaluation unavailable); rule still fires syntactically but narrowing direction is not confirmed.

See `docs/rules/type-narrowing.md`, `docs/rules/infer-extends.md`, `docs/rules/noinfer-wrapper.md`, `docs/rules/union-variant-added.md`, and `docs/rules/template-index-signature.md` for per-rule detail.

### Phase 21 rule-IDs (class-surface completion)

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
| `limitation: dynamic-static-member-key` | A computed class key depends on an unresolved expression; paired with `analysis-incomplete`. |
| `limitation: proxy-dispatched-exports` | An exported proxy or reflective construction requires runtime surface analysis. |
| `limitation: decorator-runtime-effect-opaque` | A decorator change was detected; its arbitrary runtime effects are not proven. |
| `limitation: private-brand-access-not-checkable` | A public/private field transition was detected; actual consumer reliance is unknown. |

Local `NoInfer` aliases and installed external type references are resolved by the
compiler. Symbol-keyed exports include all standard well-known symbols and
constant `Symbol.for` registry keys, which have specified identity semantics.
See [computed properties](docs/rules/computed-properties.md) for coverage and
unresolved-key reporting.

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
