# Automatic package and baseline selection

The Pledge action discovers the repository's publishable packages and compares each
built local package with the highest published version in its release line.
One action step after the normal build covers a single package or a
workspace repository without a package matrix.

This reference defines the [GitHub Action](github-action.md). The standalone
CLI continues to check one package with its existing baseline and exit-code
defaults; the action selects packages and baselines and applies the CI gate.

## Use automatic selection

After checkout and the repository's normal install/build steps:

```yaml
- uses: stentorian-io/pledge@RELEASE_TAG
```

`RELEASE_TAG` stands for a published Pledge release with automatic selection.
Each selected package gets its own baseline, packed snapshots, findings, and
result. The action checks every selected package, including unchanged members;
shared build inputs and dependencies can affect their published interfaces.

## Discover the packages

Start at the checkout root and use the repository's workspace declarations:
`package.json` workspaces for npm/Yarn-style projects, or the `packages` patterns
in `pnpm-workspace.yaml`. Respect inclusion and exclusion patterns. These files
already express package membership; see [npm workspaces](https://docs.npmjs.com/cli/v11/using-npm/workspaces/)
and [pnpm workspace configuration](https://pnpm.io/settings#packages).
Yarn's object form, `workspaces.packages`, is also supported. If both workspace
files exist, their resolved members must agree. A pnpm file without `packages`
selects only the root. Patterns must be relative to the checkout without `..`
segments; `node_modules` and `.git` are always excluded from glob discovery.
Discovery does not recursively follow directory symlinks.

Include the root package when it is publishable, along with publishable
workspace members. A manifest with `"private": true` is excluded, but a private
workspace root still supplies its member declarations. This field is distinct
from a package published to a private registry: registry visibility does not
exclude a package from checking.

Without workspace declarations, select the root package. Do not recursively
interpret every manifest as a release target: fixtures, examples, generated
output, and installed dependencies can contain their own `package.json` files.
Repositories with separate packages outside a workspace can select them with
explicit `path` inputs in separate action steps.

Validate selected packages before contacting a registry. Each must have a valid
package name and SemVer version, and resolve inside the checkout. Deduplicate
paths; different selected directories claiming the same package name are an
error. Malformed or conflicting workspace declarations, invalid selected
manifests, and an empty selection fail visibly. Show selected packages and
excluded private workspace members in the summary.

## Choose each baseline

Read the name and version from each selected package's local `package.json`.
Use the project's npm registry configuration to list published versions of that
name, then select the highest SemVer version without a prerelease suffix with
the same major version:

| Local package version | Matching published versions | Example baseline |
| --- | --- | --- |
| `2.6.0` | `2.*.*` | `2.5.3`, even if `3.1.0` is npm `latest` |
| `1.4.0` | `1.*.*` | `1.9.2` if that is the highest published `1.x` |
| `0.4.8` | `0.*.*` | `0.5.2` if that is the highest published `0.x` |
| `2.0.0-beta.1` | Stable `2.*.*` | No baseline if only `2.x` prereleases exist |

The release line comes from the package version in the checkout. Branch names
are not parsed. Selection uses SemVer order, not publication date, and is not
capped by the local version: an older checkout still compares against the
highest published version in its line. Report both versions prominently.
Resolve each baseline once and acquire that exact version for the comparison.

## Report breaking changes without blocking development before 1.0

This rule concerns the package being checked, independently of Pledge's own
version. When its local version remains `0.x`, detected breaking changes are
advisory: show the findings, but do not fail the action because of those findings.
This applies to both patch and minor changes, such as `0.4.7` to `0.4.8` and
`0.4.7` to `0.5.0`.

Baseline selection and the gate are separate decisions. Local `0.4.8` still
compares against the highest published `0.x`, using the same-major rule above.
The findings retain their actual severity. The action reports an advisory
outcome when breaking findings are permitted, rather than calling the comparison
clean. Errors, missing baselines, and incomplete analysis still fail the gate.

[SemVer](https://semver.org/#spec-item-4) allows the API to change at any point
before 1.0. This action policy permits those changes without suppressing their
detection or altering the standalone CLI's result contract.

The exemption ends when the local major becomes `1` or higher. An existing
`1.x` baseline is then checked with the normal blocking policy. For the first
`1.0.0` release with no published `1.x`, the missing-baseline policy below
applies; reaching 1.0 does not itself establish a requirement to preserve the
entire `0.x` API.

## Override selection

Inputs select automatically when omitted or empty:

| Input | Default | Explicit value |
| --- | --- | --- |
| `path` | Discover root/workspace packages | Check exactly this package directory inside the checkout; `.` selects only the root |
| `against` | Select a baseline independently for each package using its release line | Exact published version or npm dist-tag |

```yaml
- uses: stentorian-io/pledge@RELEASE_TAG
  with:
    path: packages/core
    against: '2.5.3'
```

An explicit `against: latest` selects that tag even if it points to another
release line. An explicit tag such as `next`, or an exact prerelease version,
permits comparisons with prereleases. With multiple automatically selected
packages, the same explicit selector is resolved separately for every package
name. Use separate steps with `path` for different overrides per package.
Version ranges, URLs, local baseline paths, and arbitrary cross-package
comparisons remain outside this interface.

## Compare and report every package

Pack each built local package with lifecycle scripts disabled, acquire the
published baseline, and compare their public contracts using Pledge's existing
analysis. Selecting a version does not build missing outputs or freeze
transitive dependencies. Package findings retain their existing rule IDs and
severity, regardless of the version written in the local manifest.

The summary identifies each package's path, name, local version, baseline,
selection reason, and result. Continue with other packages after an individual
comparison fails. Keep the aggregate `status` and `exit-code` output names and
include an `advisory` status, with this precedence:

1. Any operational or configuration error: `error`, exit `2`.
2. Otherwise, any incomplete comparison or missing baseline: `incomplete`, exit `3`.
3. Otherwise, any breaking findings in a package with local major `1` or higher: `breaking`, exit `1`.
4. Otherwise, any breaking findings in a package with local major `0`: `advisory`, exit `0`.
5. Otherwise: `clean`, exit `0`.

Always show breaking findings even when an error or incomplete result determines
the aggregate status. In a mixed repository, a `0.x` package's advisory findings
cannot hide blocking findings in a `1.x` package.

## Handle a missing baseline

The strict default reports `no baseline` for an
otherwise accessible package with no matching stable version, and makes the
aggregate result incomplete. It does not silently switch to another line or
claim compatibility. An explicitly requested version/tag that cannot be
resolved remains an error, as do authentication and network failures. A registry
404 alone cannot establish that a package has never been published, because
registries can conceal inaccessible packages with that response.

The first release of a new major or a wholly new package therefore needs
deliberate CI handling. There is no action option to treat a missing comparison
as a pass. The CLI without `--against` supports first-publish analysis when the
registry returns 404, as described in [the CLI guide](ci.md#install-and-run).
