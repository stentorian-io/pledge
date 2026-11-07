# npm manifest coverage

Use the [coverage inventory](language-coverage.md) to distinguish tested package
contracts from npm's publication and source-project settings. Pledge compares
the files produced by `npm pack`, public JS/TS entry points, installed command
names, peer obligations, engine ranges, and platform requirements. It does not
execute package lifecycle hooks or establish equivalence of arbitrary dependency
bodies.

The reviewed reference is npm CLI revision
[`bfacd33ccbcd908480610703b60455d2da5b57a9`](https://github.com/npm/cli/blob/bfacd33ccbcd908480610703b60455d2da5b57a9/docs/lib/content/configuring-npm/package-json.md).
Its complete package-json document has SHA-256
`3644b6951ffe70f98f02456ce7fb5b8888906e474705d979fa0bbb2eddfb42c1`. Every
indexed section has an individual assessment; exclusions do not count as passing
conformance. The npm consumer profile requires npm 11.19.0 and TypeScript 5.9.3
for its missing-dependency enrichment check, and records Node separately.
Tests install local packages offline or through a loopback HTTP
tarball server, with scripts disabled and isolated caches. They query the actual
host OS/CPU and never publish packages.

## Installed interface contracts

Real npm consumers cover allowlists, exclusions, mixed positive/negative lists,
scalar/array equivalence, empty lists and `any` for `os` and `cpu`. Engine tests
exercise a supported Node range, a rejected range with `--engine-strict`, and a
compatible widening. Ordinary npm installs only warn about engine mismatches;
Pledge compares the declared supported set. The `libc` case rejects a previously
unrestricted host; it does not validate native binaries or every libc detection
path. OS/CPU flags do not simulate mandatory-package platform checks in these
npm tests.

Packing tests remove or preserve an importable JavaScript file through `files`
and execute the same deep import after installation. npm owns packlist details,
including ignore files and mandatory inclusions. These cases do not certify
every packlist glob or distributed asset format.

Command tests compare a removed command and compatible equivalent forms, then
check the installed npm command links and run the executable with Node. A string
`bin` on `@scope/tool` names command `tool`. Targets such as `./commands/tool`
and `commands/tool` are equivalent. `directories.bin` expands nested files and
nested directories, omitting dot paths. Directory links are observed as links;
they are not claimed to be executable. npm anchors `/commands` and `../commands`
inside the package before expansion; tests verify both against an explicit
command map. The implementation follows the pinned npm package-json normalizer's
`secureAndUnixifyPath`/`binDir` behavior. Ambiguous duplicate basenames,
unreadable or unsafe scans, scan budgets, and unpacked directory symlinks retain
incomplete evidence instead of speculative command-removal findings. npm packing
removes source symlinks. Windows command wrappers and arbitrary executable
behavior are outside these Unix consumer cases.

Peer tests install a local host alongside old/new plugin tarballs: a conflicting
range fails resolution, a widening succeeds, and an absent optional peer becomes
an unmet installation obligation when made required. Other dependency graph and
prerelease combinations remain sampled rather than exhaustive.

## Dependency and browser evidence

Local tarball and local Git commit dependencies, optional dependencies, and
npm-packed bundled dependencies have old/broken/compatible forwarded-export
consumers. Missing required dependency evidence produces failed preparation and
degraded enrichment; it never proves an unresolved forwarding declaration
compatible. `dependencies` and `optionalDependencies` are installation inputs,
not promises that every internal dependency version change is a breaking public
API change.

HTTP tarball consumers acquire three package versions from an isolated loopback
server. HTTPS/TLS, redirects and authentication are not exercised. Git URL
selectors and GitHub shorthand are passed to npm for acquisition. Local
`git+file` consumers pin three temporary repository commits and verify changed
and preserved forwarded exports. Remote transport syntax, authentication,
availability, Git semver selectors and build behavior are not independently
exercised by these offline local consumer cases. The inventory records this
limit explicitly; local dependency success is not credited as proof of those
remote protocols. GitHub shorthand specifies an acquisition locator and optional
Git ref; it is not a separate semver contract between the packed releases being
compared. npm interprets the locator and resolves the ref before Pledge compares
the dependency-backed public interface. Faithful forwarding and explicit
failed-preparation evidence remain requirements of Pledge acquisition. This
scope boundary does not certify shorthand normalization or GitHub resolution,
and neither local HTTP nor local Git evidence establishes those operations.
Installation failures already degrade analysis. Build and lifecycle execution
equivalence is outside static interface comparison, including Git dependency
prepare/build hooks.

The string `browser` entry is discovered as an additional JS/TS surface. Tests
load the manifest's selected file directly with Node and compare removed and
preserved exports. They do not simulate a browser, DOM APIs, a specific bundler,
or object-form browser replacement maps. `main`, `exports`, and `type` reuse the
separate Node loader consumer evidence and its declared limitations.

## Scope decisions for the remaining sections

The document introduction and see-also list organize the reference. Name/version
publishing constraints and registry uniqueness identify a release; Pledge
selects a baseline for the same package and does not validate registry naming
rules or enforce version increments. Description, keywords, homepage, issue
URLs, people, funding and repository fields serve discovery, attribution,
support, or project navigation. License permissions are legal terms, outside
semver API analysis.

`man` and `directories.man` describe installed manual pages. Manual naming,
sections, compression, links and prose are documentation contracts outside the
compared JS/TS and executable-command interface. The general `directories`
section describes package organization; its bin and man subcontracts have
separate assessments.

`scripts`, `gypfile`, `config`, and the default start/install commands describe
lifecycle execution and native build behavior. Pledge requires built output and
disables these hooks; it cannot certify hook side effects, native compilation,
script environment variables, or command output. Changes to generated public
exports are still compared. Default AUTHORS attribution is metadata.

`devDependencies` and `devEngines` concern work on the source project.
`workspaces` describes source-root package linking; Pledge selects one packed
package, not compatibility of a monorepo's installation layout. `overrides` and
its fork-replacement examples configure the installing root, and npm ignores
them in installed dependencies. Analysis therefore removes these overrides from
its temporary installation manifest while preserving the input manifest. A real
npm consumer with an otherwise conflicting transitive override verifies this
boundary. This exclusion concerns root resolution policy, not exported contracts
that reach into dependencies. `private` and `publishConfig` control publication,
registry, tagging and access; they are not installed API compatibility gates.
