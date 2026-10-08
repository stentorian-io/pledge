# Reachable public types

A public signature includes the declarations it references, even when those
declarations are private. The type pass follows local and imported aliases,
generic arguments, merged interfaces, and inherited public members. Existing
parameter, return, and field compatibility rules compare the expanded shapes.
Renaming a private alias with an unchanged shape is compatible.

Declaration identity is scoped to its source file. Generic parameters shadow
local declarations. Recursive references retain a recursion marker; depth and
work limits produce incomplete analysis warnings. Expansion is limited to 32
levels, 4,096 steps per symbol, and 200,000 steps per package. Symbols are
processed in a stable order so budget exhaustion is reproducible. Unresolved imported types
also degrade analysis after language-server enrichment.

The pass resolves declared structural types. It does not implement the full
TypeScript compiler: arbitrary conditional and mapped type evaluation,
inferred mixin inheritance, and all compiler-option combinations remain limits.
The language server supplies inferred signatures where available. Private and
protected class members are excluded from the expanded public object shape;
separate class rules handle visibility and nominal-brand changes.

Changed opaque types and comparison-depth exhaustion emit `analysis-incomplete`
warnings. Unsupported syntax cannot establish a breaking change or compatibility;
the CLI returns exit 3 when this evidence is incomplete.
