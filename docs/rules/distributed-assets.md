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
property declaration emits Major `css-custom-property-removed`. The `style`
or `css` manifest field selects stylesheets; without either field, all shipped
CSS files are scanned.

Equivalent relative path spellings such as `./style.css` and `style.css` do
not change the comparison. When both snapshots declare stylesheet entry points,
Pledge compares their combined CSS API even if the manifest redirects to a
different retained file. Without declared entry points, files are paired by path.

## Packed file discovery

Packed asset and deep-import scans skip dotfiles and `node_modules`. Internal
symlink aliases keep their own public paths. Directory cycles, links outside the
package, broken links, unreadable paths, and the directory depth limit of 128
emit Warning `analysis-incomplete`, making the CLI exit `3`. These paths are not
followed; successfully scanned paths remain available for comparison.
