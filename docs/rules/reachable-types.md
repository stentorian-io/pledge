# Reachable public types

A public signature includes the declarations it references, even when those
declarations are private. The type pass follows local and imported aliases,
generic arguments, merged interfaces, and inherited public members. Existing
parameter, return, and field compatibility rules compare the expanded shapes.
Renaming a private alias with an unchanged shape is compatible.

Iterator and generator contracts include the values accepted by `.next()`.
Narrowing those inputs on a returned iterator is breaking; widening them is
compatible. Yielded and returned values retain output compatibility.

Declaration identity is scoped to its source file. Generic parameters shadow
local declarations. Recursive references retain a recursion marker; depth and
work limits produce incomplete analysis warnings. Expansion is limited to 32
levels, 4,096 steps per symbol, and 200,000 steps per package. Symbols are
processed in a stable order so budget exhaustion is reproducible. Unresolved imported types
also degrade analysis after compiler enrichment.
When compiler evaluation completes, it supersedes warnings about the source
type expansion limit. Other discovery failures remain incomplete. Library
callers can apply `EnrichResult::reconcile_discovery_warnings` to retain only
the outstanding warnings.

Without enrichment, the source pass resolves declared structural types. With
enrichment, the JavaScript TypeScript compiler evaluates the public
contract, including concrete mapped and conditional types, key remapping,
utility types, inherited fields, and JSDoc typedefs. Equivalent evaluated shapes
remain compatible even when their alias expressions change. Making a returned
property readonly is breaking, including properties inside nested objects.
Writable properties also retain their accepted assignment types: changing a
returned or exported `value: string | number` to `value: string` emits Major
`field-type-changed`, because assigning `42` no longer compiles. Narrowing a
readonly property's read type remains compatible.

Every overload must retain compatible inputs and results, including signatures
after the first declaration. An overload whose accepted inputs narrow or whose
result becomes incompatible emits Major `overload-removed`. Widening accepted
inputs or narrowing a readable result remains compatible.

Compiler evaluation loads the package's tsconfig and retains strict nullability
for public contracts. Packages without a tsconfig receive strict NodeNext
resolution, including dependencies that publish types only through `exports`.
It does not execute package code. Unevaluated symbolic
types (including uninstantiated mapped types), missing declarations, compiler
failures, or exhausted budgets produce incomplete analysis. The compiler pass
has a 30-second timeout, a 512 MiB heap limit, and a 16 MiB output limit in
addition to the type expansion limits above. Private and
protected class members are excluded from the expanded public object shape;
separate class rules handle visibility and nominal-brand changes.

Changed opaque types and comparison-depth exhaustion emit `analysis-incomplete`
warnings. Unsupported syntax cannot establish a breaking change or compatibility;
the CLI returns exit 3 when this evidence is incomplete.
