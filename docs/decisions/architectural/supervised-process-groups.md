# DA-supervised-process-groups: Subprocesses are supervised process groups with one termination path

## 1. Status

accepted

## 2. Context

`rhei run` promised a subprocess lifetime it did not own. The spec described
exactly one way an agent dies early — its timeout, `SIGTERM` then a 10-second
grace then `SIGKILL` ([§FS-rhei-agents.7.3](../../functional-spec/rhei-agents.spec.md#73-timeout-behavior)) — and said nothing about the reverse
direction: the supervisor exiting while an agent is in flight.

The implementation matched the omission. Agents and programs were plain
children in `rhei run`'s own process group, `rhei run` installed no signal
handler at all, and the only kill path was the timeout watchdog signalling the
**direct child pid**. Three failures followed from that one shape:

1. **Ctrl+C under the TUI orphaned agents.** In raw mode the tty generates no
   `SIGINT`; the TUI reads the key, restores the terminal, and re-raises
   `SIGINT` on `rhei` alone ([§FS-rhei-run-tui.1.8](../../functional-spec/rhei-run-tui.spec.md#18-failure-modes)). The default disposition
   killed the supervisor and left every agent running. Only the non-TUI case
   worked, and by accident: the tty delivers `SIGINT` to the whole foreground
   process group, which happened to contain the agents.
2. **Every other supervisor death orphaned them too** — `kill <pid>`, `SIGHUP`
   on terminal close, an early `?` return after workers were spawned, a panic,
   `SIGKILL`, OOM. The orphan kept writing into the workspace under its agent's
   permission mode while the state machine that governed it was gone, its
   timeout enforced by nobody, and a restart spawned a second agent for the same
   ticket over the same output paths.
3. **The timeout path had the same bug one level down.** Signalling the direct
   child left the agent's own subprocesses — MCP servers, `bash` tools,
   background jobs — alive after a timeout kill.

The bug is not three bugs. It is one missing concept: nothing named the unit
`rhei run` is responsible for, and nothing gave that unit a single way to end.

## 3. Decision

Every subprocess `rhei run` starts **itself to do a ticket's work** is a
**supervised process group** with exactly one early-termination path and three
reasons to take it: its deadline, an operator interrupt, or the supervisor's
death. That is agents, programs, and the snapshot redactor. [§FS-rhei-run.3.2](../../functional-spec/rhei-run.spec.md#32-interruption-and-process-ownership)

Three subprocesses stay outside the decision, each deliberately. A subprocess a
*callback* starts is that callback's own child and the callback contract
governs it. The `git rev-parse` / `git status` queries of the post-transition
consistency check are short synchronous bookkeeping that has ended by the time
the call returns — a group would give the shutdown nothing to hold. The editor
the browser dashboard launches is detached on purpose: it belongs to the
operator, not to the run, and outliving the run is what it is for.

The redactor is included because "every subprocess" has to mean it: it is a
30-second synchronous child of the run, spawned on the agent path, and leaving
it as a plain child would have left one process the shutdown could not see, one
poll loop that never read the token, and one hard-coded 10-second grace to keep
in step with the other two by hand. Its error semantics are unchanged — a
timeout or a non-zero exit still fails the caller and leaves the ticket where it
is — except that an interrupted redactor now says it was interrupted rather than
that it timed out.

**1. The unit is the group, not the child.**

Each subprocess is spawned with `process_group(0)`, so it leads a group its
descendants inherit, and termination is `killpg`, never `kill` on the child
pid. A subprocess that spawns helpers can no longer outlive its own death
certificate. Because a group in the background must not read the operator's
terminal, a profile that does not pipe a prompt gets `stdin` on `/dev/null` —
independently correct, and required here: an inherited tty read in a background
group stops the child on `SIGTTIN`.

The refusal to start new work is enforced **at the spawn**, not only at the
scheduler. A pass that has chosen a work item still loads the plan, resolves
tooling, composes a prompt, and hands the item to a worker thread before
anything is spawned; a check at the top of that stretch leaves a window in which
a signal still starts an agent under `bypassPermissions`. `Supervised::spawn` is
the one place work actually begins, so it is the one place the rule holds with
nothing in front of it.

**2. Timeout and shutdown are one routine.**

`Supervised::wait` is the only place a subprocess is waited on. It polls
`try_wait` and ends on whichever comes first: exit, deadline, or the stop
token. Deadline and token then run the identical sequence against the group —
`SIGTERM`, grace, `SIGKILL` — and differ only in the cause they report. Three
copy-pasted poll loops, three grace constants, and two ways of killing became
one of each.

The sequence itself is one function over a small target interface — ask, is it
gone, kill — with two implementations: an invocation watching its own child, and
the shutdown guard watching the registry for groups whose children belong to
other threads. Writing the sequence twice meant two graces that could run
concurrently against the same group and signal it twice; a group now records
that it has been asked to stop, so the second arrival adds nothing and waits.

When both a deadline and a shutdown are true on the same poll, the shutdown
wins. They can be: an agent seconds from its timeout when the operator hits
Ctrl+C. Calling that a timeout would fire the timeout transition and rewrite the
ticket the shutdown promised to leave alone.

**3. The token is set by a signal handler, read by the loops.**

One handler for `SIGINT`, `SIGTERM`, and `SIGHUP` does nothing but increment an
atomic and record the first signal number. Everything else is polling: the pass
loop, the worker-pool refill, the scheduler's sleeps, and each live
`Supervised::wait`. A second signal raises the count and skips the grace, which
is what a second Ctrl+C has always meant. The exit code is `128 + signal`
because the run really did end by that signal.

The token counts two things separately, because they are two different facts.
"The run is shutting down — end every wait, start nothing more" is what an
operator's signal and an error unwind both mean, and either raises it. *How
many times the operator asked* is only ever the operator's, and only that count
decides whether the grace is skipped: escalating to an immediate `SIGKILL` is
something a person asks for twice, never something a failed `?` on another
thread arranges on their behalf. Counting both on one number let a single
Ctrl+C plus any teardown kill a group outright, with none of the 10 seconds the
agent was promised to flush and commit.

The shutdown flag is also released when the run that raised it is done, unless a
signal raised it. The flag is process-global while the reason for it was one
run's, and a process can drive more than one — the in-process tests do. Left
standing, it made every later run in the process break out of its first pass and
report success without doing any work. A signal is never released: that one
stopped the process, not just the run.

Because the two facts are separate, so are the two readings. `interrupt_requested`
answers "should this loop stop", which both reasons mean; `interrupted_by_signal`
answers "was this run interrupted", which only the operator's does. Every
statement the run makes about itself — the report's result, the halt diagnostic,
the exit code, the postcondition of [§FS-rhei-run.3.1](../../functional-spec/rhei-run.spec.md#31-git-consistency-after-subprocess-commits) — reads the second, and
reads it once, so they cannot disagree with each other or with a signal that
arrived after the work was done.

The token also composes the operator-facing shutdown notice, once, and hands it
to whichever waiter asked first — as **text**. It performs no I/O: where the
line is legible depends on whether a TUI is on screen, and that is the
frontend's question. Writing it to stderr from the engine was right for a TUI
Ctrl+C, which restores the terminal first, and wrong for an external `SIGTERM`
arriving mid-render, which put it inside the alternate screen. The notice goes
out as an ordinary `Message` event, and `TuiSink` sends warnings and errors to
stderr from the moment it has restored the screen ([§FS-rhei-run-tui.1.8](../../functional-spec/rhei-run-tui.spec.md#18-failure-modes)).

**4. A drop guard covers the paths no handler can.**

A live registry of group ids plus a guard declared alongside `RunReportGuard`
terminates whatever is still registered when `run_agent_mode` is left by *any*
path — `?`, panic unwind, or normal end. Without it, an error return after
workers were spawned is indistinguishable from the original bug. The registry
is process-global but ownership is not: each run claims a fresh id, worker
threads inherit it from the thread that started them, and a guard signals only
the groups its own run started — so it can never reach into work it did not
start.

**5. Interruption is an invocation outcome, not a task state.**

An interrupted invocation fires no transition. The ticket keeps its state, the
task file is not rewritten, the transitions log gains no entry, and the next
`rhei run` re-executes the state. Reporting it as `failed` or `timed out` would
route it through error or timeout transitions and park tickets in states nobody
chose; reporting it as `cancelled` would collide with `cancelled` the terminal
*task state* ([§FS-rhei-run-report.3.2](../../functional-spec/rhei-run-report.spec.md#32-task-tree)). It is its own outcome, `interrupted`,
in the journal, the dashboard, the run report, and the log footer
([§FS-rhei-agents.8](../../functional-spec/rhei-agents.spec.md#8-log-capture)).

**6. PDEATHSIG is a backstop, not the design.**

On Linux each subprocess arms `PR_SET_PDEATHSIG(SIGTERM)` in `pre_exec`. It
reaches the one case no handler can — `SIGKILL` and OOM, where the supervisor
runs no code — and only that case, narrowly: the kernel delivers the signal to
the **direct subprocess** and to nothing below it, so a group leader's own
descendants survive a supervisor `SIGKILL` unless the leader tears them down as
it dies. Group-wide teardown needs the supervisor alive to `killpg`. The
backstop is Linux-only, per-thread, and delivered after a race window the child
closes by re-checking its parent. The handled paths above are the contract;
this is insurance, and it insures less than they cover.

## 4. Alternatives considered

**Detached-and-adoptable** (issue #53's option (b)): record each live agent's
pid, task, and state in `.rhei/run.lock`, and have the next `rhei run` adopt or
reap it. This is a coherent contract, and a much larger feature: it needs pid
reuse handling, ownership transfer of the log and the accounting capture, a
policy for an adopted agent whose plan changed underneath it, and a `rhei run`
that can attach to work it did not start. It also contradicts the timeout
semantics already shipped — a timeout promises the agent is gone, so agents are
already supervisor-owned in the one case the spec described. Supervisor-owned
lifetime is what the rest of the system already assumes; adoption would be a
deliberate new capability, not a bug fix.

**A tether/shim process** between `rhei run` and each agent, which notices the
parent's death and kills its child. It works without any signal handling, and
it buys nothing here that the group plus PDEATHSIG do not: it adds a process
per invocation to the tree, another exit status to interpret, and its own
orphan case (kill the shim, keep the agent).

**`setsid` per agent** (a new *session*, not just a group). What separates it
from `process_group(0)` is session leadership and the controlling terminal —
`SIGHUP` on hangup, and whether the child can open `/dev/tty` — not signal
delivery: the tty sends `SIGINT`, `SIGQUIT`, and `SIGTSTP` to the *foreground*
process group, and `process_group(0)` already moves every subprocess out of it.
After this change no agent receives a tty signal directly on any path; `rhei
run`'s handler decides, on every path. `process_group(0)` is preferred because
it is a safe `std` API applied at spawn time, while `setsid` means a
`pre_exec` closure with its own failure mode, and because the terminal
relationship it would give up is one nothing here uses: stdio is piped to the
log or `/dev/null`, and a subprocess that tried to read the terminal would be
misbehaving.

**cgroups (Linux) / Job Objects (Windows).** The strongest containment
available, and the least portable: a cgroup needs a writable
`cgroup.subtree_control` or a delegated slice, which an unprivileged CLI cannot
assume, and Job Objects solve only the Windows half. Windows keeps
`child.kill()` exactly as before ([§FS-rhei-run.3.2](../../functional-spec/rhei-run.spec.md#32-interruption-and-process-ownership) is specified over the Unix
mechanism); revisiting it is a separate decision, not a prerequisite for
fixing the orphan.

## 5. Consequences

- A timeout now kills the agent's whole tree, not just the agent. Any state
  machine that relied on an MCP server surviving its agent's timeout will see
  it stop — which is the specified behavior, not a regression.
- Agents can no longer read the operator's terminal. A profile that wants
  operator input must pipe stdin (`stdin_prompt` / `intervene_stdin`) and be
  driven through `rhei intervene`.
- `rhei run` exits `130`/`143`/`129` where it used to be killed outright; a
  wrapper that treated any non-zero exit as a plan failure now sees a distinct,
  conventional code.
- An interrupted run leaves tickets exactly where they were. The run report
  says which invocations were interrupted and where their logs are, and re-running
  is the whole recovery procedure.
- **Not covered:** stragglers a group leader leaves behind when it exits
  *normally*. Only a timeout and an interruption tear the group down; a leader
  that returns `0` having failed to stop an MCP server it started leaves that
  server in the group, and nothing here kills it. The group is a handle for
  ending an invocation early, not a lifetime the runtime enforces at every exit.
  Making it one — reaping the group after every wait — is a separate decision
  with its own hazard: a state machine may legitimately want a helper to outlive
  the invocation that started it.
- **Not covered:** a descendant that calls `setsid()` and leaves the group — a
  daemonizing MCP server, say. It is outside the `killpg` from the moment it
  does so, and survives both a timeout and an interruption. This is the same
  limitation `PR_SET_PDEATHSIG` has, for the same reason: nothing short of a
  cgroup or a Job Object can hold a process that walks out of its container.
  Containment that a subprocess cannot leave is the cgroup/Job Object decision
  above, deferred.
