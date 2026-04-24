# `satisfies` narrowing (TYPE-01)

The TypeScript 4.9 `satisfies` operator lets a value be checked against a
type without widening the inferred type of the value:

```typescript
export const routes = {
  home: { path: '/' },
  about: { path: '/about' },
} satisfies Record<string, { path: string }>;
// inferred type: `{ readonly home: { readonly path: "/"; }; ... }`
```

Removing the `satisfies` clause — even while keeping the value identical
— widens the publicly-observable type. Consumers that relied on the
narrowed literal types (e.g., `routes.home.path` having literal-string
type `"/"`) will observe the wider `string` type.

## Rule reference

### `type-narrowing-widened` — Minor

A `satisfies T` clause present on an exported const/let/var in the old
version is absent in the new version.

```typescript
// old
export const routes = {
  home: { path: '/' },
} satisfies Record<string, { path: string }>;

// new — satisfies removed
export const routes: Record<string, { path: string }> = {
  home: { path: '/' },
};
```

Severity is Minor: the value still satisfies the original constraint, so
no hard compile break. However, consumers that inferred literal-string
types from the narrowed form will now infer the declared type.

## Limitations

### `limitation: satisfies-narrowed-type-not-verified`

When `typescript-language-server` is unavailable (degraded mode),
Interlint detects `satisfies` presence syntactically (via tree-sitter
alone) but cannot confirm that the narrowed type was actually surfaced
in the public signature through LSP hover. In that case, the finding
message carries `limitation: satisfies-narrowed-type-not-verified` so
users know narrowing direction was not confirmed. The rule still fires
— Interlint never silently drops coverage.

## Scope

Interlint diffs `satisfies` on exported const/let/var bindings only.
TypeScript does not currently permit `satisfies` on type-alias RHS
(`type X = Y satisfies Z`) — see [TypeScript issue
#52222](https://github.com/microsoft/TypeScript/issues/52222). If that
syntax lands in a future TS release, support will be added as a
follow-up.
