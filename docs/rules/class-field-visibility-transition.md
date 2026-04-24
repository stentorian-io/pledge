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

Fields are paired by name-modulo-`#` (the `#` prefix is stripped for
identity comparison). A rename within the private space (`#foo` →
`#bar`) is deliberately silent — consumers cannot observe private
fields, so a private-space rename is not an API change.

## Limitations

- An unrelated refactor that happens to drop `foo` AND add `#bar` in
  the same commit will be misreported as `foo → #foo` followed by a
  silent drop of `#bar`. This is a known heuristic limitation of
  name-modulo-`#` matching.
- A class that legally has BOTH `public foo` AND `private #foo`
  simultaneously (distinct runtime slots) does NOT fire a CLS-11
  finding — the collision guard detects that both bare names exist on
  both sides. This may mask legitimate renames within such classes.
- Nominal-type brand removal (`#brand: unique symbol` used for
  structural compat nominal typing in libraries like RxJS / Effect) is
  NOT detected under CLS-11 — the rename stays within the private
  space. This is tracked as a Phase 25 DOC-01 limitation
  (`limitation: private-brand-access-not-checkable`).

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

Decorator diffs (Phase 19 CLS-10) and visibility-transition diffs
(Phase 21 CLS-11) treat **decorator changes** and **public↔private
transitions** as **independent axes** on a class member. When a member
changes on both axes between versions (e.g. `@a foo` becomes
`@b #foo`), Interlint emits one finding per axis:

- One `class-field-made-private` (Major) from CLS-11 — the visibility
  transition.
- One `decorator-added` / `decorator-removed` / `decorator-arguments-changed`
  (severity per CLS-10) from the Phase 19 decorator diff — the
  decorator change.

This co-fire is deliberate and accepted (per Phase 21 research, Risk 8
resolution): the two findings describe genuinely distinct API changes
that consumers must evaluate independently. Interlint does **not**
suppress one in favor of the other.
