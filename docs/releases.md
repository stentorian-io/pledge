# Release Pledge

Review the version and user-facing notes in the Release Please PR. Merge it to
create a `vX.Y.Z` tag and GitHub release after validation. The publisher builds
one Linux x86-64 container and attaches its binary archive, checksums, and image
digest to that release. npm, crates.io, and Homebrew publishing are not configured.

Routine releases can also merge automatically after 24 quiet hours. The hourly
workflow waits for passing checks and GitHub's merge requirements. Add
`release: hold` to pause it. Breaking changes before 1.0 can release automatically
as a minor bump. Moving to 1.0, later major transitions, and breaking changes
from 1.0 require manual merge. A new commit, PR edit, comment, or label
change restarts the quiet period; the schedule is approximate, not a deadline.
Do not enable GitHub's separate auto-merge feature on a release PR: it would
not honor this workflow's delay or hold label.

## One version across distributions

Release Please owns the next version. Its single Rust release updates
`Cargo.toml`, Pledge's `Cargo.lock` entry, `.release-please-manifest.json`, and
`CHANGELOG.md`. Its generic extra-file updater also updates the image version in
`action.yml`. `bun scripts/releases/check-version.ts` rejects disagreement.
Pass a tag, for example `bun scripts/releases/check-version.ts v0.4.0`, when
validating a release artifact.

The bootstrap baseline is 0.3.0 at the existing `v0.3` commit. Earlier tags are
preserved, not renamed or recreated. Commits after that baseline determine the
first automated release. The CLI rename is breaking, so that first release
needs a minor bump. Normal new tags use three components, such as `v0.4.0`.

When npm packaging is introduced, register its package versions and exact
platform dependencies as Release Please `extra-files` updates and extend the
version check and release-file allowlist together. Do not add a second version
calculator. Homebrew should consume that same release's version and artifacts.

## Classify changes for readers

Before each commit, review the staged diff as a potential changelog entry:

- Name the user-visible result, not the implementation task.
- Use `fix` for corrections to documented behavior, `feat` for compatible
  capabilities, and `perf` for user-visible performance improvements.
- Keep internal refactoring, tests, CI, and maintenance out of product release
  notes. Do not call internal tooling a product feature just because it is code.
- Mark incompatible public changes with `!` and explain the impact and migration
  in a `BREAKING CHANGE:` footer. Preserve that footer when squash-merging.

For example:

```text
fix: avoid reporting unchanged re-exports as breaking
```

```text
feat!: rename the comparison command

BREAKING CHANGE: replace old-command with pledge in scripts and CI jobs.
```

Hooks validate syntax and suggest categories; they do not rewrite intent or
judge whether the prose is useful. Review that meaning before committing.
CI validates PR titles and descriptions for squash merges, and every incoming
commit on main for direct pushes. Use squash merges for ordinary and release
PRs, with a Conventional Commit title and any breaking footer retained.

## Compatibility policy

Pledge's public contract includes commands, flags, configuration, exit codes,
machine-readable reports, rule IDs, and documented platform support. Backend
runtime requirements apply only to their language/ecosystem. The analysis
severity assigned to a compared package is separate from Pledge's own version.

Correcting a false positive or false negative within a documented rule is a
patch, even if CI outcomes change. Deliberately expanding a stable rule's scope,
changing finding severity, removing a rule ID, or changing output/exit-code
contracts incompatibly is breaking. Document meaningful detection changes.
New opt-in capabilities are compatible features; new default checks that can
fail previously accepted inputs require compatibility review and a breaking
marker when they expand the promised checks.

| Commit intent | Before 1.0 | From 1.0 |
| --- | --- | --- |
| `fix`, `perf` | Patch | Patch |
| `feat` | Minor | Minor |
| Breaking change of any type | Minor | Major; manual merge |
| Internal maintenance | No release alone | No release alone |

Review reverts by their net user impact. A revert restoring released behavior
may need a fix release. Early development uses ordinary `0.x.y` releases;
separate `alpha`/`beta` suffix channels are not enabled. A `0.x` minor can break
compatibility, so consumers should pin an exact release or a compatible range.

## Enable the workflows

The workflows use the repository's built-in `GITHUB_TOKEN`; no custom GitHub
App or long-lived publishing token is needed.

1. In **Settings → Actions → General**, allow GitHub Actions to create pull
   requests. Organization policy must also permit this.
2. Protect `main`: require PRs, `Release checks`, `Release image`, and an up-to-date
   branch. Enable squash merging. Configure required reviews as desired:
   scheduled merging never bypasses them. A review requirement can therefore
   still require human approval for routine releases.
3. Create the `release: hold` label. Review the first release PR manually.
4. Allow the repository to publish packages. After the first image is published,
   open the `pledge` container package settings and change visibility to **public**.
   GHCR initially creates private packages; public visibility lets action users
   pull anonymously. This is a one-time maintainer setting.

Release Please requests checks explicitly for each release PR head. These
read-only checks validate the requested SHA and PR identity. The merge scheduler
dispatches **Release** after merging, and Release calls the publisher directly
with the newly created tag. This avoids relying on suppressed token-created
push/release events or approval-required bot PR runs. Do not approve duplicate
bot PR runs just to start release validation; use the dispatched checks.

Manually dispatch **Merge routine release** with its default `dry-run` input to
inspect eligibility without merging. Scheduled runs perform eligible merges.

The scheduler only handles PRs from `github-actions[bot]` and the exact
`release-please--branches--main--components--pledge` branch,
checks their changed files, scans commits since the last release for breaking
markers, and merges the exact checked head SHA. Missing tags, failed checks,
conflicts, unexpected release files, and unknown states prevent merging.
It never checks out or executes code from the release PR with its write token.

## Publish or retry artifacts

The publisher checks out the exact `vX.Y.Z` tag and verifies the Cargo and release
manifest versions. Its multi-stage Docker build uses Debian slim and bundles
Node.js, npm, and the default JavaScript TypeScript compiler. Only `linux/amd64`
is built. No runner-side Pledge installation or compiler download is needed for
the container; registry access and built package outputs are still required.
The standalone binary needs Linux with glibc 2.36 or newer, Node.js, and npm.

Each release contains:

- `pledge-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`, with the binary and licenses.
- `container-image.json`, with the GHCR digest, source revision, and platform.
- `SHA256SUMS`, covering the archive and image record.

The image is tagged `ghcr.io/stentorian-io/pledge:vX.Y.Z`. The publisher tests
compatible and breaking packages against a loopback npm registry before pushing,
plus the action's success, breaking, error, and incomplete-analysis outcomes.
It also checks authenticated npm/pnpm workspaces, same-major baseline selection,
and advisory breaking findings before 1.0 alongside stable package failures.
The release tag's action metadata references that same image version. Moving
major aliases are not published while `0.x` minor releases may break compatibility.
Retries reuse an existing version's image, validate its revision and version,
and compare existing asset bytes. They refuse to replace differing assets.

If publication fails after release creation, run **Publish release artifacts**
manually with the same exact tag. Re-running Release Please does not republish
an already-created release. If dispatch fails after a successful scheduled merge,
run **Release** on `main`. For a failed check on an unchanged release PR, re-run
that check workflow; automated dispatch does not keep retrying a failed commit.
If dispatch was missed entirely, run **Release** to request it again.

## Verify release tooling

Run `make test-tools` for isolated hook and release-policy tests. Run
`bun scripts/releases/check-version.ts` for local version consistency. Hosted
checks run `make check-all`, including compiler integration and coverage,
before release creation. `Release image` builds the same Dockerfile and runs the
packaged CLI smoke test. These packaging checks supplement the language suites;
they do not establish exhaustive language conformance.

Future distributions must use the exact release tag, verify its version, test the
packed artifacts, and publish the same version everywhere. Keep publication
retryable without changing tags or overwriting existing versions. npm platform
packages must be available before publishing the launcher. Prefer trusted
publishing and promote the stable channel only after all required artifacts
are available.

Upstream references: [Release Please configuration](https://github.com/googleapis/release-please/blob/main/docs/manifest-releaser.md),
[GitHub token workflow triggers](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow),
[GHCR visibility and authentication](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry),
and [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/).
