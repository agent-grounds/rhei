# FS-rhei-run-headless: Detached Runs

A run and the surface that watches it are two different things. `rhei run`
binds them into one process: close the terminal and the run dies with it, and
the only way to watch a run is to be the shell that started it. `--headless`
separates them. The run becomes a detached supervisor identified by a **run
id**; `rhei attach` is a client that connects a terminal surface to it and
disconnects again without touching the work. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

```bash
rhei run --headless plan.rhei.md   # prints a run id, returns
rhei attach 3f9a2c                 # TUI against the live run; Ctrl+C detaches
rhei runs                          # what is live on this machine
rhei stop 3f9a2c                   # the interruption contract of §FS-rhei-run.3.2
```

Everything a live run already does — the loopback dashboard, `rhei intervene`,
the run report, the transition journal — is unchanged and works the same way on
a detached run.

## 1. `--headless`

| Flag | Default | Description |
|------|---------|-------------|
| `--headless` | false | Detach the run into its own session and print its id |

`--headless` **detaches**; it does not merely suppress the TUI. A foreground
run with plain output is what `--no-tui` already selects
([§FS-rhei-run-tui.1.4](rhei-run-tui.spec.md#14-frontend-selection)), and a machine-readable one is `--json`
([§FS-rhei-run-json](rhei-run-json.spec.md#fs-rhei-run-json-rhei-run---json)). What `--headless` adds is that the run outlives the
command that started it.

`rhei run --headless` re-executes `rhei run` as a child in a **new session and
process group**, with stdin on `/dev/null` and stdout and stderr redirected to
`runtime/run.log`. The launcher then waits for the child to publish its run
descriptor (§2) and returns. A `SIGHUP` on the launching terminal reaches the
launcher's session, not the run's.

The child is an ordinary `rhei run`: it acquires the same run locks
([§FS-rhei-run.2.6](rhei-run.spec.md#26-run-lock)), drives the same execution loop, honors the same signals,
and writes the same run report. It is *not* a service, keeps no cross-run
state, and exits when its plan is done.

`--headless` conflicts with `--tui` (there is no terminal to own) and with
`--dry-run` (a preview has nothing to detach from).

`--json` alongside `--headless` describes **the launcher's** output: it prints
the new run's descriptor (§2) as one JSON object instead of the human block,
and is not forwarded to the detached run. A detached run's machine-readable
form is `runtime/events.jsonl` and `rhei attach --json` (§5.3), not a stream
into a log file nobody reads.

### 1.1. Startup Is Synchronous

The launcher does not print an id for a run that already died. It waits, up to
30 seconds, for the child to publish `runtime/run.json` (§2) naming the child's
own pid, while watching for the child to exit first. The **workspace
descriptor** is the handshake, not the registry entry: the launcher knows the
workspace it is launching into, whereas the registry lives under a state
directory it has no way to guarantee is writable. Four outcomes:

- **The child published `running`.** The launcher prints the id and exits `0`.
- **The child published a `finished` descriptor with exit code `0`.** The run
  started, did its work, and ended inside the handshake window. The launcher
  reports it as a run that finished before it returned, and exits `0`: a
  completed plan is not a failed launch.
- **The child exited, or wrote `failed`.** The launcher prints the child's own
  diagnostic — the tail of `runtime/run.log` — and exits non-zero. An invalid
  plan, a held run lock, and an unresolvable agent all fail here, as loudly as
  they do in the foreground.
- **The wait elapsed.** The launcher reports that the run did not report ready,
  names `runtime/run.log` and the child pid, and exits non-zero. It does not
  kill the child: a slow start is not a failed one, and the descriptor and
  `rhei runs` will show what became of it.

The launcher holds a dedicated **`.rhei/headless-launch.lock`** across the whole
stretch — pre-check, truncating `runtime/run.log`, spawning, handshake — and
takes it without waiting. A second launcher on the same workspace fails at once
with a message naming the run that is starting, rather than truncating the first
run's console and then timing out on a child it did not start. The lock is
distinct from the run lock ([§FS-rhei-run.2.6](rhei-run.spec.md#26-run-lock)), which the *child* takes and which
covers every involved execution root — and it is the run lock, not this one,
that catches two launches on different member plans of one project.

A **detached child** never waits on a contended run lock: it fails fast with the
[§FS-rhei-run.2.6](rhei-run.spec.md#26-run-lock) diagnostic, so a lock refusal reaches the operator as a lock
refusal rather than as a 30-second "did not report ready".

The launcher's exit code answers "did the run start?", not "did the run
succeed?". The run's own exit code is recorded in the descriptor and reachable
with `rhei attach --wait` (§5.3).

### 1.2. Human Gates

A detached run **waits at a human gate**, as an interactive TUI run does
([§FS-rhei-run-tui.1.5.7](rhei-run-tui.spec.md#157-liveness-color-and-lifecycle)), rather than exiting the way a non-interactive run
does. Waiting is the point: the operator who releases the gate is expected to
arrive later, through `rhei attach` or the browser dashboard, and a run that
quit at the gate would have nothing left to release. `rhei stop` is the way out
when nobody is coming.

The marker that tells a run it is the detached child describes *that process*,
not its work: it is cleared from the environment of every subprocess the run
supervises. An agent or program that runs `rhei` of its own is an ordinary
invocation, and must not inherit gate-waiting, a control server it did not ask
for, or a refusal to detach.

### 1.3. Platform

Detachment needs a new session, a new process group, and inherited-handle
control. On Unix `--headless` is supported. Elsewhere it fails with a
diagnostic naming `--no-tui` as the nearest available behavior; the flag
remains in `--help` on every platform so the interface does not change shape
between them.

## 2. The Run Descriptor

Every non-dry run — detached or not — publishes `runtime/run.json`:

```json
{
  "id": "3f9a2c",
  "pid": 48213,
  "status": "running",
  "workspace": "/home/u/proj/panta/auth",
  "plan": "/home/u/proj/panta/auth",
  "state_machine": "/home/u/proj/panta/auth/states.yaml",
  "control_url": "http://127.0.0.1:54321",
  "started_at": "2026-08-22T14:03:22Z",
  "headless": true,
  "parallel": 2,
  "log": "/home/u/proj/panta/auth/runtime/run.log",
  "events": "/home/u/proj/panta/auth/runtime/events.jsonl",
  "exit_code": null
}
```

`status` is `running`, `finished`, or `failed`. `exit_code` is **always
present**, and `null` until the run records one — a fixed shape a consumer can
read without having to tell "still running" from "this build omitted the
field". `control_url` is present only while the loopback control server is live
(§4) and is the same address `runtime/dashboard.json` publishes for
`rhei intervene` ([§FS-rhei-viz.5](rhei-viz.spec.md#5-running-execution-view)). `log` is absent for a foreground run, which
has no redirected console of its own.
`state_machine` is present only when the run was given an explicit
`--state-machine`, and is what an attached surface resolves under (§5).

**Every path is absolute.** A descriptor is read by a process standing in some
other directory, so a relative `plan` or `events` names nothing where it is
read. `started_at` is stamped once the run holds its locks, not when the command
was typed: a run that queued behind another began when it got the lock, and
`rhei runs` orders by this field.

The descriptor is rewritten once more when the process ends, with the terminal
`status` and the run's `exit_code`. A run that is `SIGKILL`ed writes nothing
further and is left saying `running` — which is why liveness is decided by the
run lock and not by this field (§3).

Every run also writes a **registry entry** at
`$XDG_STATE_HOME/rhei/runs/<id>.json` (falling back to
`~/.local/state/rhei/runs/`) holding the same object. The registry is what maps
a bare id to a workspace, so `rhei attach <id>` works from any directory. A
failure to write it is a warning, not a silence: the run continues, reachable by
path rather than by id.

**The entry outlives the run.** When the run ends its entry is rewritten with the
terminal status and exit code rather than deleted, because the question
`rhei attach <id>` answers after a run ends — how did it go? — is the one the CI
shape of §5.3 is built on, and an id that stops resolving the instant the answer
exists is useless. Entries are removed only when the workspace no longer names
the run: superseded by a later run, or the workspace descriptor gone from disk.
At most 100 ended entries are kept, newest first.

## 3. Run Identity and Liveness

The run id is the same short identifier the run report already uses
([§FS-rhei-run-report.2](rhei-run-report.spec.md#2-markdown-ui)), so one id names a run in `runtime/run-reports/`, in
`rhei runs`, and in `rhei attach`.

With no reference at all, a command means the enclosing workspace's run, which
is what an operator standing in the project almost always means. A reference
resolves in this order:

1. An **exact id** among the runs not known to have ended.
2. A **path** to a plan or workspace, resolved to that workspace's
   `runtime/run.json`.
3. A **unique id prefix** among the runs not known to have ended.
4. An **exact id** among the runs that have ended.
5. A **unique id prefix** among the runs that have ended.

The first tier is *not known to have ended*, not *live*: an entry whose liveness
could not be decided resolves there too, with a live run winning an exact tie.
A run that cannot be checked is precisely the run an operator needs `attach` and
`stop` to reach, and a reference that stops resolving because a lock file became
unreadable takes both away at the worst moment. Ended runs come last because
they accumulate: a two-character prefix that resolves today would otherwise
start reporting "matches four runs" tomorrow, for runs the operator has
forgotten.

If access to a workspace cannot be checked, listing retains its external
registry entry as **unknown**, names why it could not be checked, and does not
read or mutate that root, declare the run ended, or prune the entry. That entry
remains resolvable by id; an unrelated accessible run remains stoppable by exact
id under its own guards and the existing signal-identity checks. An access
failure must not invent a pending-recovery hop. An established recovery marker,
even with unreadable or corrupt contents, retains the strict refusal in
§FS-rhei-recover.4.

An ambiguous prefix is an error that lists the matching runs rather than picking
one. It lists **at most ten** and then says how many more there are: with a
hundred retained entries, a full listing is not an answer to "which one did you
mean?".

**The run lock is the primary liveness oracle, and stable lock ownership closes
its identity and pathname gaps.** A refused `.rhei/run.lock` proves only that
some process holds the current lock file; it does not identify the supervisor
recorded by a matching non-terminal descriptor. An acquirable or missing
pathname likewise does not by itself prove that the recorded run ended: a lock
belongs to the opened inode, and renaming or unlinking that inode can leave the
live run holding it while the original pathname is absent or names an unlocked
replacement.

On Linux, each acquired run-lock inode records the run id, pid, workspace, and
the process's kernel start identity. After the workspace descriptor agrees on
id and pid, `/proc` proof governs both a held current pathname and a displaced
inode: the same process identity owning an exclusively locked file descriptor
with that exact record is live; a successful inspection that finds the recorded
process gone or not the owner is ended; and an inconclusive inspection is
unknown. Numeric pid existence, executable name, command line, or a coarse
start-time coincidence is not ownership. Platforms without an equivalent
stable ownership probe retain lock-only behavior: a held current lock is live,
a free current lock is ended, and a missing pathname is unknown. Neither rule
overrides terminal status or a workspace descriptor with a different id or
pid. Probing is a **read**: it creates neither the lock file nor the `.rhei`
directory, because a listing must not write into every workspace it inspects.

An ended liveness verdict does not release, replace, or steal a kernel lock
that another process still holds. A new run remains subject to the ordinary run
lock refusal until that holder releases it.

**A refused lock is a held lock, on every platform.** The refusal is spelled
with a different error per operating system — `EWOULDBLOCK` where the lock is a
`flock`, a lock-violation error where it is a mandatory byte range — and the
probe must recognize both as contention. On Linux, exact ownership then decides
whether the recorded run is live, ended, or unknown; elsewhere the held lock is
live. Reading only one spelling turns every running run on the other platform
into an *unknown* entry, which is the one verdict that makes `attach`, `stop`,
and `runs` hedge about a run that is plainly there.

**A released lock is not everywhere released at the same instant.** Where the
lock is a `flock`, the kernel drops it with the descriptor, so a run that has
exited already reads as ended by the time its exit is observable at all. Where
it is a Windows byte range, the handles of a dead process are closed by the
operating system asynchronously, and for an unspecified interval after exit its
lock may still be refused — so a run that is gone can read as *live* for a
moment there. Nothing downstream is corrupted by it: the entry is re-probed,
and the next probe answers *ended*.

**A probe has three answers, not two: live, ended, and *unknown*.** An entry
this process could not read, a workspace descriptor it could not open, a lock it
could not probe — none of those say anything about the run. A missing lock file
is not a free lock, because `flock` survives unlinking; on Linux the stable
ownership inspection decides whether the recorded process owns the displaced
run-lock inode, is gone or not its owner, or cannot be checked conclusively.
Only a *decided* end lets anything be pruned, and only the two conditions of §2
prune at all; an unknown entry is kept and reported (§6). Treating unknown as
death let a momentary `chmod 000`, a full disk, an exhausted descriptor table,
or an inconclusive ownership inspection permanently unregister a run that was
working the whole time.

**Every consumer answers the third case, and none of them answers it "ended".**
Resolution keeps it (above); `rhei runs` lists it separately (§6); `rhei attach`
opens its surface and says what it could not confirm (§5.2). On Linux, `rhei
stop` treats liveness and permission to signal as separate questions: it may
signal an undecided or current-path-live entry only after the exact ownership
proof below succeeds (§7). Platforms without that proof retain the lock-only
stop behavior. The commands that *wait* — `attach --wait`, `attach --json`,
`stop --wait` — keep waiting through an undecided probe rather than reporting an
end that was never observed. Waiting is bounded: after a short grace of
consecutive undecided probes they stop and say so on stderr, exiting non-zero.
A wait that ends must have observed either an end or a failure to check; it must
never report a run it could not check as finished, and a record stream that
stops early must never exit `0` ([§FS-rhei-run-json.2.1](rhei-run-json.spec.md#21-records)).

Reading the registry is a read. Shell completion classifies entries exactly as
the listing does but removes nothing: a tab keypress must not unlink a file.
Completion offers the live and undecided ids first, then the most recent ended
ones, which is the order resolution tries them in.

## 4. The Control Server and the Browser Page

The loopback server that serves the dashboard ([§FS-rhei-viz.7.1](rhei-viz.spec.md#71-dynamic-live-during-rhei-run)) is the run's
**control server**: it carries `/snapshot`, `/intervene`
([§AR-rhei-viz-flow.7](../architecture/rhei-viz-flow.spec.md#7-intervene-the-single-mutation-boundary)), and `/transition-gate` ([§FS-rhei-viz.5.1](rhei-viz.spec.md#51-human-gate-transitions)). The browser
page is one thing it serves, not the reason it exists.

`--headless` therefore always starts the control server, so an attached surface
can intervene and release gates. `--no-dashboard` continues to mean "do not
send me to a browser": on a detached run it withholds the dashboard link, so
nothing invites one, while the control endpoints an attached surface needs stay
up. Turning off a view the operator does not want must not also turn off the
ability to intervene in the run.

**`rhei stop` is not a route.** Stopping a run sends it a signal, exactly as an
operator's Ctrl+C does, so the server keeps the single inbound mutation
boundary it was designed around ([§AR-rhei-viz-flow.7](../architecture/rhei-viz-flow.spec.md#7-intervene-the-single-mutation-boundary)) and stopping inherits the
interruption contract of [§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership) without restating it.

## 5. `rhei attach`

```bash
rhei attach [<RUN>] [--json] [--since <SEQ>] [--wait]
```

`rhei attach` connects the run TUI ([§FS-rhei-run-tui.1.5](rhei-run-tui.spec.md#15-tui-surface)) to a run this process
did not start. It is a **reader of files plus a client of the control server**:

- The plan model comes from the plan on disk, through the same loader the run
  itself uses, and under the **run's own state machine** — the one the
  descriptor records (§2), not whatever the default resolves to now. A surface
  that resolved a different machine would draw states the run cannot be in.
- The runtime overlay comes from `runtime/events.jsonl`
  ([§FS-rhei-run-json.3](rhei-run-json.spec.md#3-durable-event-log)): the client replays the file to rebuild slot state,
  then follows it as the run appends.
- Live agent output comes from the per-task logs named by each
  `slot_assigned.log_path`, tailed directly. That is where the complete
  transcript already is ([§FS-rhei-run-tui.1.2](rhei-run-tui.spec.md#12-live-agent-traffic)), so nothing is duplicated into
  the event log to make attachment work.
- **Intervene** and **human gate release** post to the run's `control_url`,
  through the identical boundaries the browser dashboard and `rhei intervene`
  use. When the run serves no control URL, both actions report themselves
  unavailable rather than failing after the operator has typed.

Several surfaces may attach to one run at once, and a browser dashboard may be
open beside them. None of them is privileged: they are all readers of the same
files and clients of the same two endpoints.

### 5.1. Attaching Does Not Drive

**`Ctrl+C` in an attached surface detaches. It does not stop the run.** This
inverts the driving TUI, where Ctrl+C re-raises `SIGINT` on the run itself
([§FS-rhei-run-tui.1.8](rhei-run-tui.spec.md#18-failure-modes)), and the inversion is the whole point: the reflex that
ends a foreground command must not end a run somebody else's terminal is also
watching. `q` detaches too, at any time, rather than only once the run has
finished.

Stopping an attached run is deliberate and separate: `rhei stop <id>`. The
surface says so — it is labelled as attached, and its action bar names the stop
command rather than implying a key does it.

Detaching leaves the run untouched: no signal, no transition, no journal entry.
The run does not know a surface was there.

### 5.2. Attaching to a Run That Has Ended

`rhei attach` on a finished run does not open a live surface. It reports the
run's recorded result and points at what outlived it — `runtime/run-report.md`
([§FS-rhei-run-report.1](rhei-run-report.spec.md#1-report-artifact)) and the frozen `runtime/dashboard.html`
([§FS-rhei-viz.7.1](rhei-viz.spec.md#71-dynamic-live-during-rhei-run)) — because those, not a screen, are what the operator came
back for.

A run whose liveness could not be decided (§3) is **not** a run that has ended.
`rhei attach` opens the surface anyway and warns once, naming what it could not
check. Reporting a working run as finished is the worse of the two mistakes: the
operator loses the surface, and the run's own output says nothing about it.

### 5.3. `--json`

`rhei attach --json` opens no surface. It writes the run's event records to
stdout in the format of [§FS-rhei-run-json.2](rhei-run-json.spec.md#2-record-envelope), starting from `--since <SEQ>`
(default: the beginning of the run) and following the live run until it ends.
This is how a tool attaches to work it did not start.

`--wait` makes the command exit with **the run's own exit code** once the run
ends, rather than with its own success. It composes the CI shape the launcher
deliberately does not provide on its own:

```bash
id=$(rhei run --headless --json plan.rhei.md | jq -r .id)
rhei attach --json --wait "$id"     # exits with the run's status
```

The id keeps resolving after the run ends (§2), so the wait may also be started
late — or arrive after the answer.

**Only a recorded `0` exits `0`.** A run that recorded *no* exit status did not
end on its own: it was `SIGKILL`ed, OOM-killed, or its machine went away.
`--wait` says so and exits non-zero, because reporting that as success is how a
killed run passes CI.

`--wait` without `--json` opens **no surface at all**. It waits quietly and
prints the same result block an attach to an already-finished run prints (§5.2),
so a wait that outlived its run and an attach that arrived afterwards are
indistinguishable to the reader. Reading no records is also why it is the one
shape that needs no `runtime/events.jsonl`: a run that failed to write its log
(§8) is still a run whose exit status is worth waiting for. The three shapes are
therefore: `attach` is a terminal surface, `attach --wait` is a quiet wait,
`attach --json` is a record stream.

Both waits treat an undecided probe (§3) as "keep waiting", not as an end, and
both give up loudly rather than quietly: past the grace they report on stderr
what they could not check and exit non-zero. For `--json` that also means the
stream is announced as possibly incomplete, because a stream ending without
`run_finished` otherwise reads as an interrupted run ([§FS-rhei-run-json.2.1](rhei-run-json.spec.md#21-records)).

## 6. `rhei runs`

```bash
rhei runs [--json] [--all] [--since <TIME>] [--until <TIME>]
```

Lists the live runs on this machine — id, plan, pid, start time, parallelism,
and control URL — newest first, pruning the registry entries §2 says are
prunable. With no live runs it says so and exits `0`: an empty list is an
answer, not a failure. The address is labelled as the run's **control** URL, not
as a dashboard: under `--no-dashboard` nothing may invite a browser (§4), and
the endpoints an attached surface needs are up either way.

Entries whose liveness could not be decided (§3) are **listed separately, with
the reason**. Keeping them silently and omitting them from the listing produces
exactly the "no runs are live" lie the tri-state exists to prevent.

`--json` emits an array of the live descriptor objects (§2) and follows the
error envelope of [§FS-rhei-errors.5](rhei-errors.spec.md#5-machine-readable-errors); undecidable entries are reported on stderr,
so stdout stays the array and nothing else.

### 6.1. Run History

`--all` also lists the ended runs the registry still holds, newest first, under
a heading of their own, so a finished run is never read as a live one.
`--since <TIME>` and `--until <TIME>` select on `started_at` over the same
half-open interval and in the same time vocabulary `rhei cost` uses
[§FS-rhei-cost-accounting.8.2](rhei-cost-accounting.spec.md#82-selecting-records), and both imply `--all`: a window is a
question about history.

With none of the three, `rhei runs` is exactly what §6 describes — live runs
only, the same pruning, the same empty-list line, and `--json` emitting the bare
array. Under `--all` or a window, `--json` emits an object instead, so the answer
can carry what §6.2 requires:

```json
{
  "schema": "rhei.run-history.v1",
  "runs": [],
  "truncated": null
}
```

### 6.2. What Retention Hid

The registry keeps a bounded number of ended entries (§2). That is enough to
resolve an id after the fact and not enough to answer a question about a window:
the entries past the cap were unlinked and no longer exist to be counted.

So a window that reaches back past the oldest ended entry still held, on a
registry standing at its cap, is a window this machine cannot answer. `rhei runs`
says so rather than presenting what remains as the whole window. The text
listing prints a line beginning:

```text
Retention truncated this window:
```

naming the cap and the earliest start still held; `--json` fills `truncated`
with the same two facts in place of `null`. A total summed over a silently
shortened window is the misleadingly cheap number this surface exists to
prevent. [§FS-rhei-cost-accounting.6.2](rhei-cost-accounting.spec.md#62-coverage-of-a-selection)

## 7. `rhei stop`

```bash
rhei stop [<RUN>] [--kill] [--wait]
```

Sends the run `SIGINT`, entering the interruption contract of [§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership)
unchanged: in-flight invocations are terminated as process groups and reaped,
no ticket transitions, the run report is written, and the run exits
`130`. `rhei stop` adds nothing to that contract and must not: an operator's
Ctrl+C and a `rhei stop` are the same event reaching the run by two routes.

`--kill` *asks twice*: it sends the signal, waits out a short grace, and — if
the run is not known to have ended — sends it again, which is the escalation
[§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership) reserves for an operator who asks twice. It is not a different
mechanism and does not skip the first grace; the run's own handler decides what
a second signal means, exactly as it does for a doubled Ctrl+C. An undecided
probe (§3) does not skip the escalation either, for the same reason it does not
skip the first signal.

`--wait` blocks until the run has actually gone, and then reports its recorded
exit status. Without it, `stop` returns as soon as the signal is delivered,
because delivery is what it promises. An undecided probe keeps the wait going:
returning on one reported a run as ended while its process was still tearing
down the work it was asked to stop.

Stopping a run that has already ended is not an error: it says so and exits
`0`. That short-circuit needs a *decided* end, though. An entry whose liveness
could not be checked (§3) still reaches the pre-signal checks rather than being
reported as ended. On Linux, failure to open a stable process handle decides
that the recorded process is absent only for `ESRCH`; unsupported pidfds,
permission failures, resource exhaustion, and every other open error are
inconclusive and refuse the stop rather than reporting success.

Before every signal, `rhei stop` re-reads the workspace descriptor. A missing
or changed descriptor refuses the signal rather than trusting the registry's
memory of the pid. On Linux, an unreadable descriptor also refuses; the command
then opens a stable handle to the process, revalidates its start identity, and
requires `/proc` to prove that the same process owns an exclusively locked file
descriptor carrying this run's exact structured lock record. The proof applies
whether that descriptor names the current `.rhei/run.lock` inode or a renamed
or unlinked one. Current-path contention establishes only that some process
holds the lock; listing liveness and permission to signal a recorded pid both
require exact ownership on Linux. Missing, mismatched, stale, or inconclusive
ownership refuses the signal with a diagnostic, and the stable
process handle keeps exit and pid reuse between proof and delivery from
redirecting the signal. Numeric pid existence and registry/workspace agreement
alone are never sufficient. Platforms without this stable ownership probe
retain the previous best-effort descriptor re-read and lock-only stop behavior.

## 8. Failure Modes

- **The launching terminal closes.** The run is in its own session and does not
  receive the terminal's `SIGHUP`. It keeps going; `runtime/run.log` keeps
  filling.
- **A second `--headless` run on the same workspace.** It fails at the run lock
  with the existing diagnostic ([§FS-rhei-run.2.6](rhei-run.spec.md#26-run-lock)), synchronously, before an id
  is printed. Two launchers racing each other fail the same way, at the launch
  lock of §1.1, and only one run is ever started.
- **The run dies without cleaning up.** The descriptor still says `running`.
  On Linux, the next `rhei runs` stops listing it as live once exact ownership
  finds the recorded supervisor gone or not the owner, even when another
  process inherited and retains the held lock description. On other platforms,
  the next free-lock probe stops listing it. `rhei attach` reports the run as
  ended. The entry stays until the workspace stops naming the run (§2), so
  `rhei attach <id>` can still say what happened — and `--wait` reports the
  missing exit status rather than inventing `0`. Any independently retained
  kernel lock still refuses a new run until its holder releases it.
- **`runtime/events.jsonl` cannot be written.** A warning on stderr; the run
  continues. The surfaces that read records — `rhei attach` and
  `rhei attach --json` — are then unavailable and say so, naming the file. A run
  that cannot be watched is still a run that works, and still a run
  `rhei attach --wait` can wait for (§5.3).
- **A detached run's `run.log` grows without bound.** It is truncated at run
  start, like the latest run report: one file is one run, and the previous
  run's console is superseded rather than accumulated.
- **Two runs write one `run.log`.** The launch lock of §1.1 serialises
  launchers, not a *live* run's writes: where the pre-check is blind — two
  launches on member plans that share a root, so the refusal is left to the run
  lock — a second launcher truncates the console log a running child is still
  writing to. The log is therefore emptied at launch and **appended** to
  thereafter, so the two consoles can interleave by lines but neither can
  overwrite the other's bytes. A diagnostic that was rendered whole is never
  cut: whatever a reader is shown, including the launcher's own tail of this
  file, names paths that can still be copied and opened.
- **The attached surface's terminal goes away.** The client ends the way a
  driving TUI does ([§FS-rhei-run-tui.1.8](rhei-run-tui.spec.md#18-failure-modes)) — and because it was only ever a
  reader, the run is unaffected.

## Related Specifications

- [Run Command](rhei-run.spec.md) — the execution loop, run locks, and interruption
- [Run JSON Stream](rhei-run-json.spec.md) — the event record contract and `runtime/events.jsonl`
- [Run TUI Specification](rhei-run-tui.spec.md) — the surface `attach` connects
- [Flow Visualization](rhei-viz.spec.md) — the control server's routes and the browser page
- [Per-Run Report](rhei-run-report.spec.md) — the run id and what outlives a run
- [Detached Runs Decision](../decisions/architectural/detached-runs.md) — why a supervisor, not a daemon
