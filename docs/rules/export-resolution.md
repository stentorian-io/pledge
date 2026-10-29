# Public export resolution

Export identity includes the public subpath and the complete condition path.
Two subpaths can expose the same file without losing either public API. A
re-export cycle stops the current traversal; it does not prevent another entry
point from using the same module.

Named aliases retain both their public name and the original declaration name
and position. Star exports exclude `default`, preserve type-only boundaries,
and defer to explicit exports. Conflicting star exports of different bindings
are ambiguous and do not create a public named export. Namespace re-exports
retain their namespace prefix, including its `default` member.

Conditional objects retain JSON order. A `default` branch hides later siblings.
`condition-priority-changed` is Major when added or reordered conditions select an
incompatible API, and Minor when the selected API remains compatible.
Reordering mutually exclusive `import` and `require` branches is compatible.
The priority pass is bounded; exceeding its branch limit emits
`analysis-incomplete` and the CLI exits 3.
Wildcard targets include nested paths, and more specific mappings or `null`
exclusions override broader patterns. Export arrays select the first valid
target; a missing target file does not activate a later array element.

JavaScript entry points also discover adjacent TypeScript source or declaration
files using extension substitution, including `.d.mts` and `.d.cts`. Packages
without an `exports` field expose source files through legacy deep imports.
Hashed chunk names remain physical paths for reading and type requests;
normalization is restricted to directive comparison identity.

Changing a non-null target path emits Minor `condition-target-changed`; the
resolved public API determines whether that move is breaking. Removing a target
still emits Major. Raw private `imports` removals, reroutes, pattern changes,
and condition changes are Info findings. Their effects on public re-exports
are compared separately. An unused private map entry is not a public contract.

Unresolvable entry points and source processing limits produce incomplete
analysis warnings. See [Node package resolution](https://nodejs.org/api/packages.html)
and [TypeScript extension substitution](https://www.typescriptlang.org/docs/handbook/modules/reference.html#file-extension-substitution).

A types/runtime format mismatch is reported only when newly introduced. An
unchanged mismatch in both versions is not a breaking change between them.

CommonJS entry points forward literal `require()` targets, including bound
require aliases and object spreads. `process.env` equality/inequality branches
retain separate conditions; compatible rewrites of the same predicate preserve
identity. Installed dependencies and generated dot-prefixed package directories
are resolved through `node_modules`. Missing generated output is incomplete
evidence; Pledge does not execute package generators or lifecycle scripts.

TypeScript `export =` declarations expose their assigned binding as the default
API. Compiler enrichment follows that binding, including functions merged with
namespaces, so their call signatures and attached properties remain available
for compatibility checks.

## Release tags

TSDoc tags (`@deprecated`, `@internal`, `@alpha`, `@beta`, and `@public`) are
recognized on default exports, named and star re-exports, destructured pairs,
compound-assignment members, and bundler CommonJS output. Deprecation produces
an `export-deprecated` warning. Internal, alpha, and beta exports are excluded
from public API comparison.
