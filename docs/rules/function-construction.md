# Function construction and class inheritance

Replacing `export const Api = function () {}` with `export const Api = () => {}`
breaks consumers calling `new Api()`, although ordinary calls remain possible.
Pledge reports Major `function-constructor-removed` when a public ordinary
function becomes an arrow, async function, or generator. Renaming the function's
private expression name preserves construction support. The check describes the
function's built-in construction capability; it does not execute its body or
infer custom proxy behavior.

Changing the base of an exported JavaScript or TypeScript class reports Major
`class-inheritance-changed`. Consumers can depend on its prototype chain and
`instanceof` behavior even when both bases have identical members. A local
immutable alias of the same base preserves that identity and stays compatible.
The alias check follows constant identifier bindings; mutable bases, arbitrary
expressions, and cross-module alias identity require additional evidence.
