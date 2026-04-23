# Stage-3 decorator rules (CLS-10)

TypeScript and ECMAScript stage-3 decorators mark exported classes, methods,
fields, and parameters in ways consumers observe either:

- **Type-level**: through the decorator's name identity (e.g., `@Deprecated`
  advertised in docs); or
- **Source-identity**: a build-time diff of public API shape that
  includes which decorators are applied where.

Interlint's Phase 19 `tree_query::extract_decorators` reads decorators
directly from the `decorator` AST node in both `tree-sitter-typescript`
0.23.2 and `tree-sitter-javascript` 0.25.0 grammars. No comment-scan
fallback is used for decorators in Phase 19.

## Decorator positions supported

| Position        | Grammar placement                                                                                                               |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Class-level     | `class_declaration` / `abstract_class_declaration` / parent `export_statement` (TS); `class_declaration` / `export_statement` (JS) |
| Method-level    | Preceding-sibling decorators in `class_body` (TS); direct children of `method_definition` (JS)                                  |
| Field-level     | `public_field_definition` (TS); `field_definition` (JS)                                                                         |
| Parameter-level | `required_parameter` / `optional_parameter` decorators on constructor parameter-properties (TS)                                  |

## Decorator forms recognized

| Source form              | `DecoratorSpec.name`       | `DecoratorSpec.args`   |
| ------------------------ | -------------------------- | ---------------------- |
| `@Component`             | `"Component"`              | `None`                 |
| `@Route('/users')`       | `"Route"`                  | `Some("('/users')")`   |
| `@ns.log`                | `"ns.log"`                 | `None`                 |
| `@(expr)` (parenthesized)| source text of expr        | `None` (best-effort)   |

Argument bytes are source-text verbatim — Interlint does not parse argument
expressions semantically. Two decorator calls are considered argument-equal
iff their argument byte-text is byte-equal.

## Rule reference

### `decorator-added` — Major

A decorator present on a class, method, field, or parameter in the new
version was not present in the old version.

```typescript
// old — export class Foo {}
// new — @Component\nexport class Foo {}
```

### `decorator-removed` — Major

A decorator present in the old version was dropped in the new version.

### `decorator-arguments-changed` — Major

A decorator with the same name has different arguments between versions.

```typescript
// old — @Route('/users')
// new — @Route('/people')
```

This fires exactly one `decorator-arguments-changed` — not one
`decorator-added` plus one `decorator-removed`.

## Limitations

The following cases are explicitly NOT covered by Phase 19. Each will be
surfaced as a first-class `limitation:` rule-ID in Phase 25 (DOC-01):

- **`limitation: decorator-runtime-effect-opaque`** — Interlint detects
  decorator presence and argument changes, but does NOT verify what the
  decorator does at runtime. A decorator that silently changes a class's
  constructor behavior between versions will not surface as a finding
  beyond the presence / argument delta. This is accepted: static analysis
  cannot reason about arbitrary decorator factory bodies.
- **`limitation: jsdoc-decorator-annotations-not-scanned`** — Babel-compiled
  JavaScript sometimes emits decorators as JSDoc-style `@decorator`
  annotations in comments adjacent to the compiled class. Phase 19 does NOT
  scan these. If you ship babel-compiled output with decorator comments,
  Interlint will report no decorator findings against it. Use TypeScript
  compiled output (which emits real decorator AST nodes) or ship
  uncompiled TypeScript sources.
- **`limitation: parameter-decorators-not-diffed`** — Parameter decorators
  on non-constructor method parameters (e.g., `foo(@Inject('svc') svc: Service)`)
  are recognized by the AST extractor at the parameter node but are NOT
  surfaced in the per-method decorator diff because `ClassMember` tracks
  `param_count` rather than per-parameter detail. Constructor
  parameter-properties (parameters marked `public`/`private`/`protected`/`readonly`,
  which TypeScript synthesizes into class fields) ARE covered — their
  decorators appear on the synthesized `ClassMember` Property entry.
  Full parameter-level decorator diffing is tracked for Phase 21+.

These limitations are documented so users can see from the rule docs which
cases Interlint deliberately does not cover, rather than discovering silent
gaps in the field.
