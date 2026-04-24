# Template-literal index signature key pattern diff (REF-09)

TypeScript 4.4 introduced template-literal types as index signature
keys, letting authors constrain the valid key shapes of a mapping type:

```typescript
export interface EventMap {
  [key: `on${string}`]: (e: Event) => void;
}
// Valid keys: `onClick`, `onHover`, `on…` — anything matching `on${string}`.
```

Narrowing the key pattern (`` `on${string}` `` → `` `onClick${string}` ``)
drops the previously-valid keys `onHover`, `onMouseOver`, etc. from the
type. Widening the pattern (the reverse direction) is additive.

See the [TypeScript 4.4 release notes](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-4-4.html)
for the template-literal-in-index-signature feature.

## Rule reference

### `index-signature-key-pattern-changed`

| Direction | Severity |
|-----------|----------|
| Narrowing (new pattern is a strict prefix-extension of old; fewer keys valid) | Major |
| Widening (old pattern is a strict prefix-extension of new; more keys valid) | Minor |
| Unrelated (neither prefix-extends the other) | Major (conservative) |
| Equal | no finding |

**Narrowing example:**
```typescript
// old
export interface EventMap {
  [key: `on${string}`]: (e: Event) => void;
}

// new — narrowed: any `onHover` / `onMouseOver` key now invalid
export interface EventMap {
  [key: `onClick${string}`]: (e: Event) => void;
}
```
Fires Major `index-signature-key-pattern-changed`.

**Widening example:**
```typescript
// old
export interface EventMap {
  [key: `onClick${string}`]: (e: Event) => void;
}

// new — widened: all previously-valid keys still valid, plus more
export interface EventMap {
  [key: `on${string}`]: (e: Event) => void;
}
```
Fires Minor `index-signature-key-pattern-changed`.

## Algorithm

Interlint parses both the old and new `key_type_text` into a
`Vec<TemplatePart>` (literal segments interleaved with placeholder
types). Direction is computed by:

1. Concatenating the literal segments of each side.
2. Requiring placeholder types to match structurally (equal `TypeShape`).
3. Comparing concatenated literal strings for prefix-extension:
   - new-literals is a strict extension of old-literals → Narrowing.
   - old-literals is a strict extension of new-literals → Widening.
   - Otherwise → Unrelated (conservative Major).

This is a narrow algorithm. More nuanced subset relations involving
placeholder-type subtyping (e.g., `` `${Capitalize<string>}` `` being
a subtype of `` `${string}` ``) are NOT computed — they fall to the
conservative-Major path.

## Scope

REF-09 fires at two surfaces:
- **Type aliases** — `export type X = { [key: \`on${string}\`]: T }`
- **Interfaces** — `export interface X { [key: \`on${string}\`]: T }`

Classes are out of scope — TypeScript does not allow index signatures
on class bodies directly (only on implemented interfaces).

## Interaction with REF-07

The existing REF-07 `index-signature-changed` rule fires for any
string-inequality between two key types. Without dedup, a template-
literal pattern change would produce THREE findings:
- REF-07 `index-signature-changed` "removed `` `on${string}` ``"
- REF-07 `index-signature-changed` "added `` `onClick${string}` ``"
- REF-09 `index-signature-key-pattern-changed` (narrowing)

Interlint suppresses the REF-07 pair (and its interface-level twin
`interface-index-signature-changed`) for any signature matched by
REF-09. Consumers see exactly ONE finding per pattern change — the
REF-09 rule with direction-aware severity.

## How to fix

- If the narrowing was intentional, bump the major version and document
  the change in release notes; callers with pattern keys that no longer
  match must update their key literals.
- If the narrowing was accidental (e.g., a typo in the literal prefix),
  restore the original pattern.
- For widening (Minor), no code change is required on the consumer side —
  existing keys remain valid. Document the expanded key space in release
  notes so consumers know the new keys are supported.
