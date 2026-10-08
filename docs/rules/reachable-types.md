# Reachable public types

A public signature includes the declarations it references, even when those
declarations are private. The type pass follows local and imported aliases,
generic arguments, merged interfaces, and inherited public members. Existing
parameter, return, and field compatibility rules compare the expanded shapes.
Renaming a private alias with an unchanged shape is compatible.

Declaration identity is scoped to its source file. Generic parameters shadow
local declarations. Recursive references retain a recursion marker; depth and
work limits produce incomplete analysis warnings. Unresolved imported types
also degrade analysis after language-server enrichment.

The pass resolves declared structural types. It does not implement the full
TypeScript compiler: arbitrary conditional and mapped type evaluation,
inferred mixin inheritance, and all compiler-option combinations remain limits.
The language server supplies inferred signatures where available. Private and
protected class members are excluded from the expanded public object shape;
separate class rules handle visibility and nominal-brand changes.
