# Discriminated union variant added (BEH-06)

A discriminated (tagged) union has a common discriminant property in
every variant, each carrying a literal type:

```typescript
export type Shape =
  | { kind: 'circle'; radius: number }
  | { kind: 'square'; side: number };
```

Consumers commonly exhaustive-switch on the discriminant:

```typescript
function area(s: Shape): number {
  switch (s.kind) {
    case 'circle': return Math.PI * s.radius ** 2;
    case 'square': return s.side * s.side;
  }
  // With `noImplicitReturns`/strict mode and an exhaustive switch, TS
  // verifies the function returns on every branch. Adding a new variant
  // to `Shape` silently breaks that check at the consumer.
}
```

Adding a new variant — even though it *widens* the type (technically a
compatible direction) — breaks the exhaustiveness check at the
consumer's switch. This is the same shape of problem as
[`enum-member-added`](./enum-member-added.md) (BEH-04), and both surface
at the same severity.

## Rule reference

### `union-variant-added` — Warning (Tier 2)

A variant is added to an exported discriminated union between versions.
Severity is Warning, matching BEH-04 `enum-member-added`. Exit code is 2
(Warning) — not 1 (Major) — because the change is not a compile-time
break for all consumers, only those relying on exhaustive switch.

```typescript
// old
export type Shape =
  | { kind: 'circle'; radius: number }
  | { kind: 'square'; side: number };

// new — variant added
export type Shape =
  | { kind: 'circle'; radius: number }
  | { kind: 'square'; side: number }
  | { kind: 'triangle'; base: number; height: number };
//  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
//  fires: Warning union-variant-added (kind: 'triangle')
```

Fires one Warning per added variant, keyed by discriminant literal
value. The message names the discriminant property (`kind`) and the
added literal value (e.g. `"triangle"`).

## Scope

### Discriminated unions only

Interlint's BEH-06 detection fires **only** on discriminated unions.
Plain-union widening — e.g., `number | boolean` → `number | boolean | string`
— is structurally compatible and produces no finding. Only consumers of
the full discriminated shape can exhaustive-switch on a discriminant; a
plain-union value cannot be exhaustively destructured the same way, so
the consumer contract differs.

### Discriminant heuristic

A union is treated as "discriminated" when **both** of the following
hold:

1. Every variant is an object type, AND
2. At least one property name appears in every variant with a literal
   type (`'circle'`, `42`, `true`, etc.)

The first such property name (in BTreeMap order over the first variant's
fields) is the discriminant. If multiple candidate discriminants exist
(e.g. a union with both `kind:` and `type:` literal-typed in every
variant), the alphabetically-first one wins; practically the choice
does not affect the result because a discriminant-literal added in the
new version will register under either picked discriminant.

### Scope boundary — what BEH-06 does NOT cover

- **Plain-union widening** (`number | boolean` → `number | boolean | string`):
  no finding. This is existing TYPE-diff compatible-widening behavior
  and is preserved. Use the plain-union rules (`type-incompatible` on
  narrowing) for those shapes.
- **Non-literal discriminants**: a union like
  `{ tag: typeof SymbolA } | { tag: typeof SymbolB }` uses symbol-typed
  discriminants rather than literal-typed. The locked v0.3 heuristic
  requires `TypeShape::Literal(...)`, so these unions are treated as
  plain. Phase 22's ECF-13 work covers well-known symbols separately.
- **Local-alias obfuscation**: if a consumer aliases the discriminant
  type (`type Kind = 'circle' | 'square'; type Shape = { kind: Kind; ... };`)
  the discriminant values are not visible as literals on the variant
  field — BEH-06 will not fire. This is an acceptable false-negative;
  the canonical tagged-union pattern inlines the literal.

## How to fix

If you intentionally added a variant to a discriminated union in a
non-major release:

- Treat the release as a minor break at the consumer-contract level even
  though it's structurally compatible. Consumers running with exhaustive
  switches will need to add the new case.
- Document the added variant in your release notes.
- If the new variant can be avoided (e.g. by a secondary type alias),
  prefer that path to keep strict consumers unbroken.

If the variant was added unintentionally, restore the original union
shape.

## Related

- **BEH-04 `enum-member-added`** — the same exhaustive-switch-break
  concern applied to TypeScript `enum` declarations. Same severity
  (Warning), same message rationale.
- **TypeScript handbook — Discriminated unions**: <https://www.typescriptlang.org/docs/handbook/2/narrowing.html#discriminated-unions>
