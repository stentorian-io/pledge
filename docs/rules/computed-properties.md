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
writes on unrelated local objects are excluded from module export enumeration.

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
