# REQ-cross-platform: One tool on Linux, macOS, and Windows

Rhei is a cross-platform tool, not a Unix tool with ports. This requirement
holds for every feature from the moment it is specified: a feature is not
complete until its tests pass on every supported platform. [§GOAL-rhei-outcomes](../goals.md#goal-rhei-outcomes-goals)

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
exposes (signal traps, job control), where the gate's reason ([§REQ-cross-platform.3](cross-platform.md#3-tested-not-assumed)) covers the
fixture too.

A fixture never puts a shell in front of a real tool either, however thin the
forwarding looks. A shell re-parses what it forwards: Windows `cmd` reads `^` as
its own escape character, so a `.bat` forwarding `%*` hands git `main{commit}`
where the caller wrote `main^{commit}`, and `rev-parse --verify --quiet` reports
that as an ordinary unresolvable ref. The tool then reads as broken on the one
platform whose shell differs, for a reason that lives entirely in the harness
([§REQ-cross-platform.2](cross-platform.md#2-parity)). A test that needs a tool to be absent therefore takes that tool's
directories out of `PATH` and leaves the rest of it alone, rather than rebuilding
`PATH` around a wrapper.

### 4.1 A Reader Waits For The Content, Not For The File

A fixture script that waits for a file another process publishes and then parses
it waits for the content, not for the file to exist. Writing a file is creating
it and then filling it, so for a moment it exists and holds nothing, and a reader
that parses it in that moment dies on a run whose behaviour held.
agent-grounds/rhei#553 is that failure: a helper parsed a pid file its writer had
only just created, and the test reported a lock that was never taken, with the
helper's traceback left in the job log rather than in the failure.

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
waits for the terminal to end before it can say so. A wait for a run to end once
its terminal has gone is bounded too, and when it runs out the failure also
carries what the run left behind: its stderr and, read before the run is killed,
whether what it started is still alive.

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

### 7.2 A Terminal Is Created Close-On-Exec, And A Verdict On Its Hang-Up Waits For It

A native pseudo-terminal a test opens has both ends created close-on-exec, by
the call that creates them, and never flagged afterwards, however soon after.

Tests in one binary spawn at any moment on other threads, and a spawn inherits
every descriptor not flagged at that instant. A process spawned between the
creation and the flag holds a copy for as long as it lives, and what ends a
terminal is the last close. While that process holds the master, closing the
test's own copy hangs nothing up; while it holds the slave, a reader of the
master does not reach the end. Nothing complains at the terminal. The failure
shows up somewhere else, as a run that did not notice its terminal had gone and
a bound that runs out on it. agent-grounds/rhei#554 is that failure: shown on
Linux by running it, and inferred from the logs of the two macOS runs it
reports. That is a defect of the harness, not of the run or of the platform
[§REQ-cross-platform.2](cross-platform.md#2-parity), and setting the flag sooner does not cure it: a gap
between two calls is a gap however short, so only a descriptor created with the
flag has none.

The flag does not close everything. A process forked after the terminal was
opened and not yet through its `exec` holds a copy of every descriptor, flagged
or not, so the last close can come a moment after the test's own. On Linux that
was a few milliseconds; nothing has measured it on macOS. So a test that judges
the hang-up directly waits for it, within the binary's margin [§REQ-cross-platform.8](cross-platform.md#8-a-margin-outlasts-a-stalled-runner),
rather than judging the statement after its close.

The test of agent-grounds/rhei#554 is the exception, because a wait would undo
what it proves. It shows that a run notices its lost terminal, so it releases the
run's next print straight after the close. A wait there would also outlast an
inherited holder: the run would print into a dead terminal and the test would
pass with the defect back. What it keeps is a small exposure, not a zero one: the
run's one print can land inside that moment, and the failure then reads as a run
that did not exit, with the report [§REQ-cross-platform.7](cross-platform.md#7-a-terminal-harness-reports-what-it-read-without-waiting-for-the-end) asks for.

A terminal opened through a library that flags afterwards carries that window
with it, as `portable_pty` does, which the tests that also run on a ConPTY share.
A verdict on such a terminal must not turn on its last close, so the worst a
leak does there is delay a reader's end. A test whose verdict does turn on the
last close opens its terminal the first way.

The measure has three parts:

1. A process spawned after the terminal was opened and still alive when its
   master closes does not stop the hang-up: a write to the slave fails with `EIO`
   within the margin, on every platform with a native pseudo-terminal.
2. The test of agent-grounds/rhei#554 passes with the moment after the creation
   widened to a second beside another test's spawn.
3. No source calls `nix::pty::openpty`, the call that leaves the flag to its
   caller, and no file that opens a terminal sets its close-on-exec flag
   afterwards. The first part sees a flag that was lost and cannot see one that
   came late, so this is what stops the easy way of writing that.

## 8. A Margin Outlasts A Stalled Runner

A test that runs a fixture [§REQ-cross-platform.4](cross-platform.md#4-portable-fixtures) bounds it: an agent or program
timeout, a ceiling on how long the spawn took. Most of those bounds are not what
the test is about. They are there so that a broken run ends rather than hangs,
and a correct run never reaches them. Such a bound is a *margin*, and it is
sized for the slowest runner the gate [§REQ-cross-platform.3](cross-platform.md#3-tested-not-assumed) runs on at its
worst, not for what a warm interpreter takes. A shared runner can stall every
interpreter it starts for tens of seconds, and a margin a stall can cross fails
a test whose behaviour held. agent-grounds/rhei#483 is that failure: for about
40 s a `windows-latest` runner started no Python that reached its first line,
and five tests whose 5 s and 10 s margins fell inside that window failed
together, each reporting the timeout of an agent that had done nothing wrong.

So a test binary's margins are one value, named once in its test support, and
no margin is written shorter beside it. The size is then decided in one place,
against the stall rather than the warm start, and a green run still pays
nothing for it: a passing test never waits a margin out.

The measure is a runner that delays every fixture's interpreter by more than
the bounds the margin replaced and by less than the margin: the binary fails
only the tests whose bound is the behaviour [§REQ-cross-platform.8.1](cross-platform.md#81-a-bound-that-is-the-behaviour-stays-as-tight-as-the-behaviour).

For the e2e binary's shared mock-example settings, both the fallback
`defaults.agent_timeout` and every generated agent profile's `timeout` take
the one named 60-second fixture margin. The profile wins over the fallback,
so protecting only the default leaves the short bound in force. A focused
check reads the helper's generated settings and checks both levels, including
every profile. The behavioural measure is the discussion example with healthy
mock interpreter starts delayed seven seconds: it still converges, its
downstream task completes, and `decision.md` and `applied.md` exist. These
example bounds are execution margins; the behaviour-bound exception below
continues to apply to tests that mean their timeout to fire.

### 8.1 A Bound That Is The Behaviour Stays As Tight As The Behaviour

Some bounds are what the test is about: a timeout the test means to fire, or the
value a spawn is meant to report back. Such a bound stays as tight as the
behaviour needs, and it is named as behaviour where the margins are checked, so
that loosening it is a decision rather than a sweep. Where the behaviour also
needs the fixture to act before the bound — a line printed before the timeout
fires — a stalled start can still cross it. Making that test outlast a stall
takes a longer bound and a longer fixture together, which every run pays for, so
it is decided test by test rather than by the margin.

A bound around something no fixture does — a value no spawn reads, a wait that
is the code's own — is neither, and is named as such where the margins are
checked.

### 8.2 An Absence Is Pinned Outside The Margin

A test that pins an absence — the spawn did not wait for a pipe the agent's
grandchild still holds, the agent was never started — keeps what it waits on
outside its margin. Inside it, the absence the test asserts is one a stalled
runner produces on its own: an agent killed before its first line leaves no
trace of having been spawned, and a hold that ends before the margin lets a
spawn that waited for it pass for one that did not. The pin then passes against
the defect it is there to catch. So the fixture holds for longer than the
margin, or until the test releases it, and the test observes the absence itself
rather than reading it off how long the spawn took.

## 9. An Authored File Reads The Same Whatever Editor Saved It

Some editors, most of them on Windows, save UTF-8 with a byte order mark: the
three bytes `EF BB BF`, read as the character U+FEFF, ahead of the first line.
The mark says how the file is encoded and is not part of what it says, so one
leading mark is not part of the text of any file a user authors for Rhei to
read: a state machine ([§FS-rhei-states.1](../functional-spec/rhei-states.spec.md#1-schema-additions)), a template manifest and a `--values`
file ([§FS-rhei-templates](../functional-spec/rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification)), and the plan, workspace index and task files of
[§FS-rhei-language-reference.1](../functional-spec/rhei-language-reference.spec.md#1-language-surfaces), frontmatter included. Such a file loads exactly
as the same bytes without the mark: the same machine, the same values, the same
header, the same frontmatter. It is never refused because of the mark, and
never refused as text that is not YAML or that holds more than one document
([§FS-rhei-errors.1.2.2](../functional-spec/rhei-errors.spec.md#122-a-state-machine-that-will-not-load)).

Exactly one mark is set aside, and only at the very start of the file. A second
U+FEFF after it, or one anywhere else in the file, is content, and the file is
read with it in place. Setting the mark aside moves no line: a line number a
refusal reports is the same with the mark as without it, and a column on the
first line counts from the character after the mark.
