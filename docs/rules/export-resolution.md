# Public export resolution

TypeScript path aliases select exact mappings before wildcard mappings. Wildcard
keys match both their prefix and suffix; the longest matching prefix wins, with
declaration order breaking ties. Only that mapping's replacement list supplies
fallbacks. Relative imports do not consult aliases; selected bare aliases take
precedence over unrelated sibling files. For TypeScript sources, aliases also
precede package `imports` resolution of `#` specifiers; runtime JavaScript keeps
its package-imports target. Targets use the same source, declaration, and
directory probing as other TypeScript imports. This follows the
[TypeScript path-mapping rules](https://www.typescriptlang.org/docs/handbook/modules/reference.html#paths)
and keeps discovery from selecting a different public barrel across runs.
Unmapped package names do not fall back to sibling files. An unresolved external
re-export retains an incomplete-analysis warning instead of borrowing evidence
from a similarly named local module.

For Rust callers, `TsconfigPaths.paths` is an insertion-ordered `IndexMap`, and
`resolve_tsconfig_alias` returns mapped targets without extension probing. Node
discovery applies that probing after alias selection.

Export identity includes the public subpath and the complete condition path.
Two subpaths can expose the same file without losing either public API. A
re-export cycle stops the current traversal; it does not prevent another entry
point from using the same module.
Type and value exports with the same name retain separate declarations and type
evidence. Removing a type alias does not remove a surviving value of that name;
reordering those declarations preserves both contracts.
Type-only forwarding also preserves a same-named value's `typeof` contract
without making that value available to runtime imports.

Named aliases retain both their public name and the original declaration name
and position. Reserved-word aliases have the same identity as their quoted
spelling, including named and namespace re-exports. Explicit aliases to
`default` remain public exports. Star exports exclude `default`, preserve type-only boundaries,
and defer to explicit exports. Conflicting star exports of different bindings
are ambiguous and do not create a public named export. Namespace re-exports
retain their namespace prefix, including its `default` member.
Renaming a dependency's default export exposes only the chosen alias unless
`default` is also explicitly forwarded.
Importing a namespace and then exporting it through a local alias or default
export preserves the same member identities as a direct namespace re-export.
Type-only namespace imports and exports retain their type-only boundary.
Named and default imports re-exported through local bindings resolve to their
source declarations, retaining type/value spaces, class members, and callable
contracts. Type-only import and export modifiers restrict that forwarded surface.

Conditional objects retain JSON order. A `default` branch hides later siblings.
`condition-priority-changed` is Major when added or reordered conditions select an
incompatible API, and Minor when the selected API remains compatible.
Reordering mutually exclusive `import` and `require` branches is compatible.
The priority pass is bounded; exceeding its branch limit emits
`analysis-incomplete` and the CLI exits 3.
Wildcard targets include nested paths and packed dotfiles. Every `*` in a target
is replaced with the same captured subpath, including targets such as
`./*/index-*.js`. More specific mappings or `null`
exclusions override broader patterns. Export arrays select the first valid
target for each condition set, skipping unmatched conditions and null entries;
a missing target file does not activate a later array element. Array discovery
checks up to 128 condition combinations; larger maps produce incomplete analysis.

Public modules remain contracts even when they export no names. Deleting a
wildcard-selected polyfill emits Major `subpath-removed`; restricting an entry
from a bare target to an import-only condition emits Major
`condition-priority-changed` when `require()` loses access. Removing an already
null subpath does not remove an API. A top-level `"exports":null` has a different
meaning: Node uses legacy `main` and deep-import resolution. Removing `exports`
also preserves any root or subpath still reachable through that legacy fallback.
Runtime access is checked separately from declaration resolution: a remaining
`.d.ts` file cannot replace a removed JavaScript module. Deep ESM imports require
an exact file path even when CommonJS can still find an extensionless subpath.

Export targets are resolved as URLs, including percent-encoded filenames.
An encoded `%2A` remains a literal asterisk when a target also contains wildcard slots.
Mixed subpath/condition objects and integer condition keys are invalid Node
configurations and produce incomplete analysis rather than a clean verdict.

JavaScript entry points also discover adjacent TypeScript source or declaration
files using extension substitution, including `.d.mts` and `.d.cts`. Packages
without an `exports` field expose source files through legacy deep imports.
An entry file retains its deep-import path as well as its root entry identity.
Rerouting `main` between compatible files does not remove either unchanged
deep-import surface.
Their root entry follows Node's extension and directory-index resolution for
`main`, including the root `index.js` fallback when `main` is absent or missing.
Declaring a separate types entry does not hide that runtime root.
Hashed chunk names remain physical paths for reading and type requests;
normalization is restricted to directive comparison identity.
`typesVersions` declarations retain their version-range and public-subpath
identity, so a later version branch or another mapped subpath cannot replace
the declaration being compared.

Changing a non-null target path emits Minor `condition-target-changed`; the
resolved public API determines whether that move is breaking. Removing a target
still emits Major. Raw private `imports` removals, reroutes, pattern changes,
and condition changes are Info findings. Their effects on public re-exports
are compared separately. An unused private map entry is not a public contract.

Invalid export targets, including missing `./` prefixes and traversal outside
the package, produce incomplete analysis. Invalid array alternatives are skipped
when a later target resolves. Unresolvable entry points and source processing limits produce incomplete
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

Unconditional CommonJS assignments replace earlier exported surfaces. Rebinding
a `require()` alias affects later forwarding through that alias, while an object
already assigned to `module.exports` retains its identity. Property deletion,
runtime-controlled reassignment, and replacements whose effects are unresolved
produce incomplete-analysis warnings; compiler enrichment does not clear those
runtime mutation warnings. A possible alias reassignment makes later forwarding
incomplete without changing an object that was already forwarded. Writes through
`exports` after replacing `module.exports` also remain incomplete because the two
objects may have detached; an unconditional `exports = module.exports` reattaches
the alias for subsequent writes.

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

Direct `var` declarations (including destructuring), ordinary and async generator
declarations, and string-named aliases and namespaces participate in the same
export identity checks. Export names use their decoded JavaScript identity:
`value` and `\u0076alue` name the same binding, while a string export containing a
literal backslash retains that character. The local declaration name is retained
separately from a public alias for structural checks.
CommonJS references to unambiguous top-level function declarations or initialized
callable variables retain that callable identity. Moving the function inline
does not change its declaration kind; replacing a constructible function with
an arrow removes construction support. Conditional, block, labelled, legacy
for-in, conflicting, or reassigned callable bindings retain incomplete discovery
evidence when their exported identity cannot be established safely.
Destructured declarations include bindings with default initializers and nested
rest patterns. Names referenced only by an initializer are not public exports.

Valid re-exports with import attributes preserve their source specifier and public
names. The bundled grammars' missing re-export attribute production is validated
through the corresponding import production before extraction. Other parser
recovery errors produce incomplete discovery instead of silently assuming an
empty interface. This includes invalid exported `using` declarations. Following
an attributed re-export into a source type the resolver cannot model, such as a
JSON module, still reports explicit incomplete evidence; supporting its syntax
does not imply that every target format is supported.

Known grammar gaps for TypeScript variance modifiers, anonymous default function
signatures in declaration files, and reserved-word JavaScript export aliases are
normalized without changing public names or source positions. Variance direction,
parameter constraints, defaults, and declaration signature types remain available
to their existing comparisons. Malformed nearby syntax still makes analysis
incomplete.

The bundled JavaScript grammar can misparse `export` followed by LF, CRLF,
U+2028 or U+2029 as an identifier expression. Pledge rejects that tree and retains
incomplete discovery evidence instead of assuming that the module exports no
names. A standalone CR after `export` is covered as an accepted control.
