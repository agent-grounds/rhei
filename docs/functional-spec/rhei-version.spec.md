# FS-rhei-version: `rhei version`

Print the CLI and core crate versions so operators can include exact tool
versions in bug reports, CI logs, release checks, and support handoffs. [§GOAL-rhei-outcomes](../goals.md#goal-rhei-outcomes-goals)

## 1. Usage

```bash
rhei version
```

The command takes no arguments or options.

## 2. Behavior

`rhei version` prints one line per surfaced component:

```text
rhei-cli <version>
rhei-core <version>
rhei-validator <version>
rhei-output <version>
rhei-build <commit>
```

The version values come from the compiled crate metadata. `rhei-validator` and
`rhei-output` are modules inside `rhei-cli` rather than separate packages
([§FS-rhei-distribution.1](rhei-distribution.spec.md#1-release-targets)); they stay on their own lines because [§FS-rhei-version.3](rhei-version.spec.md#3-output-contract)
promises the component names are stable for scripts. The command does not read
plans, load state machines, inspect settings, touch runtime files, or perform
network access.

The `rhei-build` line names the commit the binary was built from:
`rhei-build <40-hex>` for a clean build, `rhei-build <40-hex>-dirty` when the
build's sources differed from that commit, and `rhei-build unknown` when no
source below could say. It is **build metadata, not binary authentication**: it
records what the build was told, and nothing checks it against the bytes that
run. The build takes the first of these that answers:

1. **The environment.** `RHEI_BUILD_COMMIT` (40 hex digits) and
   `RHEI_BUILD_DIRTY` (`true` or `false`), for packagers building outside a
   checkout.
2. **The published crate.** The `.cargo_vcs_info.json` that `cargo package`
   writes into every crate it publishes, read for `git.sha1` and `git.dirty`.
   This is what a `cargo install` from crates.io, the verify build of `cargo
   publish`, and the npm and Python wrappers that build from it read.
3. **Git.** `git --no-optional-locks rev-parse HEAD`, and the build is dirty
   when `git --no-optional-locks status --porcelain -- crates Cargo.toml
   Cargo.lock`, run from the workspace root, prints anything — untracked files
   included, because cargo compiles any file a `mod` names. Git is consulted
   only when the repository it finds tracks rhei-cli's own `Cargo.toml`, so a
   published crate unpacked inside some other repository never borrows that
   repository's HEAD.
4. **`unknown`.** No git on PATH, a git that fails, a source tree that is no
   checkout. The build never fails for want of a commit, on any platform.

The build is redone when what it read changes: the files that `git rev-parse
--git-path` names for `HEAD`, for the branch ref HEAD points at, for
`packed-refs` and for `logs/HEAD` — whichever of them exist, so a ref that lives
only in `packed-refs` is still caught when the next commit writes it loose, and
`logs/HEAD`, which is per worktree, gains a line on every commit, checkout and
reset — plus `crates/`, `Cargo.toml` and `Cargo.lock`, the same three paths the
dirty check reads, and the two environment variables. The index is not watched.

`rhei --version` carries the same fact in a parenthetical after the version,
which stays the second field: `rhei 0.6.4-dev (d0446a7)`,
`rhei 0.6.4-dev (d0446a7, dirty)`, or `rhei 0.6.4-dev (build unknown)`. The
commit is not written as semver build metadata (`0.6.4-dev+d0446a7`), because
that would change the token scripts compare.

## 3. Output Contract

The output is plain text on stdout. Each line is `<component> <semver-or-build-version>`.
The component names are stable for scripts that need to extract a specific
component version, whether or not that component is a separately published crate.

## 4. Related Specifications

- [Validate Command](rhei-validate.spec.md) - command used to verify plans
- [States Command](rhei-states-cmd.spec.md) - command used to inspect state-machine configuration
