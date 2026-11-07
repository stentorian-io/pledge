# Review Test262 feature coverage

Use the Test262 row in the [language inventory](language-coverage.md) to locate
package-interface evidence for a feature flag. The row accounts for the 199
names in the pinned `features.txt`; it does not report Test262 test pass rates
or certify an ECMAScript engine. A proposal flag retains the status assigned by
the pinned taxonomy, regardless of whether another source calls it standardized.

## Package-interface evidence

Consumer tests compare old and new packages with the same Node consumer and
check Pledge's findings independently. Focused controls cover direct lexical
bindings, function constructibility, class public/private fields and methods,
static members, computed and symbol property names, destructured exports,
namespace forwarding, generator protocols, and required parameter counts.
Public string-name controls include raw and escaped U+180E, U+2028, and U+2029.
Default and rest parameter evidence uses compiler enrichment; the structural
JavaScript parser alone is not credited with compiler-inferred signatures.

JavaScript class fields use a different tree-sitter production from TypeScript
fields. Direct JavaScript field tests check instance/static and public/private
identities, visibility changes, and compatible private renames. Both productions
now feed the same class comparison rules.

A partial flag credits only its named interface operations. It does not certify
all grammar alternatives, early errors, execution scheduling, private brands,
iterator closing, decorator effects, or arbitrary changes inside function bodies.
The machine-readable assessments state the limits for each credited flag.

Computed class keys using BigInt, coalescing, logical assignment, optional
chaining, or destructuring assignment currently retain explicit incomplete
structural evidence. Node consumers establish the resulting public member
name; parsing the operator does not establish that Pledge recovered that name.

## Unsupported public surfaces

The bundled JavaScript grammar cannot extract proposal auto-accessors, source
phase imports, deferred namespace imports, or deferred namespace re-exports.
Tests assert an empty discovered surface and an incomplete warning, so a
successful-looking compatibility result cannot be inferred from these inputs.

Text, bytes, JSON, and WebAssembly module interfaces are also not modeled as
ordinary JavaScript export maps. Tests retain incomplete evidence when these
interfaces are publicly forwarded. Host execution support differs by Node
version and experimental flags; that support does not establish Pledge support.
See [module loading](rules/module-loading.md) for the distinct static loading
checks.

## Engine semantics outside this row

Most Test262 flags select execution tests for built-in algorithms, including
numeric coercion, binary memory, collection iteration, regular expressions,
promise scheduling, internationalization, and garbage collection. Comparing
those algorithms between engines or proving equivalent results for arbitrary
package function bodies is outside Pledge's package-interface comparison.
Each such exclusion records the particular semantic family in the inventory.

This boundary does not exclude a package's public export of a built-in value,
its inferred or declared type, a computed public property, a callable signature,
or a loading requirement. Those remain Pledge interface responsibilities.
Removing a `Map` export is covered by export comparison; executing `Map`'s
SameValueZero, iterator, and abrupt-completion conformance tests would not
establish that coverage. Merely exporting and removing an arbitrary built-in
value is therefore not counted as feature-specific support.

Harness capabilities (`IsHTMLDDA` and required host GC) describe Test262's host
integration. The special ModuleSource flag also selects public source-binding
re-export tests, so its public interface is classified as unsupported with
incomplete evidence, alongside source-phase import grammar.
