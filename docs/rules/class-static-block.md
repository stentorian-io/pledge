# Class static initialization

Class static blocks execute when the class is defined. Assignments to the class
name or `this` contribute to its public static surface, including assignments
inside nested unconditional blocks. Constant conditions select their executed
branch. Deferred function bodies and shadowed local class names are excluded.

See the [TypeScript class static block documentation](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-4-4.html#static-blocks-in-classes).

## Surface and type comparisons

Discovery synthesizes static properties from direct class assignments. Removing
one produces Major `class-method-removed`, using the `static property` noun.
Literal, constant, template, well-known symbol, and registered symbol keys are
resolved as described in [computed properties](computed-properties.md).

Compiler enrichment additionally resolves the assigned value types and nested
property paths, such as `this.options.mode = value`. It follows initialization
order across blocks and field initializers: later writes replace earlier writes,
and a later field initializer replaces an earlier block assignment. Class-body
annotations remain the declared TypeScript contract.

The class's resolved contract includes its instance members, static members,
and constructor signatures. Changes to static property types, static method
parameters, inherited static members, and constructor parameter types are
compared structurally. Adding a static property or optional constructor parameter,
or widening a static method's accepted input, stays compatible.

## Incomplete initialization evidence

An unresolved computed key or runtime condition controlling a static assignment
cannot be treated as an absent property. Source comparisons produce
`analysis-incomplete`, Minor `class-static-type-changed`, and Info
`limitation: dynamic-static-member-key` for that unresolved surface. The legacy
limitation ID covers both the name and presence of such members.

Compiler enrichment also reports incomplete evidence for unresolved property
paths, runtime-controlled assignments, and calls that can change the class
object during initialization. These are explicit bounds of the current
initialization analysis, not claims that every such expression is impossible to
analyze. Arbitrary calls can depend on runtime inputs and side effects that the
package's declarations do not specify.

Source-only comparisons detect member presence and modifiers. Assigned value
and constructor types require compiler evidence; a clean source-only comparison
does not establish type compatibility.
