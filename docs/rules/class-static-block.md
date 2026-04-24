# Class static initialization block member extraction (CLS-12)

ES2022 introduced class static initialization blocks —
`class Foo { static { ... } }` — which run at class-definition time and can
assign properties on the class itself. Side-effect assignments to `Foo.bar`
or `this.bar` inside a static block contribute to the class's public runtime
surface.

Interlint extracts these assignments as synthesized static Property members
and tracks them via the existing
[class-member removal rule (CLS-03)](./class.md).

See the [TypeScript 4.4 release notes — Class Static
Blocks](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-4-4.html)
for the language feature.

## Supported assignment shapes

| Shape | Extracted | Notes |
| --- | --- | --- |
| `Foo.bar = expr` | Yes | Object must textually match the enclosing class name |
| `this.bar = expr` | Yes | Supports anonymous `export default class` |
| `Foo.Bar.baz = expr` | No | Depth-2+ member chains out of scope (depth-1 only) |
| `Foo[computed()] = expr` | No | Subscript expressions surface a `limitation:` annotation (see below) |
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

When a static block contains a subscript-expression LHS like
`Foo[computed()] = x` or `Foo[Symbol.iterator] = x`, Interlint cannot
statically determine the property name and does not extract the assignment.
Instead, a single Minor finding is emitted with the message substring
`(limitation: dynamic-static-member-key)` so consumers know Interlint
declined to diff these members:

```typescript
export class Registry {
  static {
    Registry[key] = 'value';
    // Interlint emits: "static initialization block on class `Registry`
    //  contains dynamic-key assignment(s) — members behind computed
    //  keys are not diffed (limitation: dynamic-static-member-key)"
  }
}
```

Well-known Symbol exports (e.g. `Symbol.iterator`, `Symbol.asyncIterator`)
will become first-class in a future milestone — tracked as Phase 22 ECF-13.
Generic computed keys remain under this limitation.

### Nested scope and depth-2 member chains

Any assignment inside a nested block scope (`static { { Foo.x = 1 } }`)
or with a depth-2+ LHS (`Foo.Bar.baz = expr`) is silently skipped. These
patterns are rare in practice; consumers relying on them should document
public class surface in the standard class body rather than in static-block
side-effects.

## Why no new rule-ID?

Phase 21 reuses the existing class-member rules (`class-method-removed`,
`class-static-type-changed`) because the class's public surface is the
same whether a static property was declared at the class-body level
(`class Foo { static bar = 1 }`) or in a static-block side-effect
(`class Foo { static { Foo.bar = 1 } }`). Consumers cannot tell the two
apart at runtime, so Interlint should not split the diff semantics.
