# Tuple compatibility

Tuple element types, optional markers, and rest markers determine assignability.
Making `[value?: string]` required or replacing `[...values: number[]]` with
`[values: number[]]` emits Major `type-incompatible`. Function parameters also
use these markers when checking whether previously accepted arguments remain valid.

Labels describe positions; they do not change the values or their order.
`tuple-label-reordered` and `tuple-label-dropped` are Patch findings.
`tuple-label-renamed` and `tuple-label-added` remain Minor informational changes.
Changes to element types are checked independently of labels.

Unresolved generic rest elements and changed middle rest elements are compared
conservatively. The checker does not expand arbitrary variadic type expressions.

See the [TypeScript tuple documentation](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-4-0.html#labeled-tuple-elements).
