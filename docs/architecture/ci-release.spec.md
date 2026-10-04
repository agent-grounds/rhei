# AR-ci-release: CI and release automation mirror local gates

Rhei uses GitHub Actions as the remote authority for formatting, linting,
build, test, grounding, pre-commit, and release checks. The workflow layout
keeps normal pull-request feedback fast while moving slower packaging work to
pre-release and release workflows. [§FS-rhei-distribution](../functional-spec/rhei-distribution.spec.md#fs-rhei-distribution-rhei-distribution-and-release-process)

## 1. Development CI

The matrix is the cross-platform requirement made executable: Rhei supports
Linux, macOS, and Windows as one tool ([§REQ-cross-platform.3](../requirements/cross-platform.md#3-tested-not-assumed)), so the
suite that proves a behaviour runs on all three.

The `CI` workflow runs on pushes and pull requests as two jobs that run in
parallel, so pull-request feedback takes as long as the slowest test platform
and no longer.

**`test`** runs on Linux, macOS, and Windows. Each platform installs the pinned
Rust toolchain from `rust-toolchain.toml`, restores the cargo registry and
`target` cache, and runs the same four commands — the Rust formatting, lint,
and build gates, then the whole Rust test suite in one command:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::all
cargo build --workspace --all-targets --locked
cargo test --workspace --all-targets --locked --no-fail-fast
```

No platform is a subset of another: every target the workspace has runs
everywhere, which is what [§REQ-cross-platform.3](../requirements/cross-platform.md#3-tested-not-assumed) asks of this workflow. The
individual tests that genuinely need POSIX semantics — signal delivery and
process groups, file modes and symlinks, a contended `flock` — are
`#[cfg(unix)]` and each says at the gate which semantics it exercises.

The suite's mock agents, programs, and callbacks are Python scripts, which is
what lets one command run on all three platforms, so each `test` job installs a
Python alongside the Rust toolchain.

That Python is also what runs the repository's release scripts, so the `test`
job runs their own tests as a fifth command. The pre-commit hook runs the same
tests on every contributor's machine
([§FS-rhei-distribution.6](../functional-spec/rhei-distribution.spec.md#6-local-gates)), Windows included, which makes
them a behaviour the cross-platform requirement covers: proving them on Linux
alone would leave two of the three platforms they run on unproven
([§REQ-cross-platform.3](../requirements/cross-platform.md#3-tested-not-assumed)). One of them reads the repository's own
`docs/changelog.md` and checks the shape the release reads - the header, one
inline release section, `Older releases`, and no `## Unreleased`
([§FS-rhei-distribution.5.1](../functional-spec/rhei-distribution.spec.md#51-what-a-releases-notes-list)) - with the release script's own heading
patterns, so a changelog the release could not generate into fails on its own
pull request rather than on the scheduled release.

Subprocess-driving E2E and integration harnesses must ask Cargo to verify and,
when needed, rebuild the `rhei-cli` binary from the current checkout before the
first subprocess use in each harness process. A pre-existing profile binary is
not evidence of freshness; successful verification may be shared by later uses
in that process.

The check and the rebuild must name one directory: the profile directory the
harness will execute the binary from, which it reads off the running test
binary's own location. The rebuild is directed at that directory explicitly
rather than left to what a child process happens to inherit, because
`--target-dir` on the outer invocation reaches no child and an inherited
`CARGO_TARGET_DIR` can name a third place — so a run given a target directory of
its own rebuilds where the harness is looking instead of into the checkout's
default `target/`. Directing the rebuild must not drop what the profile
directory already told the harness, the release profile included. When a build
that succeeded leaves no binary where the harness looks, the harness reports the
path it checked and the build it ran to produce it, because a message naming
only the path sends a reader hunting in a checkout that built correctly.

**`lint`** runs on Linux only. It runs `grund config validate` and
`grund check .`, then the repository `.pre-commit-config.yaml` against all
files with the cargo hooks skipped — `test` has just run them on three
platforms, and running the suite a second time on one of them bought nothing —
so the remaining hook contract (fissile, lychee, attribution boilerplate) is
enforced remotely. It runs no changelog step, because no pull request is checked
for a bullet ([§FS-rhei-distribution.5.1](../functional-spec/rhei-distribution.spec.md#51-what-a-releases-notes-list)), and so it checks out the
single commit a checkout takes by default: nothing in the job needs the pull
request's base. The job keeps its name,
`repository gates (grund, fissile, lychee, changelog, attribution)`, because
`main`'s ruleset requires it as a status check by that exact name, and a
renamed job is a check no pull request can satisfy until a person edits the
ruleset. The gate binaries (`grund`, `lychee`, `fissile`) are installed from
source only on a cache miss: they live under a root of their own keyed by their
pinned versions, so a version bump rebuilds exactly that tool and nothing else.

Both jobs stay inside the one `CI` workflow because the release helpers (§3)
look the green run up by workflow name.

## 2. Local Hooks

The pre-commit hooks run `grund`, formatting, clippy, build, tests, the release
scripts' tests, link checks, and attribution boilerplate checks before a
commit. The pre-push hook reruns the Rust tests and nothing else
([§FS-rhei-distribution.6](../functional-spec/rhei-distribution.spec.md#6-local-gates)): no hook, at either stage, checks
`docs/changelog.md` for a bullet, because a change adds none.

## 3. Release Workflows

The release workflow verifies the requested version against the selected source
ref, checks package-name ownership or availability, builds PGO binaries for the
supported release platforms, publishes crates.io packages in dependency order
when requested, and creates or updates the GitHub release from the extracted
changelog notes.

Patch and minor release helper workflows follow the same model as the release
workflow: they require a green `CI` run on `main`, create a version bump commit,
dry-run the release workflow from the candidate branch, then fast-forward
`main` and dispatch the publishing release.

Both helpers generate the release's notes
([§FS-rhei-distribution.5.1](../functional-spec/rhei-distribution.spec.md#51-what-a-releases-notes-list)) in the same step
that bumps the versions: `prepare_changelog_release.py prepare <version>` lists
the commits since the previous tag with git, maps each to its pull requests
with one forge read per commit, and writes the numbered section, the archive and
the `Older releases` line only once the whole list is built. A forge it cannot
ask fails the step before anything is written
([§FS-rhei-distribution.5.2](../functional-spec/rhei-distribution.spec.md#52-when-the-forge-cannot-answer)), so a dispatch
again is the whole recovery. The generated changelog rides the version bump
commit, so no bot commit and no branch-protection bypass is added. Resolving a
commit to its pull request is a forge read, which is the one permission the
notes add to these workflows. Neither helper writes, stamps or edits the
changelog in any other step.

`Auto bump` first asks the release script whether a release is due
([§FS-rhei-distribution.5.3](../functional-spec/rhei-distribution.spec.md#53-when-the-scheduled-release-waits)). Its step
`Gate - non-doc/CI changes since last tag` is one call,
`prepare_changelog_release.py due <tag> --output "$GITHUB_OUTPUT"`, which writes
the step's `ok` output and the notice the run leaves when it holds. The step
keeps its id, so every later step keeps the condition it already carries, the
advance to the next `-dev` version included, and that advance holds with the
release. The answer lives in the script rather than in the workflow's shell,
because "due" is "the list `prepare` would write is not empty", so `due` builds
that same list rather than a second approximation of it, and because a script is
tested on all three platforms where a workflow's shell is tested on none. It
reads paths with git first, with the docs-and-CI filter the step used to run
inline, so a week of docs alone asks the forge nothing.

`Release minor` does not ask: a person started it, and starting it is the
decision. `prepare` refuses an empty list on its behalf, before any version is
committed.

## 4. PGO Boundary

PGO is exercised by the manual pre-release workflow and the release workflow,
not by the normal development CI matrix. This keeps pull-request feedback tied
to correctness and API behavior while still verifying that packaged binaries
can be generated before a release. [§FS-rhei-distribution.4](../functional-spec/rhei-distribution.spec.md#4-pgo-binary-builds)
