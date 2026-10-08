# Rule reference

These pages describe selected compatibility rules, their severity, examples,
and static-analysis limits. Rule identifiers are defined in
[`src/compare/subtype.rs`](../../src/compare/subtype.rs); the list below is not
an exhaustive list of every implemented rule.

| Area | Reference |
| --- | --- |
| Tuples | [Tuple compatibility](tuple-compatibility.md) |
| Package requirements | [Version ranges and platforms](package-requirements.md) |
| Public exports | [Export resolution](export-resolution.md) |
| Referenced types | [Reachable public types](reachable-types.md) |
| Triple-slash directives | [Directive changes](directive.md) |
| Decorators | [Decorator changes](decorator.md) |
| Class visibility | [Public/private transitions](class-field-visibility-transition.md) |
| Static initialization | [Class static blocks](class-static-block.md) |
| `satisfies` | [Type narrowing](type-narrowing.md) |
| Constrained `infer` | [Inference constraints](infer-extends.md) |
| `NoInfer` | [Inference wrapper removal](noinfer-wrapper.md) |
| Discriminated unions | [Added variants](union-variant-added.md) |
| Template index signatures | [Key pattern changes](template-index-signature.md) |

For a new rule, document the exact emitted identifier, severity, trigger,
before/after example, and false-positive boundaries. Distinguish emitted
informational findings from unsupported cases that only have documentation.
Use behavior names in explanations rather than development phase numbers.
