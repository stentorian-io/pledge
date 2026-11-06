# Module loading and CommonJS named imports

## CommonJS named ESM imports

Major `cjs-named-export-removed` reports a named ESM import that Node previously
exposed from a CommonJS entry point but no longer recognizes. For example,
changing `exports.api = 1` to `module.exports = create()` can preserve
`require('package').api` while breaking `import { api } from 'package'`.
Node discovers these names through static patterns in its bundled parser;
equivalent JavaScript runtime objects can have different import surfaces.

Pledge asks the installed Node executable to parse package source without
executing that source. It uses Node's own import resolver and compares both
versions under the same default and combined declared custom condition contexts.
Require-only and types-only branches are excluded. Literal CommonJS re-exports
are followed through Node's require resolver within the prepared package.
Missing re-exports, cycles, unsupported parser interfaces, and resource limits
produce `analysis-incomplete`, rather than treating missing evidence as an empty
namespace. The check bounds entry points, condition contexts, source bytes,
module count, graph depth, names, subprocess output, and execution time.

The parser integration is tested on Node 22.12.0, 24.0.0, and 26.7.0. Results
describe the installed runtime; they do not establish named-import support on
every Node release allowed by `engines.node`. Default imports and Node's
unconditional `module.exports` namespace marker are excluded from this rule.
See [Node's CommonJS namespaces](https://nodejs.org/api/esm.html#commonjs-namespaces).

## Synchronous module loading

Major `module-became-async` detects a previously synchronous public module that
gains top-level `await`, directly or through a local static dependency. Node
rejects `require()` of such a module with `ERR_REQUIRE_ASYNC_MODULE`, even if
its exports are unchanged.

The check follows static imports, re-exports, and literal top-level `require`
calls. Await inside function bodies does not make the containing module async.
Top-level `await using` also makes a module asynchronous; ordinary `using` does
not. Local runtime resolution follows JavaScript files and package directory
entry points, rather than substituting adjacent TypeScript source files.
Import-only entry points and declaration-only files are excluded. Cycles are
bounded; unreadable or unresolved local modules emit `analysis-incomplete`.

Execution inside external packages, dynamically constructed imports, and
runtime calls to deferred functions are outside this packed-file graph check.
See [Node module loading](https://nodejs.org/api/packages.html#module-resolution-and-loading).

## Node syntax requirements

Adding `using` or `await using` to a reachable runtime module requires the Node
24 release line. This includes declarations inside function bodies, because an
older runtime must parse those bodies before the function can be called.
Major `node-syntax-requirement-raised` is emitted when the syntax is new and the
old and current `engines.node` ranges both still support older Node versions.
Import-only entry points and nested local module dependencies are included;
declaration files and TypeScript compiler companions are excluded.

If the release also raises `engines.node` to Node 24 or newer, the metadata rule
reports that requirement without an additional syntax finding. Unchanged use of
the syntax does not create a new finding. Missing or unparseable engine ranges
leave this newly introduced requirement as `analysis-incomplete` instead of
assuming a runtime support policy. Range unions are checked in full. This check
covers explicit resource-management syntax; it does not establish compatibility
for every JavaScript feature or runtime API.

The support boundary was verified against Node 22.12.0 and Node 24.0.0, whose
[release notes](https://nodejs.org/en/blog/release/v24.0.0) enable explicit resource
management. Prereleases in the Node 24 line are not classified as older release
lines solely because of semver ordering.
