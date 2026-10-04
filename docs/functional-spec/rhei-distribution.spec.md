# FS-rhei-distribution: Rhei distribution and release process

Rhei releases ship the command-line binary, Rust crates, and release notes in a
repeatable process so users can install the same version from crates.io or a
GitHub release artifact. The release process must keep published package
versions, binary names, and release notes aligned with the workspace version.
[§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

## 1. Release Targets

Each release publishes exactly two crates.io packages when crate publishing is
enabled, in this order:

1. `rhei-plan` — the plan model, parser, and workspace primitives, for
   callers that want to read Rhei plans without the CLI
2. `rhei-cli` — the tool itself, which depends on `rhei-plan`

Only packages with an audience outside this repository are published. Cargo
requires every dependency of a published package to be published too, so a
crate that exists purely to divide the CLI's own source would have to be
released, named, and version-locked forever for no one's benefit. Subsystems
without an external audience therefore live as modules inside `rhei-cli`
rather than as separate packages, and a new workspace crate is a decision to
add a permanent public package, not a way to organize files.

`rhei-agent-core` is deliberately unpublished while it remains a re-export of
`rhei-plan` with no callers; it becomes a release target when it has an
API of its own. The N-API crate stays unpublished for the same reason.

The crate name `rhei` belongs to an unrelated project on crates.io, so the CLI
publishes as `rhei-cli`; the crate name is not the command name.

`rhei-cli` installs two identical binaries, `rhei` and its short alias `rh`, so
both are on `PATH` after `cargo install rhei-cli`. GitHub release artifacts
package both names with `README.md` and a SHA-256 checksum; on platforms with
symbolic links the archive stores `rh` as a link to `rhei` rather than a second
copy. Public language API packages use the package name `rhei-api` on npm and
PyPI; native N-API support is an implementation detail and is not a crates.io
release target.

Both binaries are one line of hand-off, and the CLI does its work on a thread
whose stack it sizes itself rather than on the one the platform hands `main`.
The platforms disagree about that stack by a factor of eight — a Windows main
thread reserves 1 MiB where Linux and macOS give 8 — and the smaller of the two
is not enough for this CLI: on Windows every invocation overflowed it, `rhei`
with no arguments included. A stack asked for in code travels with the binary,
which a build-time linker setting does not: it is not read when someone
installs the published crate, which is how the binary reaches the machine this
concerns.

## 2. Version Source

The workspace package version in `Cargo.toml` is the release version. Internal
path dependencies that publish to crates.io must use the same exact version as
the workspace. Package wrappers under `packages/` also carry the same release
version so source-built npm and PyPI packages can install the matching
`rhei-cli` crate.

## 3. Release Modes

Releases can be started from a `vX.Y.Z` tag or manually from the release
workflow. Manual publishing creates or reuses the matching tag when crate
publishing or GitHub release creation is enabled. Dry runs can execute the
release build without publishing crates or creating a GitHub release.

## 4. PGO Binary Builds

Distributed GitHub release binaries are built with profile-guided optimization.
The PGO training run exercises the local repository and example plans through
the everyday CLI surfaces agents and contributors use most: version reporting,
validation, listing, rendering, state-machine inspection, template discovery,
and read-only next-task selection.

Source installs such as `cargo install rhei-cli --locked` use Cargo's ordinary
release profile instead of PGO. PGO is a packaging optimization, not a behavior
contract.

Release jobs build with a newer toolchain than the `rust-version` the crates
declare, because PGO instrumentation does not link on aarch64 Linux under the
MSRV compiler. `rust-version` states what a consumer needs in order to build
the published crates and is unaffected by the compiler that produced the
release binaries; the test matrix stays on the MSRV toolchain so the promise
keeps being checked. The release toolchain is pinned explicitly rather than
tracking stable, so every distributed binary in a release comes from one
compiler version.

## 5. Release Notes

`docs/changelog.md` contains an `Unreleased` section and the latest inline
release section. Release automation promotes `Unreleased` into a numbered
release section, archives the previous inline section under `docs/changelog/`,
and extracts the inline section for GitHub release notes.

No pull request adds a bullet to `Unreleased`, and none is refused for leaving
the changelog alone. The section is written once, before a release, from what
merged since the previous one (§5.1); the scheduled release waits for it rather
than ship code the section does not yet describe (§5.3); and the release stamps
a pull request number only onto a bullet that is missing one and that it can
credit to a pull request of its own (§5.2).

### 5.1. Who writes `Unreleased`, and when

A change does not record itself. The issue a pull request closes already says
what changed, so asking every change for a bullet made every change pay for a
line the release can write once: the line, its number, a conflict on every
rebase, and a gate that turned finished work red. No pull request is checked for
a bullet, and none needs to touch `docs/changelog.md`.

Whoever cuts a release writes the section first, in one pull request, before
either release helper runs, by this rule:

- The list is every pull request merged since the previous release tag that
  changed more than docs and CI. Docs and CI are the paths under `docs/` and
  `.github/`, every `*.md` file, and the root `LICENSE` and `lychee.toml`.
- Each pull request on the list gets a bullet, or is named in one.
- A bullet's words come from the issues its pull request closed, or from the
  pull request's own description where it closed none.
- Every bullet ends in its own pull request's number, written `(PR #N)`, so the
  release has nothing to stamp onto it.
- A pull request whose bullet is already pending is skipped.

A bullet is its `- ` line together with every following line up to the next
bullet or the end of the section; trailing blank lines belong to no bullet.
Every line of the section that carries text belongs to a bullet, and the section
comes first, followed by the inline release section and then `Older releases`.
That is the shape the release reads, and it is checked on every pull request, so
a malformed write-up fails on its own pull request rather than on the release it
was written for. An empty section has that shape too.

### 5.2. What the release stamps

Release automation stamps the pull request number onto each `Unreleased` bullet
it can resolve, in the form the section already uses, before it promotes the
section. A bullet that already carries a number is left as written; a
placeholder is replaced where it stands rather than followed by a second token.
The placeholder replaced is the token the bullet ends with, even where a line
break splits it, the same token that counts as no number below; a placeholder
the bullet quotes in its prose is part of its text and is left as written.

A bullet is resolved through the commits its lines were written in, and only a
bullet whose every non-blank line resolves to one and the same pull request is
stamped. A bullet written on the release branch and later corrected by a pull
request belongs to neither, and crediting it to the correction would be worse
than leaving it blank.

A number goes into one bullet at most. Every bullet is resolved before any is
written, and a pull request that more than one bullet resolves to is stamped
onto none of them: every bullet a write-up writes resolves to the write-up's own
pull request, and crediting a whole release to the pull request that described
it would be worse than leaving the bullets blank. Each bullet so left is
reported with the reason
`PR #N would go into K bullets; write each its own (PR #N)`.
A bullet that already ends in its own number is neither written nor counted, and
a placeholder counts as no number.

Stamping never fails a release. A bullet that cannot be resolved is left as
written and reported as a warning naming the bullet and the reason, and so is a
forge that cannot be reached at all: a release that could not be cut over a
changelog annotation would cost more than the missing annotation does.

### 5.3. When the scheduled release waits

The scheduled release ships only what `Unreleased` describes. Before it cuts
anything it looks at what changed outside docs and CI (§5.1) since the previous
release tag:

- Nothing did: it does not release, and says
  `Only docs/CI changes since <tag>; skipping.`
- Something did after the section was last written: it does not release, and
  says
  `Changes merged since ## Unreleased was last written (<short sha>) wait for their release section; skipping.`,
  followed by the paths that changed.

Otherwise the release is due. A run that holds ends green: holding is the
expected answer between a merge and its write-up, not a failure, and only an
error fails the run.

The section was last written by the newest commit after the tag, on the default
branch's first-parent line, whose `Unreleased` body - its bullets, read as §5.1
reads them - differs from the body in that commit's first parent. Where no such
commit exists, the tag counts as the last write. An edit elsewhere in
`docs/changelog.md`, the note above the section included, is not a write. The
tag sits on the release commit, which empties the section, so right after a
release nothing has been written yet, and the version advance that follows the
release holds until the next write-up.

A release a person starts does not hold: starting it is the decision that a
release is due. It refuses an empty `Unreleased` instead, with
`## Unreleased has no bullet entries to promote; write the release section first`,
because writing the section is the one thing that lets it proceed.

## 6. Local Gates

The local pre-commit configuration mirrors the CI checks that are cheap enough
to run before a commit, and the pre-push hook reruns the Rust test suite. The
release PGO build is intentionally excluded from local commit hooks.

The pre-commit configuration also runs the release scripts' own tests with
`python -m unittest discover -s scripts/tests -t .`, so the shape check of §5.1,
the stamping of §5.2 and the hold of §5.3 are proven on the contributor's own
platform before a commit, and no hook checks a branch for a changelog bullet.

## 7. Supported Platforms

Which platforms Rhei supports, and what support means, is a project-wide
requirement rather than a property of the release process:
[§REQ-cross-platform](../requirements/cross-platform.md#req-cross-platform-one-tool-on-linux-macos-and-windows). The release's part is §1 of it — a release ships binaries
for every supported platform (§4) — and the PGO matrix is the list.
