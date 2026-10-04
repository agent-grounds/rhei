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

`docs/changelog.md` contains the latest inline release section and an
`Older releases` section. Release automation generates the numbered section for
the version it releases, archives the previous inline section under
`docs/changelog/`, and extracts the inline section for GitHub release notes.

Nobody writes the changelog. No pull request adds a bullet, none is refused for
leaving the changelog alone, and nobody writes anything before a release: the
release lists the pull requests merged since the previous one (§5.1), read from
the forge, and fails rather than writes an incomplete list (§5.2). The scheduled
release is due whenever that list is not empty (§5.3).

### 5.1. What a release's notes list

A change does not record itself, and neither does a person on its behalf: a
pull request's title already says what it changed, so the release writes the
list from the titles at the moment it is cut.

The list is every pull request merged since the previous release tag that
changed more than docs and CI. Docs and CI are the paths under `docs/` and
`.github/`, every `*.md` file, and the root `LICENSE` and `lychee.toml`.

- The commits are those between the previous release tag and the commit being
  released. The previous release tag is `v<version>`, where the version is the
  one the inline release heading names; a release whose previous tag does not
  exist is refused.
- Each commit is mapped through the forge to the pull requests it belongs to,
  and only a merged pull request counts. One pull request that merged as several
  commits is listed once.
- A pull request's paths are the union of the paths its commits changed, so a
  pull request is left out only when every one of its commits touched docs and
  CI alone.
- A commit that changed more than docs and CI and belongs to no pull request -
  a direct push, or the version advance that follows a release - is not listed,
  and the release warns about it with
  `warning: commit <short sha> changed code but belongs to no pull request; it is not in the notes`.
- Each entry is the pull request's title linked to the pull request, then its
  number: `- [<title>](<url>) (PR #N)`. A `[`, `]` or `\` in a title is escaped
  with a backslash, so the link still renders.
- The entries are newest first, ordered by where each pull request's newest
  commit sits on the default branch's first-parent line, so the order comes
  from git rather than from timestamps.

When the release archives a generated section, its line under `Older releases`
summarises it as `<N> pull requests.`, N being the number of entries it held.

The file's shape is checked on every pull request: the header, then one inline
release section, then `Older releases`, and no `## Unreleased`. The check reads
headings only, not the entries, so a maintainer may still correct an entry by
hand after a release.

### 5.2. When the forge cannot answer

The notes are read from the forge, so a release that cannot ask it has no notes
to write, and fails before it writes anything - the changelog, the archive and
the tree are left exactly as they were:

- `gh` is not on `PATH`: `error: gh is not on PATH; a release's notes are read from the forge`.
- The forge cannot be asked about a commit, or answers with an error:
  `error: could not ask the forge about commit <short sha>: <gh's message>`.

A partial answer is a failure too: one unanswered commit fails the whole list,
because a list missing one pull request looks exactly like a correct one, and a
release dispatched again costs less than notes that quietly leave a change out.

A list with nothing on it is refused with
`error: no pull request merged since v<previous version> changed more than docs and CI; nothing to release`,
because a release with empty notes would fail at extraction anyway, and the
first step is where the reason is clearest.

### 5.3. When the scheduled release waits

The scheduled release is due when the list of §5.1 is not empty, and asks the
same question the release answers. Before it cuts anything:

- Nothing changed outside docs and CI since the previous release tag: it does
  not release, and says `Only docs/CI changes since <tag>; skipping.` This is
  read from git alone and asks the forge nothing.
- Something did, but no pull request on the list: it does not release, and says
  `No pull request that changed code merged since <tag>; skipping.` The version
  advance that follows every release changes code and belongs to no pull
  request, so it holds here rather than release nothing.

Otherwise the release is due. A run that holds ends green: holding is the
expected answer while nothing has merged, not a failure. A forge that cannot
answer (§5.2) fails the run, because a list it could not build is not an empty
one.

A release a person starts does not hold: starting it is the decision that a
release is due. It refuses an empty list instead, with the refusal of §5.2.

## 6. Local Gates

The local pre-commit configuration mirrors the CI checks that are cheap enough
to run before a commit, and the pre-push hook reruns the Rust test suite. The
release PGO build is intentionally excluded from local commit hooks.

The pre-commit configuration also runs the release scripts' own tests with
`python -m unittest discover -s scripts/tests -t .`, so the shape check and the
generated notes of §5.1, the failures of §5.2 and the due gate of §5.3 are
proven on the contributor's own platform before a commit, and no hook checks a
branch for a changelog bullet.

## 7. Supported Platforms

Which platforms Rhei supports, and what support means, is a project-wide
requirement rather than a property of the release process:
[§REQ-cross-platform](../requirements/cross-platform.md#req-cross-platform-one-tool-on-linux-macos-and-windows). The release's part is §1 of it — a release ships binaries
for every supported platform (§4) — and the PGO matrix is the list.
