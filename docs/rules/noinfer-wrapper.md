# `NoInfer<T>` wrapper diff (GEN-07)

TypeScript 5.4 introduced the `NoInfer<T>` utility type. It marks a
position as "do not infer from here" so the type system derives the
type parameter from some other position:

```typescript
export function createStore<T>(
  initial: T,
  update: (prev: NoInfer<T>) => T
): T;
//                  ^^^^^^^^^^^^^
//                  blocks inference from `update`'s param;
//                  T is inferred from `initial` only
```

See the [TypeScript 5.4 release notes](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-5-4.html)
for the `NoInfer<T>` feature and
[Total TypeScript's explainer](https://www.totaltypescript.com/noinfer)
for the motivation.

Removing `NoInfer<T>` allows consumers' previously-blocked inference
paths to fire, changing the shape of inferred type parameters. This is
not a hard compile break — consumer code continues to type-check —
but inference behavior shifts and may cause the wider type to be
derived.

## Rule reference

### `generic-inference-constraint-removed` — Minor

Fires when `NoInfer<T>` is removed from a position that Interlint
tracks. Two positions are in scope:

**Function-parameter position** (TS 5.4+ common case):

```typescript
// old
export function createStore<T>(
  initial: T,
  update: (prev: NoInfer<T>) => T
): T;

// new — NoInfer removed from `prev`
export function createStore<T>(
  initial: T,
  update: (prev: T) => T
): T;
```

Fires one Minor `generic-inference-constraint-removed` per position
where the wrapper was removed.

**Type-parameter constraint position** (less common but supported):

```typescript
// old
export interface Store<T extends NoInfer<Base>> { item: T; }

// new — NoInfer removed from constraint
export interface Store<T extends Base> { item: T; }
```

Fires Minor `generic-inference-constraint-removed`.

Severity is Minor. Removing `NoInfer` does not break the type
contract; it changes the *inference* of the type parameter, which
consumers may or may not have relied on.

## Scope

Interlint tracks `NoInfer` at the two positions above.

Return-type positions are NOT tracked — return-type inference doesn't
have the same consumer-observable semantics as parameter inference,
and a consumer's observable behavior at a return position is
dominated by the return type itself rather than by what got inferred
for the type parameter.

## How to fix

If removing `NoInfer<T>` is intentional and you accept that consumer
inference may now derive a wider type parameter, ship the change in a
minor (or major, by team preference) release with a release-notes
callout. There is no automatic migration — the inference shift is the
intended behavioral change.

If the removal was unintentional (e.g., a refactor stripped the
wrapper), restore `NoInfer<T>` at the affected position to silence
the rule.

## Limitations

### `limitation: noinfer-via-local-alias`

Interlint recognizes `NoInfer` only when the type is written verbatim.
Users who alias `NoInfer` locally — e.g.,

```typescript
type MyNoInfer<T> = NoInfer<T>;
export function f<T>(arg: MyNoInfer<T>): T;
```

— produce a `Generic { name: "MyNoInfer", args: [T] }` shape in the
diff engine, not `NoInferWrap(T)`. Removing `MyNoInfer` in this case
won't fire `generic-inference-constraint-removed` even though the
inference effect is identical.

This is a known limitation and surfaces in CLI output as
`limitation: noinfer-via-local-alias`.
