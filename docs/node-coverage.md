# Node module coverage

Use the export-resolution and module-loading checks to compare public package
entry points under the installed Node runtime. The coverage inventory pins the
Node 26.7 module documentation separately from the tested Node 22.12, 24.0, and
26.7 execution profiles. A partial assessment covers the concrete consumers
linked there; it does not certify the complete upstream section.

Public root and subpath exports, local private-import forwarding, conditional
selection, package-name self references, and CommonJS named ESM imports have
breaking and compatible consumer examples. Synchronous loading examples cover
top-level await in the entry point and local static dependencies, with deferred
async function bodies as compatible controls. Missing relative imports,
extensionless ESM imports, and directory imports retain incomplete loading
evidence instead of assuming an empty graph.

Public re-exports from `node:`, `data:`, and absolute `file:` URLs do not have
modeled source interfaces. They retain incomplete discovery evidence, including
named aliases and star exports. Other URL schemes receive the same treatment;
Node itself needs custom loading support for HTTP URLs. Static non-builtin URL
imports also retain incomplete module-loading evidence. These limits concern
public extraction and loading, even when Node can successfully execute the
package.

JSON, experimental text modules, and WebAssembly interfaces are not modeled as
JavaScript source interfaces. Following their public re-exports retains
incomplete discovery evidence. Tests exercise attributed JSON/text forwarding
and Wasm instance/source imports; runtime support differs between the declared
Node profiles. This does not validate Wasm string builtins, linking, reserved
names, cache identity, or arbitrary binary exports.

The comparison does not prove function-body equivalence for APIs computed from
`import.meta`, filesystem paths, dynamic imports, builtins, or module cache
state. Removing an ordinary exported function still participates in export
comparison; changing what its body computes is outside that contract. Likewise,
platform addon execution, `NODE_PATH`, require cache/extension hooks, and custom
loaders are application execution policies outside the packed-file graph.

Experimental package maps are consumer startup configuration, not a field in
the compared packed manifest. Pledge does not accept a package-map file, select
package IDs, or compare application dependency-instance identity. Excluding
those configuration sections does not exclude ordinary package exports,
imports, or dependency resolution from public interface checks.
