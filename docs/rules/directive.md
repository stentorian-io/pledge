# Triple-slash directive rules (INF-05)

TypeScript's triple-slash `/// <X ... />` pseudo-XML directives are part of a
package's public interface in two senses:

1. `/// <reference path="..." />` and `/// <reference types="..." />` declare
   ambient type dependencies that consumers inherit when they import the
   package.
2. `/// <amd-module name="..." />` and `/// <amd-dependency path="..." />`
   define the module name and factory dependencies of AMD-bundled output.

Interlint's Phase 19 `comment_scan` pass extracts directives from
`.d.ts`/`.ts` files and diffs them between the old and new versions of the
package.

## Recognized directive forms

| Form | Example |
|------|---------|
| `<reference path="..." />` | `/// <reference path="./chunk.d.ts" />` |
| `<reference types="..." />` | `/// <reference types="node" />` |
| `<reference lib="..." />` | `/// <reference lib="es2017" />` |
| `<reference no-default-lib="true" />` | `/// <reference no-default-lib="true" />` |
| `<amd-module name="..." />` | `/// <amd-module name="NamedModule" />` |
| `<amd-dependency path="..." name="..." />` | `/// <amd-dependency path="legacy/moduleA" name="moduleA" />` |

TypeScript 5.5+ adds a `preserve="true"` attribute on reference/types/lib
directives; Interlint captures it and treats its toggle as a
`directive-changed` (see below).

## Position rule

Triple-slash directives are only recognized when they appear at the top of the file, before any non-comment statement. This matches the TypeScript compiler's behavior: directives buried deeper in the file are treated as plain comments and do NOT contribute to a directive-family finding.

Leading plain comments (`// ...`, `/* ... */`) before a `///` directive do
NOT break the directive window — only non-comment statements do.

## Rule reference

### `directive-added` — Major

A directive present in the new version was not present in the old version.

```typescript
// old/src/index.d.ts
export declare const x: number;

// new/src/index.d.ts
/// <reference path="./x.ts" />
export declare const x: number;
```

### `directive-removed` — Major

A directive present in the old version was dropped in the new version.

```typescript
// old/src/index.d.ts
/// <reference types="node" />
export declare const x: number;

// new/src/index.d.ts
export declare const x: number;
```

### `directive-changed` — Major

A directive with the same name and the same set of attribute keys has a
different attribute value.

```typescript
// old — /// <reference lib="es2015" />
// new — /// <reference lib="es2017" />
```

This fires exactly one `directive-changed` — not one `directive-added` plus
one `directive-removed`. Toggling the TS 5.5+ `preserve="true"` attribute
is treated as a `directive-changed` as well because it alters the emit
behavior of the compiler.

### `directive-reordered` — Minor

The same set of directives appears in both versions, but in a different
document order. Reference-path resolution is depth-first in declaration
order, so reordering can affect the order in which ambient types are
resolved during consumption. Pure reorders never surface as 1 `added` +
1 `removed`.

## Attribute-order equivalence

Attribute order inside a single directive is NOT significant. The two forms

```typescript
/// <amd-dependency path="a" name="b" />
/// <amd-dependency name="b" path="a" />
```

produce the same `DirectiveRecord` and compare equal — because
`DirectiveRecord.attrs` is a `BTreeMap<String, String>` whose `PartialEq`
is order-independent.
