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
an inference change: the inferred result may become wider and affect
consumers that relied on the previous result.

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

The inference-change finding is Minor. The ordinary type rules separately
report observed parameter or return-type incompatibilities.

### `generic-inference-constraint-added` — Major

Adding `NoInfer<T>` can reject calls that previously inferred `T` from multiple
arguments. For example, `<T extends string>(value:T, fallback:T)` accepts
`("red", "blue")`; changing the second parameter to `NoInfer<T>` rejects that
call. Local aliases and generic callables returned by an API are included.

## Scope

Interlint tracks `NoInfer` at the two positions above.

Callable contracts returned by an API, or exposed as object properties, are
also inspected. A `NoInfer` wrapper used only as a non-callable result type does
not change that result's assignability.

## How to fix

If removing `NoInfer<T>` is intentional and you accept that consumer
inference may now derive a wider type parameter, ship the change in a
minor (or major, by team preference) release with a release-notes
callout. There is no automatic migration — the inference shift is the
intended behavioral change.

If the removal was unintentional (e.g., a refactor stripped the
wrapper), restore `NoInfer<T>` at the affected position to silence
the rule.

## Alias resolution and degraded analysis

Compiler enrichment resolves local and imported aliases of `NoInfer<T>` before
comparison. An alias rename retains the same inference behavior; removing its
wrapper produces `generic-inference-constraint-removed`. `NoInfer<T>` and `T`
have the same assignability, so removing the wrapper alone is not reported as
parameter narrowing.

Without compiler information, only the available source shapes can be compared.
Missing type evidence produces incomplete analysis. There is no emitted
`limitation: noinfer-via-local-alias` finding.
