# Synchronous module loading

Major `module-became-async` detects a previously synchronous public module that
gains top-level `await`, directly or through a local static dependency. Node
rejects `require()` of such a module with `ERR_REQUIRE_ASYNC_MODULE`, even if
its exports are unchanged.

The check follows static imports, re-exports, and literal top-level `require`
calls. Await inside function bodies does not make the containing module async.
Import-only entry points and declaration-only files are excluded. Cycles are
bounded; unreadable or unresolved local modules emit `analysis-incomplete`.

Execution inside external packages, dynamically constructed imports, and
runtime calls to deferred functions are outside this packed-file graph check.
See [Node module loading](https://nodejs.org/api/packages.html#module-resolution-and-loading).
