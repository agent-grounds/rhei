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

Every pull request is represented in `Unreleased` by a bullet of its own, and
the pull request's number is stamped onto that bullet by the release rather
than written by its author. The author cannot know the number: it does not
exist until the pull request is opened, which is after the push a gate could
refuse, so a rule that asked for it could first be enforced by CI and nowhere
earlier.

### 5.1. What the pull request check requires

A pull request is checked for a bullet, and for a number only when a number is
known. `Unreleased` must hold at least one bullet that the same section does
not already hold at the merge base of the pull request's base and head. A
bullet is its `- ` line together with every line up to the next bullet or the
end of the section, and two bullets are the same bullet when their text matches
once runs of whitespace collapse to single spaces and a trailing pull-request
token - a written number, or the placeholder that stands in for one - is
dropped. So rewrapping a bullet is not a change and rewording one is; writing a
number onto a bullet somebody else wrote is not a change; a bullet moved into
the section from elsewhere is new, and one moved out is not.

The comparison is against the merge base rather than against the base branch's
tip, because a release promotion that lands on the base after the branch point
would otherwise read as the branch putting the promoted bullets back under
`Unreleased`, and a branch that added nothing would pass.

Where a number is known, a bullet the pull request added or changed may not
carry a different pull request's number, and the placeholder is not a number.
Bullets from earlier pull requests keep their own numbers and are not examined.
A number that cannot be resolved is not an error, which is the ordinary case
before the pull request exists.

The refusal names the missing bullet and says that no number is needed, because
for a contributor meeting the rule for the first time that message is the only
place the rule is stated at the moment it applies.

### 5.2. What the release stamps

Release automation stamps the pull request number onto each `Unreleased` bullet
it can resolve, in the form the section already uses, before it promotes the
section. A bullet that already carries a number is left as written; a
placeholder is replaced where it stands rather than followed by a second token.

A bullet is resolved through the commits its lines were written in, and only a
bullet whose every non-blank line resolves to one and the same pull request is
stamped. A bullet written on the release branch and later corrected by a pull
request belongs to neither, and crediting it to the correction would be worse
than leaving it blank.

Stamping never fails a release. A bullet that cannot be resolved is left as
written and reported as a warning naming the bullet and the reason, and so is a
forge that cannot be reached at all: a release that could not be cut over a
changelog annotation would cost more than the missing annotation does.

## 6. Local Gates

The local pre-commit configuration mirrors the CI checks that are cheap enough
to run before a commit, and the pre-push hook reruns the Rust test suite. The
release PGO build is intentionally excluded from local commit hooks.

The pre-push hook also runs the changelog check of §5.1, and runs it whether or
not a pull request exists, so that a push whose `Unreleased` section holds no
new or changed bullet against the branch's base is refused on the contributor's
own machine. That is the only place the rule can reach them before CI does, and
it is why the check asks for a bullet rather than for a number. The refusal
names `SKIP=changelog-pr-entry` as the way to push a branch that is not
becoming a pull request; that skip is local and CI does not honour it, so the
pull request is still checked. Where the branch's base resolves to no ref at
all the hook degrades to requiring the section to hold a bullet and says which
refs it tried, because a refusal a contributor cannot act on is worse than the
CI failure it is preventing.

## 7. Supported Platforms

Which platforms Rhei supports, and what support means, is a project-wide
requirement rather than a property of the release process:
[§REQ-cross-platform](../requirements/cross-platform.md#req-cross-platform-one-tool-on-linux-macos-and-windows). The release's part is §1 of it — a release ships binaries
for every supported platform (§4) — and the PGO matrix is the list.
