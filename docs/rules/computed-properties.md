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
