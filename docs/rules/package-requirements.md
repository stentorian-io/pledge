# Package requirements

`peer-dep-version-bumped` and `engine-version-bumped` are Major when a new npm
range excludes any previously accepted versions. This includes raising the
minimum, lowering the maximum, removing a union branch, and switching between
disjoint ranges. Reordering branches and widening support are compatible.
The historical rule names remain stable. Unparseable changed ranges are
compared conservatively.

`platform-narrowed` is Major when `os` or `cpu` rejects a previously accepted
platform. Positive entries form an allowlist and `!` entries exclude platforms.
Removing the field or replacing it with an empty list restores unrestricted
support and is compatible.
