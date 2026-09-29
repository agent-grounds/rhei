# REQ-cross-platform: One tool on Linux, macOS, and Windows

Rhei is a cross-platform tool, not a Unix tool with ports. This requirement
holds for every feature from the moment it is specified: a feature is not
complete until its tests pass on every supported platform. [§GOAL-rhei-outcomes](../functional-spec/goals.md#goal-rhei-outcomes-goals)

## 1. Supported Platforms

The supported platforms are the ones a release ships binaries for
([§FS-rhei-distribution.4](../functional-spec/rhei-distribution.spec.md#4-pgo-binary-builds)): Linux, macOS, and Windows, on `x86_64` and
`aarch64`. Support means the same thing on all of them — the rules below.

## 2. Parity

Every user-visible behaviour — every command, prompt section, artifact, lock,
and error — behaves the same on every supported platform. Where the platform
genuinely differs (symbolic links, path spelling, process signals, file
locking), the specification of that behaviour says so at the point where it
differs, as [§FS-rhei-snapshots.7](../functional-spec/rhei-snapshots.spec.md#7-storage-layout) does for the snapshot `current` pointer. An
undeclared difference is a defect on the platform that differs, never a
limitation of it.

## 3. Tested, Not Assumed

The development CI runs the full test suite on all three platforms
([§AR-ci-release.1](../architecture/ci-release.spec.md#1-development-ci)). A test that runs on a subset of platforms carries, at the
gate, the platform-specific semantics it exercises and why no portable form
exists; a test is never gated because porting it is work.

## 4. Portable Fixtures

Test fixtures that stand in for agents, programs, callbacks, and redactors are
written in a form every supported platform runs — never a shell script, except
inside a test gated to one platform for semantics only that platform's shell
exposes (signal traps, job control), where the gate's reason (§3) covers the
fixture too.

## 5. Paths Are Data

Code never builds a path from `/`-joined strings, never compares two spellings
of one location as strings, and treats a rooted or prefixed path as outside
the workspace on every platform.

## 6. A Test's Own Directory Is Its Own By Construction

A test's private directory is named so that no other test can name the same
one — by construction, not by luck. Two names asked for are distinct whether
the two requests come from one process or from two, and whatever the clock read
when they were made: a clock reading may be one ingredient of the name, never
the whole of it.

Nothing downstream catches a name two tests share. Creating a directory that is
already there succeeds silently, so both tests proceed: each writes its fixture
into the other's tree, each reads what the other wrote, and whichever finishes
first removes the tree the other is still reading. The failure surfaces as a
wrong assertion or a missing file somewhere else entirely, never as a complaint
about the name.

The supported platforms [§REQ-cross-platform.1](cross-platform.md#1-supported-platforms) do not agree on how finely a wall
clock reads, and none of them promises a reading that has moved since the last
one. A name that leans on the clock alone therefore holds where it was written
and gives way where it is gated. That is a defect of the harness rather than a
property of the platform [§REQ-cross-platform.2](cross-platform.md#2-parity). What the three-platform gate
[§REQ-cross-platform.3](cross-platform.md#3-tested-not-assumed) reports is real — the harness is broken on every platform,
including the one it is green on — and it surfaces on the platform whose clock
reads the more coarsely, where the name ran out of resolution first.
