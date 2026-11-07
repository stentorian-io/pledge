# Checking ECMAScript package interfaces

Pledge compares the public names, callable and class shapes, property capabilities,
and loading requirements of packaged JavaScript. For example, changing an
exported ordinary function into an arrow function removes construction with
`new`; changing a CommonJS property descriptor from writable to read-only removes
a consumer's ability to assign that property.

The [coverage inventory](language-coverage.md) records empirical evidence against
the pinned ECMAScript source. A reference marked partial establishes only the
listed package consumer scenarios. It does not certify every production, early
error, abstract operation, or execution step in that reference.

## Public bindings and shapes

Independent Node consumers exercise export names and aliases, star ambiguity,
namespace forwarding, destructured bindings, async and generator declaration
identity, construction, accessors, symbol keys, and selected class initialization.
The tests compare the same consumer against both package versions and include
compatible rewrites. Compiler enrichment supplies additional inferred shapes;
its selected compiler and library versions remain part of the evidence.

Lexical consumers exercise BOM, NBSP and EM SPACE separators, comments, hashbangs,
semicolon insertion, escaped identifiers, and selected Unicode identifier forms.
These cases check whether the same public binding survives a spelling change.
They do not establish all Unicode categories, malformed syntax, or all lexical
goal choices.

The bundled grammar can parse `export` followed by a line terminator as a bare
identifier expression, silently dropping the declaration from its export tree.
Pledge rejects that tree and reports incomplete analysis. Regression consumers
cover LF, CRLF, U+2028 and U+2029 after `export`; a standalone CR is an accepted
control. Node acceptance of the other spellings does not establish Pledge
support. Unicode separators inside string and
template contents are separate, valid controls.

Non-strict CommonJS consumers also exercise HTML comment forms and selected
legacy declaration contexts. Ordinary callable aliases preserve their function
identity; ambiguous conditional or legacy bindings retain incomplete evidence.

Literal CommonJS `Object.assign` and `Object.defineProperties` consumers exercise
the direct public mutation target, member identity, writability and enumerability.
Immutable aliases of a CommonJS helper target retain incomplete evidence because
the assigned object may become detached from the public module object.
Arbitrary getters, repeated descriptor mutation and opaque computed keys require
separate evidence. Proxy dispatch, unknown symbol keys, uncertain branch effects
and suspension can also retain incomplete evidence; that is distinct from a
confirmed breaking change.

Reflective public mutation consumers also exercise sealing/extensibility,
prototype changes, legacy accessors, Reflect writes/deletes and opaque factory or
bound-function results. These paths retain runtime evidence limits instead of
silently treating their public surface as unchanged. Fresh frozen result objects
and transparent revocable proxies have independent breaking/compatible compiler
shape controls. See [reflective mutations and dynamic code](rules/computed-properties.md#reflective-mutations-and-dynamic-code)
for the direct-pattern boundaries and preserved known exports.

## Execution semantics outside the comparison

Pledge does not implement an ECMAScript engine or compare arbitrary function-body
behavior. The inventory therefore excludes specific built-in execution algorithms
such as array sorting, regular expression matching, numeric arithmetic, date
calculation, collection iteration internals, buffer memory access, garbage
collection and shared-memory ordering. Changing an exported function's algorithm
can break an application while preserving the interface that Pledge checks.

These exclusions concern those execution algorithms. They do not exclude the
public export, inferred return type, symbol-key identity or class shape of a
package that uses the corresponding built-in. Built-in object helpers that
construct or mutate a public interface are assessed separately.

Specification notation, explanatory history, bibliographies and engine
conformance requirements are also outside package comparison. Browser-only
legacy global and HTMLDDA behavior is outside the supported Node package host.
Lexical or declaration syntax used by packages is not excluded merely because
the specification also discusses it in a browser annex.

## Reading the reviewed boundaries

A reviewed disposition closes the reference audit, not the complete semantics of
an ECMAScript clause. Partial entries identify their concrete public consumers
and the remaining grammar/runtime alternatives. Unsupported entries reproduce a
specific public interface obligation and its incomplete-result path. Exclusions
identify engine execution, general program validity or document organization
outside package comparison.

Tree-sitter's successful parse does not establish full ECMAScript validity.
Complete duplicate-binding, strict-mode, temporal-dead-zone and invalid-program
error checking belongs to the engine validation scope. Selected parser/compiler
failures still produce incomplete evidence, including the valid declaration
spelling gap described above.

Module record state machines, realms, agent jobs and complete scheduling are
engine implementation responsibilities. Static exported-name resolution and the
Node synchronous-loading requirement of a top-level-await graph have separate
consumer evidence. Neither establishes every module cycle, host hook or
initialization error ordering.

Direct public mutation limits are also bounded. They recognize tested direct
CommonJS targets/results, uncertain initialization control flow and literal
`eval` assignments. They do not implement general dataflow or recursively execute
dynamic source. Untested alias, nested result and runtime effect combinations do
not acquire a compatibility guarantee from this audit.
