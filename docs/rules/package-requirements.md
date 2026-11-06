# Package requirements

`peer-dep-version-bumped` and `engine-version-bumped` are Major when a new npm
range excludes any previously accepted versions. This includes raising the
minimum, lowering the maximum, removing a union branch, and switching between
disjoint ranges. Peer dependency ranges include prereleases only when the range
explicitly admits that prerelease core; widening stable bounds can still drop
prereleases. Engine ranges follow npm's `includePrerelease` behavior instead:
changing `<20.0.0` to `<20.0.0-0` drops Node 20 release candidates. Reordering
branches and widening support are compatible.
The historical rule names remain stable. Unparseable changed ranges are
compared conservatively.

`platform-narrowed` is Major when `os` or `cpu` rejects a previously accepted
platform. Positive entries form an allowlist and `!` entries exclude platforms.
Removing the field or replacing it with an empty list restores unrestricted
support and is compatible.
The single entry `["any"]` also accepts every platform. In a mixed list such as
`["any", "linux"]`, npm treats `any` as a literal name; that list rejects macOS.

String `bin` entries use the unscoped package name as their command. For example,
`@scope/tool` with `"bin":"./cli.js"` is equivalent to
`"bin":{"tool":"./cli.js"}` and does not remove a command.
