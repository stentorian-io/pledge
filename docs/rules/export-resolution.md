# Public export resolution

Export identity includes the public subpath and the complete condition path.
Two subpaths can expose the same file without losing either public API. A
re-export cycle stops the current traversal; it does not prevent another entry
point from using the same module.

Conditional objects retain JSON order. A `default` branch hides later siblings.
Wildcard targets include nested paths, and more specific mappings or `null`
exclusions override broader patterns. Export arrays select the first valid
target; a missing target file does not activate a later array element.

JavaScript entry points also discover adjacent TypeScript source or declaration
files using extension substitution, including `.d.mts` and `.d.cts`. Packages
without an `exports` field expose source files through legacy deep imports.
Hashed chunk names remain physical paths for reading and type requests;
normalization is restricted to directive comparison identity.

Unresolvable entry points and source processing limits produce incomplete
analysis warnings. See [Node package resolution](https://nodejs.org/api/packages.html)
and [TypeScript extension substitution](https://www.typescriptlang.org/docs/handbook/modules/reference.html#file-extension-substitution).
