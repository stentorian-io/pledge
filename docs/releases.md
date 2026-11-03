# Release Pledge

Review the version and user-facing notes in the Release Please PR. Merge it to
create a `vX.Y.Z` tag and GitHub release after validation. npm, crates.io, and
Homebrew publishing are not configured yet.

Routine releases can also merge automatically after 24 quiet hours. The hourly
workflow waits for passing checks and GitHub's merge requirements. Add
`release: hold` to pause it. Breaking releases always need manual merge,
including breaking changes before 1.0. A new commit, PR edit, comment, or label
change restarts the quiet period; the schedule is approximate, not a deadline.
Do not enable GitHub's separate auto-merge feature on a release PR: it would
not honor this workflow's delay or hold label.

## One version across distributions

Release Please owns the next version. Its single Rust release updates
`Cargo.toml`, Pledge's `Cargo.lock` entry, `.release-please-manifest.json`, and
`CHANGELOG.md`. `bun scripts/releases/check-version.ts` rejects disagreement.
Pass a tag, for example `bun scripts/releases/check-version.ts v0.4.0`, when
validating a release artifact.

The bootstrap baseline is 0.3.0 at the existing `v0.3` commit. Earlier tags are
preserved, not renamed or recreated. Commits after that baseline determine the
first automated release. The CLI rename is breaking, so that first release
requires manual merge. Normal new tags use three components, such as `v0.4.0`.

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
| Breaking change of any type | Minor; manual merge | Major; manual merge |
| Internal maintenance | No release alone | No release alone |

Review reverts by their net user impact. A revert restoring released behavior
may need a fix release; do not assume reverting a breaking change cancels its
manual-review requirement. Prerelease channels are not enabled in this draft.

## Enable the workflows

The files are a draft until committed, pushed, and the repository is configured.

1. Install a dedicated GitHub App on this repository. Grant Contents, Pull
   requests, and Issues write access, plus Checks read access. Do not grant
   branch-protection bypass permissions.
2. Set repository variable `RELEASE_APP_ID` and secret
   `RELEASE_APP_PRIVATE_KEY`. The workflows request short-lived installation
   tokens. Using an App allows bot-created PRs and merges to trigger checks;
   the built-in `GITHUB_TOKEN` suppresses those subsequent events.
3. Protect `main`: require PRs, the `Release checks` check, and an up-to-date
   branch. Enable squash merging. Configure required reviews as desired:
   scheduled merging never bypasses them. A review requirement can therefore
   still require human approval for routine releases.
4. Create the `release: hold` label. Review the first release PR manually.

Manually dispatch **Merge routine release** with its default `dry-run` input to
inspect eligibility without merging. Scheduled runs perform eligible merges.

The scheduler only handles PRs from that App and the expected release branch,
checks their changed files, scans commits since the last release for breaking
markers, and merges the exact checked head SHA. Missing tags, failed checks,
conflicts, unexpected release files, and unknown states prevent merging.
It never checks out or executes code from the release PR with its write token.

## Verification and future publishing

Run `make test-tools` for isolated hook and release-policy tests. Run
`bun scripts/releases/check-version.ts` for local version consistency. Hosted
checks run `make check-all`, including compiler integration and coverage,
before release creation. No Rust product behavior changes are needed here.

Future publishers must use the exact release tag, verify its version, test the
packed artifacts, and publish the same version everywhere. Keep publication
retryable without changing tags or overwriting existing versions. npm platform
packages must be available before publishing the launcher. Prefer trusted
publishing and promote the stable channel only after all required artifacts
are available.

Upstream references: [Release Please configuration](https://github.com/googleapis/release-please/blob/main/docs/manifest-releaser.md),
[GitHub App tokens and workflow triggers](https://github.com/googleapis/release-please-action#other-actions-on-release-please-prs),
and [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/).
