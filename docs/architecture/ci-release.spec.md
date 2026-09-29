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

That Python is also what runs the repository's gate scripts, so the `test` job
runs their own tests as a fifth command. The changelog gate of
[§FS-rhei-distribution.6](../functional-spec/rhei-distribution.spec.md#6-local-gates) runs on contributors' machines
rather than only in CI, which makes it a behaviour the cross-platform
requirement covers: proving it on Linux alone would leave the two platforms it
also refuses pushes on unproven ([§REQ-cross-platform.3](../requirements/cross-platform.md#3-tested-not-assumed)).

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
enforced remotely, and on pull requests the changelog entry check, which is
passed the pull request's base commit alongside its number so that it compares
the section against what the pull request actually added
([§FS-rhei-distribution.5.1](../functional-spec/rhei-distribution.spec.md#51-what-the-pull-request-check-requires)). Passing that
commit obliges the job to hold it, so this checkout fetches the full history
rather than the single commit a checkout takes by default: a base the clone does
not hold is a base the check refuses on, and before it refused, a gate that
could not see what the pull request added passed everything. The gate
binaries (`grund`, `lychee`, `fissile`) are installed from source only on a
cache miss: they live under a root of their own keyed by their pinned versions,
so a version bump rebuilds exactly that tool and nothing else.

Both jobs stay inside the one `CI` workflow because the release helpers (§3)
look the green run up by workflow name.

## 2. Local Hooks

The pre-commit hooks run `grund`, formatting, clippy, build, tests, the gate
scripts' tests, link checks, and attribution boilerplate checks before a
commit. The pre-push hook reruns tests and checks `docs/changelog.md` against
the branch's base, whether or not a pull request is open
([§FS-rhei-distribution.6](../functional-spec/rhei-distribution.spec.md#6-local-gates)); a pull request the hook can
resolve supplies a number to check, and its absence is not a reason to skip.

The changelog hook stays at the pre-push stage alone, and that placement is
load-bearing rather than incidental. CI's repository-gates job runs
`pre-commit run --all-files`, which runs the pre-commit stage, so a changelog
hook registered there would fire the same check twice inside one CI run with
different arguments — two gates disagreeing about one file, which is what
[§FS-rhei-distribution.5.1](../functional-spec/rhei-distribution.spec.md#51-what-the-pull-request-check-requires) exists
to stop. Both halves of the check, local and remote, therefore run one
implementation over one definition of a bullet.

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

Both helpers stamp the changelog's pull request numbers
([§FS-rhei-distribution.5.2](../functional-spec/rhei-distribution.spec.md#52-what-the-release-stamps)) in the same step
that bumps the versions, and before the section is promoted: the numbers are
resolved from the commits the bullets were written in, so they have to be read
where their authors left them. The stamped changelog rides the version bump
commit, so no bot commit and no branch-protection bypass is added. Resolving a
commit to its pull request is a forge read, which is the one permission the
stamping adds to these workflows.

## 4. PGO Boundary

PGO is exercised by the manual pre-release workflow and the release workflow,
not by the normal development CI matrix. This keeps pull-request feedback tied
to correctness and API behavior while still verifying that packaged binaries
can be generated before a release. [§FS-rhei-distribution.4](../functional-spec/rhei-distribution.spec.md#4-pgo-binary-builds)
