# `satisfies` and public value types

`satisfies` checks a value against a type while retaining its inferred type.
Contextual typing can affect that inference, so removing the expression can
change the public contract:

```typescript
// old: value has literal type true
export const api = { value: true } satisfies { value: boolean };
// new: value has type boolean
export const api = { value: true };
```

A consumer assigning `api.value` to a variable of type `true` compiles only
against the old version. Pledge compares the compiler-resolved value types and
reports the incompatibility as Major. Removing `satisfies` also produces the
Minor `type-narrowing-widened` context finding when the resolved type widens.

Removal does not always change the inferred type. For example, removing
`satisfies { value: number }` from `{ value: 1 }` leaves `value: number` on both
sides. When the compiler proves the new type still fits the old type, Pledge
suppresses the narrowing finding.

Exported values use read compatibility: adding a property is compatible, while
removing a readable property or widening its type can break a consumer. Adding
`readonly` to a writable public property is also breaking.

If compiler evidence is unavailable, syntactic removal is still reported with
`limitation: satisfies-narrowed-type-not-verified`. Missing evidence keeps the
overall analysis incomplete; the informational finding is not a compatibility
verdict. Local and installed external types are resolved without requiring a
language-server process.
