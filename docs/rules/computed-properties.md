# Computed properties and symbol keys

CommonJS computed exports and computed class members resolve string literals,
local `const` aliases, template strings, and standard symbol expressions without
running package code. CommonJS object literals with computed keys are included.

All standard well-known symbols are tracked, including the matching, replacement,
concatenation, and disposal protocols. Registered symbols use their constant
`Symbol.for(key)` registry key. A registered key change or removed protocol is
Major `export-removed`; class members use `class-method-removed`. Symbol exports
occupy a separate symbol space from string exports, so a string such as
`"@@iterator"` cannot conceal removal of `Symbol.iterator`.
Literal and inferred factory objects keep that separation even when a string
property is named `"[Symbol.iterator]"` or resembles a registered symbol key.

The identity of a registered symbol follows the
[ECMAScript Symbol.for algorithm](https://tc39.es/ecma262/multipage/fundamental-objects.html#sec-symbol.for).
It is not a private identity with unspecified behavior.

Compiler enrichment also retains registered and well-known symbol identities
when the symbol itself is exported as a value. Changing that identity is a
breaking value-type change. Object property keys use the resolved identity,
so renaming a local constant holding the same symbol stays compatible. A
registry key that cannot be resolved makes compiler enrichment incomplete.

Unknown keys, mutable bindings, or shadowed built-ins are not guessed. An
unresolved computed CommonJS key is an incomplete discovery warning; unresolved
class keys produce `analysis-incomplete` and the contextual
`limitation: dynamic-static-member-key` finding. Deferred function bodies and
writes on unrelated local objects are excluded from module export enumeration. This includes parameter, catch, destructuring, and
named function/class bindings that shadow `exports` or `module`. Direct
`exports.key`, `module.exports.key`, constant bracket keys, and object-literal
keys share that check. Immediately invoked functions (including `.call` and
`.apply`), static class initialization, and computed class keys are evaluated
at module initialization. An immediate wrapper may receive the public `exports`
object as its parameter, including through `.call` or a literal `.apply` argument
array. Writes after an async wrapper suspends with `await` produce incomplete
analysis because their availability depends on when the consumer reads them.
UMD factory recognition verifies eager invocation and
the public export argument; returned objects must flow to `module.exports`.
Escaped identifiers use their decoded names for these binding checks, including
public wrapper parameters and `Object` helper methods.

Whole-module assignments such as `module.exports = create()` retain compiler
evidence for their result without executing the call. Inferred plain objects
expose the same require members as literal objects, including a property named
`default`, optional reads, and accessor write contracts. Callable, constructed
class, and nominal results keep their aggregate type. Unknown results make
analysis incomplete; an explicitly declared `any` identifier keeps its declared
contract.

An inferred symbol index requires concrete registered keys from the exact
returned object literal. Dynamic registry keys, broader symbol indexes, and
symbol indexes introduced by spreads or accessors remain incomplete instead of
becoming an empty property list.

Object spreads forward properties, including a real property named `default`;
they do not forward the source module's callable or constructable value. Compiler
evidence describes the receiving object, so spreading a getter produces a
writable data property. Whole-module `require()` assignments retain the original
value contract. Bundler star-export annotations exclude `default`, matching their
re-export helpers.

The constant evaluator is deliberately bounded. Evaluating arbitrary calls,
proxies, and runtime inputs requires information beyond literal key folding;
these expressions must not be mistaken for an empty public surface.

## Proxies and reflective construction

Fresh empty proxy handlers and direct `Reflect` trap forwarding preserve the
target's property surface. Ordinary `Reflect.construct` calls retain the
constructed type. Arbitrary traps, mutable handler aliases, and custom
construction prototypes require runtime evidence. Unresolved dispatch emits
`limitation: proxy-dispatched-exports` alongside incomplete analysis.

These checks describe the property surface. They do not prove arbitrary method
behavior or private-brand checks through a proxy.

## CommonJS Object helpers

`Object.defineProperty`, `Object.defineProperties`, and `Object.assign` use the
same constant and symbol key identities as computed assignments. Renaming a
private constant key or changing a quoted key to an identifier stays compatible.
The helper must refer to the global `Object` and write to `exports` or
`module.exports`. A discarded copy such as `Object.assign({}, exports, {extra:1})`
does not export `extra`; assigning that result to `module.exports` does.
Uncalled function bodies, shadowed built-ins, and unrelated `module` properties
cannot supply module exports. Recognized UMD factories and immediately invoked
function expressions are module initialization contexts.

Changing a statically described CommonJS property from writable to nonwritable
reports Major `export-writability-removed`. Changing it from enumerable to
nonenumerable reports Major `export-enumerability-removed`: existing writes or
`Object.keys` consumers can fail even when the property's value type is unchanged.
For example, removing `writable: true` from a literal `defineProperty` descriptor
removes write support because omitted flags default to false. Adding either
capability is compatible. These checks compare literal descriptors on both sides;
unknown flags, repeated definitions, proxies, and later runtime mutations require
additional evidence and are not evaluated as descriptor contracts.
