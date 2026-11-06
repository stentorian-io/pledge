# Reachable public types

A public signature includes the declarations it references, even when those
declarations are private. The type pass follows local and imported aliases,
generic arguments, merged interfaces, and inherited public members. Existing
parameter, return, and field compatibility rules compare the expanded shapes.
Renaming a private alias with an unchanged shape is compatible.

Iterator and generator contracts include the values accepted by `.next()`.
Narrowing those inputs on a returned iterator is breaking; widening them is
compatible. Yielded and returned values retain output compatibility.

A class's `implements` clause checks its implementation against an interface; it
does not add members to the class's public type. Removing only that clause stays
compatible and does not emit `class-implements-removed`. Removing or changing an
actual public member still participates in the ordinary structural comparisons.

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
readonly property's read type remains compatible. Mutable arrays, tuples,
index signatures, maps, and sets preserve their element, key, and value write
domains too, including containers nested inside promises or readonly properties.
A returned callable retains its minimum and maximum argument counts; removing
an optional argument or replacing an unbounded rest parameter with a fixed
parameter can emit Major `param-type-narrowed`. Callback assignability still
allows ignored arguments and discarded results for `void` callbacks.
Concrete tuple rest parameters are compared as ordinary argument positions.
Builtin relationships include `Map` to `ReadonlyMap`, `Set` to `ReadonlySet`, and
`Promise` to `PromiseLike`; the reverse direction can require missing members.

Every overload must retain compatible inputs and results, including signatures
after the first declaration. An overload whose accepted inputs narrow or whose
result becomes incompatible emits Major `overload-removed`. Widening accepted
inputs or narrowing a readable result remains compatible. A single replacement
may cover multiple old overloads. Reordering overlapping overloads is breaking
when it selects an incompatible result for an old call; disjoint overload
reordering stays compatible. Overlap comparison checks at most 256 argument-type
witnesses; exhausting that budget produces `analysis-incomplete`.

Callable shapes retain explicit `this` receivers and type predicates, including
returned callbacks and object methods. Adding or narrowing a receiver requirement
can break existing calls and emits Major `this-param-changed`; removing the
requirement is compatible. Predicates must preserve narrowing in both their
true and false branches, while an assertion must preserve the type guaranteed
after it returns. Top-level changes emit `type-guard-widened` or
`assertion-widened`; nested changes use the enclosing signature's type rules.

Compiler evaluation loads the package's tsconfig and retains strict nullability
and exact optional properties for public contracts. An omitted optional property
is distinct from a present property whose value is `undefined`; removing an
explicit `| undefined` can break consumers using `exactOptionalPropertyTypes`.
Packages without a tsconfig receive strict NodeNext
resolution, including dependencies that publish types only through `exports`.
It does not execute package code. Unevaluated symbolic
types (including uninstantiated mapped types), missing declarations, compiler
failures, or exhausted budgets produce incomplete analysis. The compiler pass
has a 30-second timeout, a 512 MiB heap limit, and a 16 MiB output limit in
addition to the type expansion limits above. Compiler syntax diagnostics, including
syntax unsupported by the selected compiler version, also make analysis incomplete.
Private class members are excluded from the public structural shape, while their
declaration identities and protected member identities are retained as nominal
brands. Unique-symbol declarations retain identity in the same way. Public input
and output relationships are compared across the package: if `consume(api())`
used to share a brand and no longer does, Major `type-incompatible` is emitted.
Consistent private declaration renames and inherited brands preserve these
relationships. Input unions retain each accepted brand regardless of member order;
protected overrides retain their inherited origin. Non-public release tags do not
participate in these relationships. Ambiguous branded overload correspondence and
nominal output unions that cannot be aligned produce incomplete analysis. The
nominal comparison has a 100,000-pair budget; exhaustion produces incomplete
analysis.

Changed opaque types and comparison-depth exhaustion emit `analysis-incomplete`
warnings. Unsupported syntax cannot establish a breaking change or compatibility;
the CLI returns exit 3 when this evidence is incomplete.
