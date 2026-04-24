# `infer U extends C` constraint diff (TYPE-02)

TypeScript 4.7's `infer U extends C` syntax lets authors constrain the
inferred type variable inside a conditional type:

```typescript
export type FirstIfString<T> =
  T extends { first: infer S extends string } ? S : never;
//                            ^^^^^^^^^^^^^^^
//                            the constraint
```

Changing the constraint `C` changes which input types narrow the
inferred `S` and what shape the resulting inferred type takes. That
propagates through to the consumer-observable result type of the
conditional.

See the [TypeScript 4.7 release
notes](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-4-7.html)
for the `extends`-on-infer feature.

## Rule reference

### `infer-extends-constraint-changed` — Major

The `C` in `infer U extends C` on an exported conditional type changes
between versions.

```typescript
// old
export type FirstIfString<T> =
  T extends { first: infer S extends string } ? S : never;

// new — constraint changed from `string` to `number`
export type FirstIfString<T> =
  T extends { first: infer S extends number } ? S : never;
```

Severity is Major. Any consumer relying on the pre-change constraint to
shape the inferred type will observe different behavior — inputs that
previously narrowed now fail to narrow, and vice versa.

## Scope

The infer-identifier name (`U` vs `V`) is NOT diffed. Consumers cannot
observe the inferred-variable's identifier — only the resulting type.
Renaming `infer U extends string` to `infer V extends string` fires no
finding.

Only the `extends` constraint triggers `infer-extends-constraint-changed`.
Changing the wider conditional structure (the `check` type, `then_type`,
or `else_type` legs) fires standard TYPE-diff rules — not this one.

## How to fix

If you need to adjust the constraint, consider whether the public
surface truly needs to change. If consumers depend on the old constraint
shape, bump the major version. If the constraint tightening is a bug
fix (the old constraint was too loose and allowed invalid narrows),
document the fix in your release notes so downstream consumers know to
retest their inferred types.
