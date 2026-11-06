# Distributed JSON and stylesheets

Pledge compares shipped JSON files, CSS APIs, and asset paths alongside package
exports. Removing an asset emits Major `asset-file-removed`.

## JSON fields and schema requirements

Removing a JSON object field or changing its JSON kind emits Major
`json-field-removed`. Adding an ordinary field emits Minor `json-field-added`.
The package's own `package.json` is handled by the package metadata rules.

A JSON Schema can make an existing property mandatory without changing its
definition:

```json
{"type":"object","properties":{"email":{"type":"string"}},"required":[]}
```

Changing `required` to `["email"]` rejects objects that previously omitted the
property. Pledge emits Major `json-required-field-added` for each newly required
name, including requirements in nested schema objects. The same rule applies
when a new property is added as required.

Requirement order does not matter. Removing a requirement, or the whole
`required` array, does not produce a breaking finding for that requirement.
Objects containing a `required` array are interpreted as schemas for this
check. Other array contents and scalar value changes are not compared; this
is not a complete JSON Schema compatibility validator.

Unreadable or malformed JSON in either snapshot emits Warning
`analysis-incomplete`. JSON comparison stops at nesting depth 20, retaining
the Patch `json-comparison-truncated` diagnostic and also emitting
`analysis-incomplete`. These cases make the CLI exit `3`; they cannot establish
compatibility. Findings from successfully compared assets remain available.

## CSS APIs

Removing a class selector emits Major `css-class-removed`; removing a custom
property declaration or `@property` registration emits Major
`css-custom-property-removed`. Identifier escapes are compared by their decoded
names, so `.api` and `.\61 pi` describe the same class. The `style`
or `css` manifest field selects stylesheets; without either field, all shipped
CSS files are scanned.

Equivalent relative path spellings such as `./style.css` and `style.css` do
not change the comparison. Retained stylesheet paths keep independent APIs.
When both snapshots declare stylesheet entry points, Pledge also compares APIs
from redirected entries against the new selection, even if the manifest points
to a different retained file. Without declared entry points, files are paired by
path. Unreadable selected files emit `analysis-incomplete`; they do not suppress
findings from other readable files.

Invalid UTF-8 or CSS that requires parser recovery emits `analysis-incomplete`;
recovered selectors cannot establish which rules a browser keeps. Unsupported
grammar, including identifier escapes the parser cannot represent, follows the
same policy. Empty required arguments of `:not()`, `:has()`, `:lang()`, `:dir()`,
`:state()`, `:host()`, `:host-context()`, the `nth-*` child/type selectors,
`::part()`, and `::slotted()` also produce incomplete analysis. Empty forgiving
lists in `:is()` and `:where()` remain valid syntax. These syntax checks do not
model browser-specific feature support.
Escaped letters in property names are normalized before parsing;
quoted strings and comments retain their original spelling. These comparisons
track declared names, not the cascade, property values, or registration
descriptor compatibility.

Manifest-selected stylesheets must resolve inside the packed package, including
through symlinks. Absolute paths and paths resolving outside it emit
`analysis-incomplete` without reading external contents. Deeply nested CSS rules
are traversed without recursive Rust calls, so nesting does not exhaust the
process stack.

## Packed file discovery

Packed dotfiles and files inside dot-prefixed directories participate in asset
comparison; only dependency and version-control metadata directories are
excluded. Internal symlink aliases keep their own public paths. Directory cycles, links outside the
package, broken links, unreadable paths, and the directory depth limit of 128
emit Warning `analysis-incomplete`, making the CLI exit `3`. These paths are not
followed; successfully scanned paths remain available for comparison.

Each scan also limits work to 100,000 directory entries, counting entries seen
through aliases again and including skipped names. Reaching this limit stops
the scan with `analysis-incomplete`; this bounds branching aliases even without
a directory cycle. Directories are visited in name order. If a whole directory
cannot be enumerated within the remaining budget, its contents are left
unscanned so partial results do not depend on filesystem enumeration order.
