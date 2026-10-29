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

## CSS APIs

Removing a class selector emits Major `css-class-removed`; removing a custom
property declaration emits Major `css-custom-property-removed`. The `style`
or `css` manifest field selects stylesheets; without either field, all shipped
CSS files are scanned.
