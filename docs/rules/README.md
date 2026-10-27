# Rule reference

Use these pages to understand why a change was reported, how it affects a
consumer, and what the rule can establish. Start with
[reading results](../results.md) for severities and exit codes.

These guides cover selected rules; they are not an exhaustive inventory of
every implemented check.

| Area | Reference |
| --- | --- |
| Public exports | [Export resolution](export-resolution.md) |
| Referenced types | [Reachable public types](reachable-types.md) |
| Package requirements | [Version ranges and platforms](package-requirements.md) |
| Runtime loading | [Synchronous module loading](module-loading.md) |
| Computed exports and runtime surfaces | [Properties, symbols, and proxies](computed-properties.md) |
| Triple-slash directives | [Directive changes](directive.md) |
| Decorators | [Decorator changes](decorator.md) |
| Class visibility | [Public/private transitions](class-field-visibility-transition.md) |
| Accessors and subclasses | [Read and write contracts](accessor-contracts.md) |
| Static initialization | [Class static blocks](class-static-block.md) |
| `satisfies` | [Type narrowing](type-narrowing.md) |
| Constrained `infer` | [Inference constraints](infer-extends.md) |
| `NoInfer` | [Inference wrapper removal](noinfer-wrapper.md) |
| Tuples | [Tuple compatibility](tuple-compatibility.md) |
| Discriminated unions | [Added variants](union-variant-added.md) |
| Template index signatures | [Key pattern changes](template-index-signature.md) |

## Add or change a rule

Start with the consumer behavior: what worked before, what changed, and why the
consumer is affected. Include the exact emitted rule ID and severity, a small
before/after example, compatible cases, and the analysis limits. Distinguish
emitted informational findings from unsupported cases described only in prose.

Rule identifiers live beside their owning comparisons:
[JavaScript](../../src/languages/javascript/compare/rules.rs),
[TypeScript](../../src/languages/typescript/compare/rules.rs), and
[Node](../../src/ecosystems/node/compare/rules.rs). Verify documentation against
those contracts and the relevant behavioral tests. See the
[documentation conventions](../../CONTRIBUTING.md#documentation).
