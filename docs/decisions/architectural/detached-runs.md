# DA-detached-runs: A detached run is a supervisor process, not a daemon service

## 1. Status

accepted

## 2. Context

`rhei run` fused two roles into one process: the **orchestrator** that owns
every subprocess and every transition, and the **surface** that renders what it
is doing. The fusion showed up as three unrelated-looking limitations.

A run could not outlive its terminal. Closing the shell delivered `SIGHUP`,
which [§FS-rhei-run.3.2](../../functional-spec/rhei-run.spec.md#32-interruption-and-process-ownership) correctly treats as an interruption — so a long run had
to be babysat, or wrapped in `nohup`/`tmux` by hand, which put it outside the
process-group ownership `rhei run` promises ([§DA-supervised-process-groups](supervised-process-groups.md#da-supervised-process-groups-subprocesses-are-supervised-process-groups-with-one-termination-path)).

A run could be watched only by the shell that started it. The browser dashboard
was the one exception, and only because it happens to be a loopback server. An
operator on a second terminal, or a second person, had no terminal surface at
all.

A run could not be consumed by another program. The plain frontend prints
human prose; a tool wanting task outcomes had to scrape it or read the
fixed-column journal, which was designed for `tail -f`, not for a parser.

The three have one cause and one fix: give the run an identity, publish its
events in a form something else can read, and make the surface a client.

## 3. Decision

**A detached run is the same `rhei run` process, re-executed in its own
session.** `--headless` spawns `rhei run` again as a child with `setsid`, its
own process group, stdin on `/dev/null`, and its console redirected to
`runtime/run.log`. There is no daemon, no service manager, no long-lived
supervisor process shared between runs, and no new execution path: the detached
child takes the same run locks, drives the same loop, and writes the same
report.

**We spawn; we do not `fork`.** `rhei run` is multi-threaded before it could
possibly daemonize — the render thread, the dashboard server, the stdin writer
threads — and `fork()` in a threaded process leaves the child holding locks no
surviving thread will ever release. Re-exec is the only safe shape.

**The handshake is the descriptor, not a pipe.** The child publishes
`runtime/run.json` naming its own pid with status `running`; the launcher polls
that file while watching for the child to exit. This costs a few file reads and
buys a synchronous startup contract ([§FS-rhei-run-headless.1.1](../../functional-spec/rhei-run-headless.spec.md#11-startup-is-synchronous)): a held run lock
or an invalid plan fails the launcher with the child's own diagnostic, so the
operator never receives an id for a run that is already dead. An inherited pipe
would be marginally faster and would need `pre_exec` fd juggling and a second
implementation off Unix.

The **workspace** copy is the handshake and the registry entry is not, even
though both are published in the same breath. The launcher computes the
workspace itself, so it can always read that file; the registry lives under a
state directory it has no way to guarantee is writable, and handshaking on it
turned an unwritable `$XDG_STATE_HOME` into a 30-second wait and a launcher
failure for a run that was working the whole time.

**The run lock is the primary liveness oracle, but its pathname is not the
lock.** `.rhei/run.lock` is `flock`-based and released by the kernel when the
holder dies, so contention on the current pathname proves that an open file
description still holds that inode. Acquiring the pathname is not a complete
answer: rename, unlink, or replacement can leave the live process holding the
old inode while the current path is free or absent. On Linux, the acquired inode
therefore carries a structured run id, pid, workspace, and kernel process-start
identity. `/proc/<pid>/fdinfo` bridges the pathname gap only when it proves that
the same stable process identity owns an exclusive flock on a descriptor
carrying that exact record. `rhei stop` requires this ownership proof before it
signals even when the current pathname is contended, and uses a stable process
handle for delivery so exit and pid reuse cannot redirect the signal. A numeric
pid, its existence, or registry/workspace agreement cannot answer ownership —
pid reuse makes a stale descriptor point at an unrelated process — and a
heartbeat would add a clock, a timeout, and a new way to be wrong.

**The oracle answers three things, not two.** Live, ended, and *unknown*. The
probe reads a file and takes a lock, and either can fail for reasons that say
nothing about the run — a `chmod`, a full disk, an exhausted descriptor table.
Folding those into "ended" would be harmless if the verdict were only rendered,
but it is also what prunes the registry: an error had to be able to unregister
nothing at all. Only a workspace that no longer names the run prunes its entry
([§FS-rhei-run-headless.2](../../functional-spec/rhei-run-headless.spec.md#2-the-run-descriptor)). Completion goes further and prunes nothing, because a
tab keypress is a read.

**The third answer is carried by the type, not by a helper.** There is no
`is_live()` to collapse it: every consumer matches on the verdict, so a new
consumer cannot silently inherit "unknown means dead" — the shape that made
`attach` report a working run as finished, `attach --json` truncate its stream
and exit `0`, and `stop --wait` return while the run was still tearing down. The
consumers that wait share one bounded grace, so "keep waiting" cannot become
"wait forever" on an outage that never clears ([§FS-rhei-run-headless.3](../../functional-spec/rhei-run-headless.spec.md#3-run-identity-and-liveness)).

**`rhei attach` reads files and posts to the two boundaries that already
exist.** Structural events come from `runtime/events.jsonl`; live agent output
comes from the per-task logs, which already hold the complete transcript
([§FS-rhei-run-tui.1.2](../../functional-spec/rhei-run-tui.spec.md#12-live-agent-traffic)); intervene and gate release post to the control server's
existing `/intervene` and `/transition-gate`. No new streaming endpoint, no
ring buffer to size, no protocol between run and surface beyond a file format
that had to exist anyway for `--json`.

**Stopping is a signal, not a route.** `rhei stop` sends `SIGINT` to the run's
pid. The loopback server keeps the single inbound mutation boundary
[§AR-rhei-viz-flow.7](../../architecture/rhei-viz-flow.spec.md#7-intervene-the-single-mutation-boundary) gives it, and stopping inherits [§FS-rhei-run.3.2](../../functional-spec/rhei-run.spec.md#32-interruption-and-process-ownership) in full
rather than growing a second, subtly different teardown.

**In an attached surface, `Ctrl+C` detaches.** The driving TUI re-raises
`SIGINT` on the run ([§FS-rhei-run-tui.1.8](../../functional-spec/rhei-run-tui.spec.md#18-failure-modes)); the attached one must not, because
the reflex that ends a foreground command would otherwise end a run that
another terminal — or another person — is also watching.

## 4. Consequences

The detached child must **not** arm the parent-death signal that
[§DA-supervised-process-groups](supervised-process-groups.md#da-supervised-process-groups-subprocesses-are-supervised-process-groups-with-one-termination-path) arms for agents and programs. That backstop
exists so a `SIGKILL`ed supervisor still takes its workers down; applied to the
headless child it would kill every detached run the instant its launcher
returned, which is the one thing detachment is for. The rule is now: supervised
*work* gets `set_pdeathsig`; the *supervisor* never does.

`runtime/events.jsonl` is truncated per run, so `seq` starts at 1 and one file
is one run. Cross-run history stays where it already lives — the transitions
journal and the timestamped run reports.

A registry entry outlives its run, capped at the hundred most recent
([§FS-rhei-run-headless.2](../../functional-spec/rhei-run-headless.spec.md#2-the-run-descriptor)). Deleting it on exit made the id stop resolving at
exactly the moment the run's result became available, which broke the one CI
shape the launcher deliberately does not provide on its own: launch detached,
then `rhei attach --wait` for the answer.

Attachment works only on the machine that runs the run: it reads that
workspace's files and dials a loopback address. Remote observation stays out of
scope, and stays consistent with [§FS-rhei-run-tui](../../functional-spec/rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal)'s own non-goal.

Detachment is Unix-only for now ([§FS-rhei-run-headless.1.3](../../functional-spec/rhei-run-headless.spec.md#13-platform)). Windows has the
primitives — `DETACHED_PROCESS`, `CREATE_NEW_PROCESS_GROUP` — but no equivalent
of the signal contract `rhei stop` inherits, so shipping it there means
designing that teardown rather than translating this one.

Because the descriptor and the event log are written by *every* run, not only
detached ones, a plain foreground `rhei run` is attachable and machine-readable
too. That was not the goal; it is what falls out of putting the identity on the
run rather than on the flag.

## 5. Alternatives Considered

**A `rheid` daemon owning many runs.** One process to attach to, cross-run
scheduling, a natural home for a registry. Rejected: it introduces a second
lifetime to manage, a socket protocol, an upgrade story, and a supervisor whose
death orphans work — the exact failure [§DA-supervised-process-groups](supervised-process-groups.md#da-supervised-process-groups-subprocesses-are-supervised-process-groups-with-one-termination-path) was
written to remove. Nothing about attaching to a run requires a process that
outlives it.

**A streaming `/events` endpoint on the control server.** Lower latency than
following a file, and no duplicate state. Rejected: the server is a
single-threaded non-blocking accept loop, so a long-lived streaming connection
would wedge it or force a thread-per-connection rewrite — and it would make
attachment depend on a server that `--no-dashboard` can turn off, where reading
a file does not.

**`--headless` meaning "foreground, no TUI".** Simpler, no process management.
Rejected: `--no-tui` already means that, and it leaves the terminal-lifetime
problem — the one that sends people to `tmux` — completely unsolved.
