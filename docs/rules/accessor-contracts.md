# Accessor contracts

TypeScript getters and setters can expose different read and write types, including
[unrelated types](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-5-1.html#unrelated-types-for-getters-and-setters).
Compiler enrichment preserves both, including instantiated generic setter types.
This applies to classes, interfaces, object literals, and returned objects.

A getter returning `number` can accept `string | number` through its setter.
Changing that setter to accept only `number` breaks consumers assigning a string,
even when the getter remains unchanged. Direct object comparisons report Major
`field-type-changed`; returned values and class contracts use their enclosing
structural type rule. Widening an accepted write type stays compatible.

Removing a class getter while retaining its setter produces Major
`class-method-removed`: a setter-only property no longer supplies the getter's
runtime value, even when TypeScript assigns it an apparent read type. Getter,
setter, and ordinary property transitions on classes also retain the existing
class member-kind rules because subclasses can depend on the declaration kind.

Protected member types belong to the subclass contract. Private implementation
members are excluded from public removal and decorator findings; visibility
transitions remain checked. Static and instance members with the same name are
compared separately.

Getter and setter bodies can perform arbitrary runtime effects. These rules
compare their declared contracts and accessor presence, without executing them.
