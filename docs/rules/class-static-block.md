# Class static initialization block member extraction (CLS-12)

ES2022 introduced class static initialization blocks —
`class Foo { static { ... } }` — which run at class-definition time and can
assign properties on the class itself. Side-effect assignments to `Foo.bar`
or `this.bar` inside a static block contribute to the class's public runtime
surface.

Interlint extracts these assignments as synthesized static Property members
and tracks them via the existing
[class-member removal implementation](../../src/compare/class_diff.rs).

See the [TypeScript 4.4 release notes — Class Static
Blocks](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-4-4.html)
for the language feature.

## Supported assignment shapes

| Shape | Extracted | Notes |
| --- | --- | --- |
| `Foo.bar = expr` | Yes | Object must textually match the enclosing class name |
| `this.bar = expr` | Yes | Supports anonymous `export default class` |
| `Foo.Bar.baz = expr` | No | Depth-2+ member chains out of scope (depth-1 only) |
| `Foo["name"]`, `this[KEY]`, `Foo[Symbol.iterator]`, `Foo[Symbol.for("key")]` | Yes | Constant keys are resolved, including local immutable aliases and template strings |
| `Foo[computed()] = expr` | No | Unresolved expressions produce incomplete analysis (see below) |
| Non-assignment statements | No | Side-effects without a member assignment are ignored |
| Nested block scope | No | `static { { Foo.x = 1 } }` — only direct children of `static { }` are walked |

## Rule reuse

CLS-12 does **not** introduce a new rule-ID. Synthesized static-block
members flow through the existing class-member diff:

- **Removal** of a previously-assigned member fires Major
  `class-method-removed` with the finding message noun `static property`.
  Consumers referring to `Registry.defaults` will break when that
  assignment disappears from the static block.
- **Addition** fires no finding, matching existing CLS-* semantics (added
  class surface is not a breaking change).
- **Type change** would fire Major `class-static-type-changed` — but
  synthesized static-block members never carry a `type_signature` (no LSP
  round-trip on these), so this rule does not fire on them in practice.

Last-write-wins deduplication is applied within a single class: if a static
block contains `Foo.x = 1; Foo.x = 2;`, exactly one synthesized member for
`x` remains (matching runtime semantics — the final assignment is what
consumers observe). Multiple `static { ... }` blocks in one class are all
walked — tree-sitter emits each as a separate `class_static_block` child.

## Limitations

### `limitation: dynamic-static-member-key`

Computed keys are folded when their value follows from a literal, immutable
local binding, template string, or standard symbol expression. TypeScript
parentheses and `as` casts around the class reference do not hide assignments.
Changing a constant key removes the old public member. Equivalent key aliases
and escaped spellings do not produce a false break.

Unresolved keys produce an `analysis-incomplete` warning as well as the existing
Minor `class-static-type-changed` and Info
`limitation: dynamic-static-member-key` findings. A runtime environment key or
unknown function result requires additional evidence; an unrelated object's
computed assignment does not affect the class surface.

### Nested scope and depth-2 member chains

Any assignment inside a nested block scope (`static { { Foo.x = 1 } }`)
or with a depth-2+ LHS (`Foo.Bar.baz = expr`) is silently skipped. These
patterns are rare in practice; consumers relying on them should document
public class surface in the standard class body rather than in static-block
side-effects.

## Shared class-member rules

Static block comparisons reuse the existing `class-method-removed` and
`class-static-type-changed` rules because the class's public surface is the
same whether a static property was declared at the class-body level
(`class Foo { static bar = 1 }`) or in a static-block side-effect
(`class Foo { static { Foo.bar = 1 } }`). Consumers cannot tell the two
apart at runtime, so Interlint should not split the diff semantics.
