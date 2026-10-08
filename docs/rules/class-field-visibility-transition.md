# Class field visibility transition (CLS-11)

ES2022 introduced `#`-prefixed private class fields whose identity is
distinct from same-named public fields at runtime. Transitioning a
field across the public ↔ private boundary is an observable change on
a package's public API surface.

Interlint tracks this transition on every exported class field and
method.

See the [TypeScript 3.8 release notes (ECMAScript private class fields)](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-3-8.html)
for the language feature.

## Rules

| Rule ID | Severity | Direction |
| --- | --- | --- |
| `class-field-made-private` | Major | public `foo` → private `#foo` |
| `class-field-made-public` | Minor | private `#foo` → public `foo` |

### Public-to-private example (Major)

```typescript
// old
export class UserRepo {
  cache: Map<string, string>;
}

// new — `cache` access now fails to compile in consumer code
export class UserRepo {
  #cache: Map<string, string>;
}
```

Fires Major `class-field-made-private`.

### Private-to-public example (Minor)

```typescript
// old
export class UserRepo {
  #cache: Map<string, string>;
}

// new — previously-hidden surface now observable
export class UserRepo {
  cache: Map<string, string>;
}
```

Fires Minor `class-field-made-public`.

## Matching heuristic

Fields are paired by their name without the `#` prefix and by whether they
are static. A static field is never paired with an instance field. A rename
within the private space (`#foo` → `#bar`) is deliberately silent — consumers cannot observe private
fields, so a private-space rename is not an API change.

## Limitations

- A field that changes both its name and visibility (`foo` → `#bar`)
  cannot be paired as a visibility transition. The matching rule requires
  the same bare name.
- A class that legally has BOTH `public foo` AND `private #foo`
  simultaneously (distinct runtime slots) does NOT fire a CLS-11
  finding — the collision guard detects that both bare names exist on
  both sides. This may mask legitimate renames within such classes.
- Nominal-type brand removal (`#brand: unique symbol` used for
  structural compat nominal typing in libraries like RxJS / Effect) is
  NOT detected under CLS-11 — the rename stays within the private
  space. Visibility transitions also emit an Info
  `limitation: private-brand-access-not-checkable` finding because consumer
  reliance on private-brand checks cannot be verified statically.

## Scope

- TypeScript `.ts` / `.tsx` source files (the dominant consumer shape).
- JavaScript `.js` files are NOT covered; the underlying
  `tree-sitter-javascript` 0.25.0 grammar uses `field_definition` (vs.
  TypeScript's `public_field_definition`) and is not currently wired
  into `parse_class_members`. This is a known limitation.
- Class-level ambient `.d.ts` `property_signature` declarations ARE
  covered — private fields in ambient class declarations are tracked
  symmetrically with source-level class fields.

## Decorator interaction

Class-level decorators and decorators on surviving public members are compared
separately from field visibility. Private-member decorators are excluded from
public-surface comparisons.

For `@a foo` → `@b #foo`, the public/private transition is reported along with
the private-brand limitation. The private member does not produce a decorator
change finding. Unrelated decorator changes on other public members can still
produce their own findings.
