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

A fixture never puts a shell in front of a real tool either, however thin the
forwarding looks. A shell re-parses what it forwards: Windows `cmd` reads `^` as
its own escape character, so a `.bat` forwarding `%*` hands git `main{commit}`
where the caller wrote `main^{commit}`, and `rev-parse --verify --quiet` reports
that as an ordinary unresolvable ref. The tool then reads as broken on the one
platform whose shell differs, for a reason that lives entirely in the harness
(§2). A test that needs a tool to be absent therefore takes that tool's
directories out of `PATH` and leaves the rest of it alone, rather than rebuilding
`PATH` around a wrapper.

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

## 7. A Terminal Harness Reports What It Read, Without Waiting For The End

A test that drives the built binary through a terminal — a native pseudo-terminal
on Unix, a ConPTY on Windows — waits on what the terminal shows, and every such
wait is bounded. When one runs out, the failure carries what the harness had
read from the terminal up to that moment, and how long it waited. It never
waits for the terminal to end before it can say so.

The platforms do not agree on when a terminal ends. On Unix the reader sees the
end once every process holding the terminal has gone; on Windows it sees it
only once the pseudo-console itself is closed, which the harness still holds
while it reports the failure. A transcript that is assembled only at the end is
therefore whole on one platform and empty on the other, and the failure that
most needs it — a prompt that never came, on the platform whose runner is the
slowest — is the one that reads `operator prompt missing: ` with nothing after
it. That is a defect of the harness, not of the platform [§REQ-cross-platform.2](cross-platform.md#2-parity),
and it cannot be diagnosed from the gate that reports it [§REQ-cross-platform.3](cross-platform.md#3-tested-not-assumed).

The measure is a reader that has yielded output and never ends: a wait that
runs out against it reports that output, on every platform, within the bound
it was given.

### 7.1 A Wait Matches What The Terminal Shows, Not The Bytes It Was Sent

What a wait looks for, it looks for in the text the terminal shows, not in the
bytes that drew it. A Unix pseudo-terminal passes the child's bytes through, so
the two are the same there; a ConPTY renders a screen and emits a stream that
redraws it, and that stream may move the cursor over a blank cell where the
child wrote a space, or slip a window title into the middle of a line. The
prompt is whole on the screen and broken in the bytes, so a wait that matches
the bytes misses it on one platform only, and only when the frame falls that
way — a flake, not a failure [§REQ-cross-platform.2](cross-platform.md#2-parity).

So a wait reads the stream as a terminal would show it: a cursor-forward over
blank cells reads as that many spaces, a title or any other non-printing
control sequence reads as nothing, and a sequence split across two reads is
matched once it is whole. What a failure reports is still the transcript as it
was written, escapes and all, because that is what says what the terminal was
actually sent.

The measure is the stream quoted in agent-grounds/rhei#448 — the prompt's space
drawn as a cursor-forward and a window title in the line — which signals the
prompt, as does the same stream with the sequences split across reads.
