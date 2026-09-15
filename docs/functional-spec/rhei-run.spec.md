# FS-rhei-run: `rhei run`

Drive a plan end-to-end by repeatedly claiming the next ready task, spawning the state's agent or program, waiting for completion, and performing the resulting transition. `rhei run` operates under `orchestrator` authority: the orchestrator — not the spawned subprocess — owns every state transition. See [Agents Specification — Completion Authority](rhei-agents.spec.md#31-completion-authority) for the full authority contract.

This document specifies the command contract and execution loop. The live terminal UI is specified separately in [rhei-run-tui.spec.md](rhei-run-tui.spec.md).

## 1. Usage

```bash
rhei run <RHEI_PLAN_OR_WORKSPACE> [flags]
```

## 2. Options

Flags are grouped by concern:

### 2.1. Standalone

| Flag                     | Default | Description                                                                |
|--------------------------|---------|----------------------------------------------------------------------------|
| `--dry-run`              | false   | Print the sequence of transitions that would be made without executing them |
| `--no-callbacks`         | false   | Skip execution of `on_leave` / `on_enter` callbacks                        |
| `--continue-on-error`    | false   | Continue to the next task when an agent or program exits non-zero          |
| `--parallel <N>`         | 1       | Maximum number of agents or programs to run concurrently (0 = unlimited)   |
| `--prices <PATH>`        | built-in | Price measured agent usage with a validated local price book and copy it into the run's accounting roots. See [§FS-rhei-cost-accounting.5.1](rhei-cost-accounting.spec.md#51-price-book-selection). |
| `--rhei <RHEI_ID>`       | all     | Narrow this run to the named rheis (repeatable). See §2.5.                  |
| `--tui`                  | auto    | Force TUI mode even when stdout is not detected as a TTY                   |
| `--no-tui`               | auto    | Force plain stdout output even when stdout is a TTY                        |
| `--json`                 | false   | Emit the run as a JSONL event stream on stdout. See [Run JSON Stream](rhei-run-json.spec.md) |
| `--headless`             | false   | Detach the run into its own session and print its run id. See [Detached Runs](rhei-run-headless.spec.md) |
| `--dashboard`            | auto    | Serve the loopback browser dashboard for this run                          |
| `--no-dashboard`         | auto    | Disable the loopback browser dashboard                                     |

### 2.2. Agent Execution

| Flag                    | Description                                                             |
|-------------------------|-------------------------------------------------------------------------|
| `--no-agent`            | Disable agent spawning; use callback-only advancement                   |
| `--agent <AGENT>`       | Override the agent for this run                                         |
| `--agent-mode <MODE>`   | Override the agent mode (named flag set) for this run                   |
| `--model <MODEL>`       | Override the model for this run                                         |

### 2.3. Snapshots

| Flag | Description |
|------|-------------|
| `--from-snapshot <ref>` | Override the concrete source selected by an authored `snapshot.inherit:` after that state's constraints are applied. See [Snapshot Operations Specification — Run Override](rhei-snapshot-operations.spec.md#2-run-override). |
| `--override-inherit` | Explicitly bypass authored source-selection and compatibility constraints for an ad-hoc debug run. The target state must still declare `snapshot.inherit:`. Requires `--from-snapshot`. |
| `--task <id>` | Select the task for an ambiguous snapshot override. |
| `--target <slug>` | Select the fanout target for an ambiguous snapshot override. |

### 2.4. Program Execution

| Flag                           | Description                                                                      |
|--------------------------------|----------------------------------------------------------------------------------|
| `--no-program`                 | Disable program spawning; use callback-only advancement for program states       |
| `--program-timeout <DURATION>` | Override the program timeout for this run (applied per program state)            |

### 2.5. Project Scope (`--rhei`)

`rhei run` drives a whole project by default: every load yields a Panta-rooted
graph, and a bare rhei is simply the single rhei of its implicit Panta
([§FS-rhei-panta.6.2](rhei-panta.spec.md#62-rhei-run)). `--rhei <RHEI_ID>` is repeatable and narrows the run to
the named rheis. With no `--rhei`, successfully published project members may
join the candidate set while the run is live (§3). With one or more `--rhei`
flags, the candidate set is exactly the member ids resolved at startup: later
members still participate in strict graph validation, dependency resolution,
and project-wide locking, but never become candidates in that run.

- An id that names no rhei in the project is an error listing the available
  rhei ids.
- Narrowing selects **candidate** tickets only; it never narrows where their
  priors resolve. A candidate may still be blocked by a prior in a rhei outside
  the scope, and the no-work diagnostic names that prior as out of scope
  ([§FS-rhei-panta.6.1](rhei-panta.spec.md#61-readiness-and-rhei-next)).
- Before spawning, `rhei run` reports its resolved scope and the rheis it will
  touch, using the shared scope line:

  ```text
  Scope: `rhei run` narrowed to <N> rheis: <ids>
  ```

  A one-rhei project has no fan-out to report and stays quiet
  ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)). Because a TUI run takes over the screen right after
  launch, a narrowed run repeats the scope in the run journal
  (`Scope: narrowed to <ids>`), where the interactive view can show it.
- `--parallel > 1` stays available on a project, but two tickets of one rhei
  file are still concurrent work against a single checkout; the run warns and
  names such a file. Plan-file writes themselves serialize on the file lock.
  Only **top-level tickets** count toward a file: a subtask always executes
  inside its ticket's slot, so a file holding one ticket and its subtasks is
  not shared. When *every* ticket in the run lives in one plan file, parallel
  slots could only ever schedule same-file tickets — the run then falls back
  to sequential execution with a warning, exactly as for a bare single-file
  plan.

### 2.6. Run Lock

At most one live run may drive a rhei's files at a time. A run acquires the
`.rhei/run.lock` of **every** involved execution root — the target's own root
plus each contained rhei's execution root — not just the root it was pointed
at. This is what makes a project-level `rhei run <project>` and a direct
`rhei run <project>/<rhei>` mutually exclusive: both contend on the member
rhei's lock. Locks are acquired in one canonical (sorted, absolute) order so
two multi-root runs cannot deadlock. `--dry-run` takes no locks
(§4). Narrowing with `--rhei` does not narrow the lock set: a narrowed run
still locks the whole project, keeping lock behavior independent of scheduling
scope.

When a strict reload discovers a published member whose execution root was not
known at startup, the run acquires that root's `.rhei/run.lock` before admitting
the member. This applies even when an explicit `--rhei` keeps the member out of
the candidate set: scope never weakens exclusion. New roots are added in the
same canonical order and use the same announced, interruptible foreground wait
or immediate detached-child refusal as startup locks. The project lock remains
held throughout; `--dry-run` remains lock-free and makes no live-admission
promise.

A **foreground** run blocks on a lock another run holds — waiting for your turn
is a queueing idiom people use on purpose — but says so first, naming the run it
is waiting for. Blocking in total silence is indistinguishable from a hang. The
line goes to stderr when stdout is a record stream ([§FS-rhei-run-json.1](rhei-run-json.spec.md#1-selecting-the-format)). The
wait is **interruptible**: `Ctrl+C` ends it at once, reporting that it stopped
waiting and leaving the run that holds the lock untouched. A wait a command
announces has to be one the operator can take back, and a wait parked inside a
blocking `flock` cannot see the signal at all (§3.2). A **detached child** does
not queue: it fails immediately with the diagnostic above, because its launcher
is holding a startup handshake open and would otherwise report a timeout for
what is really a lock refusal ([§FS-rhei-run-headless.1.1](rhei-run-headless.spec.md#11-startup-is-synchronous)).

The lock is also the primary answer to *"is this run still alive?"* for a run
nobody is watching. Because the lock belongs to an opened inode rather than its
pathname, matching non-terminal registry and workspace descriptors stay live
when the held inode is renamed or unlinked only where the recorded process's
stable identity and ownership of that displaced lock can be proved. The two
descriptors must agree on both run id and pid; terminal and superseded identity
still take precedence
([§FS-rhei-run-headless.3](rhei-run-headless.spec.md#3-run-identity-and-liveness)).

### 2.7. Run Identity

Every non-dry run publishes a **run descriptor** naming its id, pid, workspace,
control URL, and status, and a durable **event log** of everything it emitted.
They are what let a separate process watch, attach to, or stop a run it did not
start — see [Detached Runs](rhei-run-headless.spec.md) and
[Run JSON Stream](rhei-run-json.spec.md). A foreground run publishes them too:
the identity belongs to the run, not to `--headless`.

## 3. Execution Loop

`rhei run` runs passes until no further forward progress is possible:

Mode selection: `rhei run` uses orchestrated subprocess execution whenever any reachable non-terminal, non-gating state declares autonomous work via `program`, `agent`, `target`, `all_targets`, `model`, or `all_models`. Callback-only advancement is entered only when no such state exists, or when the caller explicitly disables spawning with `--no-agent` and/or `--no-program`. If a state declares model/target-driven work but no agent transport resolves, `rhei run` fails with a missing-agent configuration error; it does not silently fall back to callback-only transitions for that state.

For mode selection only, a spawn-enabled program poll that satisfies every
ordinary readiness, scope, and selection constraint except that its persisted
poll deadline is still in the future counts as reachable autonomous work. The
deadline continues to exclude the task from the ready set until it elapses; it
delays the next subprocess attempt rather than hiding that attempt from mode
selection.

The built-in `pending` -> `completed` machine is manual-only, not
callback-complete work. If a ready task under that built-in machine is in its
profile's initial `pending` state, `rhei run` must fail without changing the
task. The manual worker loop must claim such a task with `rhei next`, do the
work, and finish it with `rhei complete`. This prevents the built-in machine
from silently completing fresh tasks without executing them.

Every reload made for scheduling is also a live-member admission checkpoint.
The run performs one after processing each completed worker, before choosing
the next sequential task or refilling a parallel slot, and once immediately
before its normal no-work or gate stopping decision. At a checkpoint the run
strictly reloads the complete project and compares its discovered member ids
with the initialized set. A membership delta becomes eligible only after all
of the following succeed as one admission: merged settings reload; member
machine and callback-base resolution; complete graph and execution-reference
validation; acquisition of every new execution-root lock; and initialization
of the member's execution and accounting roots. Whole-run state-machine,
agent, and model overrides apply normally. An incompatible member fails
admission instead of inheriting an already initialized member's machine or
runtime paths.

Admission does not introduce a directory watcher or an unbounded idle wait. A
member visible in the final checkpoint snapshot is considered before stopping;
a member published after that snapshot waits for another run. Existing
interactive and headless gate waits keep their existing reload cadence. A
producer may therefore instantiate a follow-on member as its final program
action and have that work considered by the enclosing run, while a future
publication alone cannot keep a run alive indefinitely. Admission and failure
messages use the existing run-message and event-journal channel on text, TUI,
headless, and JSONL surfaces; no new output record family is introduced.

1. Load the state machine and plan. Validate. Errors stop the run; the
   initial report's validation **warnings** ([§FS-rhei-validate.4](rhei-validate.spec.md#4-behavior)) are emitted once,
   in report order and in the same words `rhei validate` prints them. After the
   selected frontend emits `RunStarted` and before scheduling begins, the run
   emits each through `RunEvent::Message` at warning level. Plain and headless
   frontends write it to stderr; JSON writes one `message` record with
   `level: "warn"`; TUI, dashboard, and the durable event log retain that same
   event. A warning does not change run success. This validation scope and
   frequency are the same for ordinary, `--parallel`, and `--rhei` execution.
2. Scan all task nodes, including child and grandchild tasks, and compute the
   *ready set*: tasks all of whose descendants are terminal, whose `**Prior:**`
   are all in successful terminal states
   (terminal and not `cancelled`), whose current state is non-terminal and
   non-gating, and whose current state's required `inputs:` all exist. Task
   counts, terminal counts, final state summaries, and remaining-work checks
   use the same full task tree. Tasks whose current state declares `poll:` and whose
   `metadata.tasks.<id>.pollNextAttemptAt.<state-name>` is later than the
   current wall-clock time are excluded from the ready set until the interval
   elapses. A task with an applicable provider-limit record (§3.3) is also
   excluded until its `nextAttemptAt`; queued work sharing that record's
   identity is suppressed for the same interval. See [Next Command](rhei-next.spec.md#3-default-behavior-claim-mode)
   for the manual claimability rule and [Polling States](#51-polling-states) for
   the poll scheduling rule.

   The descendant condition is the same eligibility rule `rhei next` applies
   ([§FS-rhei-next.3](rhei-next.spec.md#3-default-behavior-claim-mode)), and it is deliberately shared: a non-leaf task is a task
   in its own right, so the orchestrator schedules it — but only once the
   subtree it integrates is finished ([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)). A parent with
   an agent state is therefore *not* spawned concurrently with its children;
   it is spawned after them. Nothing stamps a parent terminal because its
   descendants are, and a run that tried to would be rejected by the
   descendants-first guard on the shared transition path
   ([§FS-rhei-transition-cmd.3.1](rhei-transition-cmd.spec.md#31-descendants-first-on-terminal-entry)) rather than leaving behind a plan that fails
   `rhei validate`.

   A task in a *supervising* state, and every descendant of one, follow the
   hold/release rule instead of the descendant condition
   ([§FS-rhei-supervision.3.2](rhei-supervision.spec.md#32-readiness)): the supervisor is ready while its subtree is
   held and nothing beneath it is in flight; its descendants are ready only
   while every supervising ancestor has released them.
3. Up to `--parallel` tasks from the ready set are executed concurrently, subject to the [concurrent-state rule](#5-parallel-execution): at most one ready task per non-concurrent state is scheduled per pass. For each task:
   - Resolve the state's target: either an agent subprocess (`agent` or resolved target selector) or a program (`program`).
   - If the state declares `snapshot.inherit:`, resolve and preload the source snapshot before spawning the agent. Polling states reject `snapshot.inherit` in v1. See [Snapshots Specification](rhei-snapshots.spec.md).
   - Compose the agent prompt ([Agents Specification — Prompt Composition](rhei-agents.spec.md#3-prompt-composition)). A prompt that cannot be composed — a `required: true` handoff with no content, an unreadable prior result — fails **that task**, not the pass: `rhei run` reports the task and the reason, then applies the same rule as any other task failure, continuing to the next task under `--continue-on-error` and aborting with a non-zero exit code without it. Sibling tasks already spawned in the pass are unaffected.
   - Spawn the subprocess with the state's resolved instructions, remaining
     transport and invocation-detail environment variables defined in
     [Agents Specification — Environment Variables](rhei-agents.spec.md#4-environment-variables), checkout-root working directory, and timeout.
   - Wait for the subprocess to exit, for the timeout to fire, or for the run to be interrupted. Each subprocess runs in its own process group and is terminated as a group — `SIGTERM`, grace 10 s, then `SIGKILL` — whichever of the three reasons ends it (§3.2).
4. On subprocess exit, evaluate the state's [Completion Condition](rhei-agents.spec.md#32-completion-condition): exit code `0` plus every required `outputs:` artifact present on disk. When the transition this exit would select lands on a `final: true` state, the ticket's non-empty `runtime/results/<task-id>.md` is one more required artifact of that condition ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)) — the subprocess is the worker that knows why the ticket is finishing, and it was told the path in its prompt ([§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition)), or, for a program, in `RHEI_RESULT_PATH` ([§FS-rhei-programs.2](rhei-programs.spec.md#2-environment-variables)).

   That last artifact is owed on a **declared route** as well as on a zero exit:
   a non-zero exit firing an exact `exit_code:` match is the program choosing
   the edge ([§FS-rhei-programs.3.2](rhei-programs.spec.md#32-evaluation-order)),
   so it is a worker speaking for itself and the engine has nothing to write in
   its place. Only that half of the condition extends. The state's declared
   `outputs:` are checked on a zero exit and skipped on a non-zero one, before
   a declared route as after it.
5. Select the outgoing transition without applying it yet.

   - **The condition holds, or the exit named its own edge.** Select the first
     declared transition whose `condition` / `exit_code` matches. A non-zero
     exit that fires an *exact* `exit_code:` match — an integer or an
     integer-array condition
     ([§FS-rhei-programs.3.2](rhei-programs.spec.md#32-evaluation-order)) —
     belongs in this branch and not the next one: it is a **declared route**,
     the program's way of saying where the ticket goes, so the engine takes the
     edge and writes no result entry of its own
     ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)).
   - **The engine ended the work, or the exit named no edge.** Its timeout
     fired; or the subprocess exited non-zero and matched no transition, or
     matched only a `"nonzero"` catch-all, or spent a poll state's attempt
     budget. Route through the
     state's error or timeout transition per
     [Agents Specification — Execution Loop](rhei-agents.spec.md#52-execution-loop).
     The error route selects only a transition the state declares with
     `exit_code` (§FS-rhei-programs.3.2), except a poll state's exhaustion
     edge: once its attempt budget is spent, the first matching non-self-loop
     transition is selected regardless of `exit_code` (§5.1).
     When no such transition is declared and `--continue-on-error` is unset,
     `rhei run` aborts with a non-zero exit code. These outcomes are the
     engine's own, so the engine supplies the ticket's result message for them
     ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)).
   - **The completion condition fails** — the subprocess exited `0` and a
     required `outputs:` artifact is missing, or the edge this exit selects
     lands on a `final: true` state and the ticket has no result
     ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)). The
     second half holds for a declared route as well: an exact `exit_code:`
     match into a `final: true` state is the program's own answer, and the
     engine writes nothing in its place, so a program that leaves
     `RHEI_RESULT_PATH` empty stalls here
     ([§FS-rhei-programs.2](rhei-programs.spec.md#2-environment-variables)).
     **No transition fires.** The ticket stays in the
     state it is in, the engine logs the missing-artifact warning of
     [§FS-rhei-agents.3.2.1](rhei-agents.spec.md#321-runtime-semantics) naming every path it checked — the result under the
     artifact name `result`, so the operator sees which file the run is waiting
     for rather than a ticket that silently stopped advancing — and records the
     ticket as halted on missing outputs for the run report
     ([§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout)). The stalled ticket is not spawned again for the
     rest of the pass, and the run **continues with the other claimable
     tickets**: in sequential mode (`--parallel 1`) exactly as in the worker
     pool, and with or without `--continue-on-error`, which governs non-zero
     exits and has nothing to say here. One ticket's failure to finish its own
     work is never a verdict on the tickets beside it. The run halts only when a
     pass makes no progress at all (step 9), and then exits non-zero with every
     stalled ticket named.

     The recovery is to **run the state again**, and only that. A later pass
     that reaches the ticket — this run's, or the next `rhei run`'s — schedules
     the same invocation, because step 3 skips an invocation only when the whole
     completion condition already holds for it ([§FS-rhei-agents.3.2](rhei-agents.spec.md#32-completion-condition)). The engine
     never advances the ticket instead: doing so would put it in a `final: true`
     state with no account of the work, and the only sentence the engine could
     write there would be about a worker it did not watch.

     Running the state again is bounded. One visit to a state — the span
     between two consecutive moves of the ticket — may be spawned at most
     `attempts` times ([§FS-rhei-agents.3.2.3](rhei-agents.spec.md#323-attempt-budget)), a budget that is persisted with
     the visit and therefore survives the end of a run: a fresh `rhei run` does
     not buy the ticket a fresh allowance, because "once per run, forever" is
     exactly the unbounded case the budget exists for. Entering the state again
     is a new visit and does bring a fresh budget. When the budget is spent the
     ticket stalls here, through this same path and no other: it stays in its
     state, is out of the running for the rest of the run, is named in the halt
     with the attempts it spent and the artifact it still owes, and the run's
     exit code is the one step 9 gives any run that ends with tickets
     unfinished. No transition fires on an exhausted budget — an error edge, a
     timeout edge, or a move to `cancelled` would record a verdict on work the
     engine never saw.
   - **The subprocess exited `0` in a supervising state and the visit released
     nothing.** The completion condition held, but the edge it selects is the
     supervisor's own self-loop and the visit neither moved the subtree nor
     left it able to move ([§FS-rhei-supervision.3.6](rhei-supervision.spec.md#36-empty-visits)). **No transition
     fires**, by the path above and with the same consequences: the ticket
     keeps its state, the visit is not spent, the engine warns naming the
     descendants left with nowhere to go, and the run carries on with the
     tickets beside it. Firing the self-loop would release the subtree on the
     strength of a visit that released nothing, and a released supervisor is
     woken only by a descendant that moves — so a subtree that cannot move
     would leave the run beyond the reach of a rerun rather than merely
     stalled.

     This is the one stall that does **not** charge the attempt budget above.
     The budget bounds a worker that could not answer for itself; this worker
     answered, and the engine withheld its edge, so the attempt is given back
     and a fresh `rhei run` does visit the state again. "Once per run, forever"
     is accepted here because the alternative is worse than unbounded: a
     supervisor bars its subtree while it is held, so a spent budget would make
     the workspace unrecoverable rather than merely slow, and within a run the
     ticket is re-visited only after something else advanced
     ([§FS-rhei-supervision.3.6](rhei-supervision.spec.md#36-empty-visits)).
6. For agent invocations, extract measured usage and write the accounting
   invocation record when the resolved agent supports accounting. Accounting
   failures affect cost coverage but do not alter transition selection. [§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)
7. For agent-bearing states with supported snapshot sessions, write
   auto-emitted `_state` snapshots and any matching named `snapshot.emit:`
   after transition selection and before the transition is applied. Poll
   self-loop attempts do not emit because the selected transition is known;
   terminal poll exits may emit. See
   [Snapshots Specification — Emit on Exit](rhei-snapshots.spec.md#102-emit-on-exit).
8. Apply the selected transition and append one central state-transition entry
   to `runtime/state-transitions.log` as `<task-id> <from>@<to>`.

   **One selected transition does not reach this step: a poll state's
   self-loop.** Step 5 may select it, but the engine has already handled it by
   scheduling the next attempt and releasing the slot
   ([§FS-rhei-states.2.2](rhei-states.spec.md#22-semantics)), so there is no move to apply and no entry is
   appended — the task is in the state it was already in. The pass stops working
   that task and continues with the rest. Every other selected transition,
   including a poll state's exhaustion edge, is applied here.

   When the moved task has a supervising ancestor, the shared path records the
   checkpoint on the nearest one and holds its subtree ([§FS-rhei-supervision.2](rhei-supervision.spec.md#2-checkpoints)). The
   subprocess **must not** call `rhei transition` or `rhei complete`; the
   orchestrator owns the transition. When the effective target is `final:
   true`, the transition passes the terminal-result obligation
   ([§FS-rhei-transition-cmd.3.2](rhei-transition-cmd.spec.md#32-terminal-result-on-entry)) on the shared path like any other verb, and
   terminal result finalization is performed as defined in
   [Complete Command — Result File](rhei-complete.spec.md#3-result-file).
9. Repeat until no pass makes progress. Exit `0` when the plan reaches a state where every task is terminal. Exit non-zero when progress halts with non-terminal tasks remaining and no further advancement is possible.

   This is the **pass loop's** bound, and it is not the attempt budget of step
   5. The two answer different questions and neither substitutes for the other:
   the budget bounds how many times one visit to a state may be spawned, across
   runs; the pass loop bounds how long *this* run keeps trying, given what its
   passes achieve.

   A ticket that stalled under step 5 is out of the running for the rest of that
   pass. A pass that moved *something* — any ticket, any transition — does not
   release those tickets; it records that the run has made progress since the
   last release, and the pass loop goes on to the tickets that are still
   claimable. A pass that ends with a ticket newly stalled and other claimable
   tickets still untried also continues, since it has not yet asked everything
   it could ask.

   The release happens at the one moment the run would otherwise stop: a pass
   that moved nothing and has no untried ticket left. If some earlier pass had
   made progress, every stalled ticket is released and given another turn, and
   the run continues; if that turn moves nothing either, the run ends and names
   every ticket still stalled. So the allowance is not one extra pass per run —
   it renews every time the run makes progress — and it is not a bound on how
   often one ticket may be re-spawned. That bound is the attempt budget above,
   which is per state visit and outlives the run.

### Who supplies the result on a terminal edge

`rhei run` never invents one. Each route says who does:

| Route | Terminal result comes from |
|-------|-----------------------------|
| Agent or program exits `0` and the selected edge is terminal | The subprocess, which wrote `runtime/results/<task-id>.md` before exiting. Missing, and step 4 fails the completion condition (no transition, task stays put). |
| A **fanned-out** state (`all_targets` / `all_models`) whose selected edge is terminal | Every invocation, each into its own fragment `runtime/results/<task-id>/<state>/<visit_count>/<identity>.md`; the completion condition checks the invocation's own fragment, and once the last fragment lands `rhei run` merges them into `runtime/results/<task-id>.md` before applying the transition, idempotently ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)). One worker's account never stands in for another's, and no invocation overwrites a sibling. A `program:` state is not fanned out: it runs once and writes the ticket-level file. |
| Timeout ([§FS-rhei-agents.7.3](rhei-agents.spec.md#73-timeout-behavior)) | The engine, which knows the timeout that ended the work and writes it as the result message. |
| Unavailable required tooling ([§FS-rhei-agents.6](rhei-agents.spec.md#6-missing-tooling)) | The engine, which names the kind and the unavailable ids. |
| Non-zero subprocess exit that named no edge — an error transition, an unmatched code, or a `"nonzero"` catch-all | The engine, which names the exit code. |
| Non-zero subprocess exit that fired an **exact** `exit_code:` match ([§FS-rhei-programs.3.2](rhei-programs.spec.md#32-evaluation-order)) | The subprocess. The exit is a declared route rather than a failure the engine could narrate, so the ticket's result is the program's to write (`RHEI_RESULT_PATH`, [§FS-rhei-programs.2](rhei-programs.spec.md#2-environment-variables)). Missing, and step 4 fails the completion condition exactly as it does on a zero exit. |
| Callback-only advancement (`--no-agent`, or a machine with no autonomous state) | A callback that wrote the result file, if one did — otherwise the engine, which records that it took the edge itself and that **no worker result was recorded**. What it says about the worker is what it can prove: with a spawn record for the source state on disk ([§FS-rhei-agents.8.4](rhei-agents.spec.md#84-spawn-records)) the sentence names the worker that ran — `agent '<id>'` or `program \`<command>\`` — its log, and how it ended; only with no such record does it say that no worker ran. |
| Human gate released from a live surface — browser dashboard ([§FS-rhei-viz.5.1](rhei-viz.spec.md#51-human-gate-transitions)) or TUI ([§FS-rhei-run-tui.1.5.5](rhei-run-tui.spec.md#155-live-actions-intervene-and-human-gate)) | The human who released it, through the gate surface's own optional **Result** field. The message rides the transition like `rhei transition --result` does. Left blank with no result on disk, a release into a terminal state is refused, and the refusal names `rhei transition <id> --from <state> --to <state> --result "<why>"`. Releasing a gate into a non-terminal state is unaffected either way. |

The line the table draws is one rule: **the engine writes a result only for the
outcomes the engine itself produced** — a timeout it fired, tooling it could not
start, an exit code it read that no declared route matched, an edge it walked
with no subprocess in the state.
It never speaks for a worker that ran, and it never speaks for a human. Where a
worker ran, a missing result is a failed completion condition, not a sentence
the engine makes up; where a human decided, the human is asked — which is why
the gate surfaces have a field to answer in rather than a refusal to work
around.

The callback-only sentence is a fallback, not boilerplate: it is written only
when the edge really did land on a `final: true` state and the ticket has no
result of its own, so a callback that writes one is never overwritten or
contradicted. Recording "no worker result was recorded" is the point — it is
the fact the old, empty result file withheld, and the reason the audit trail
used to depend on which verb drove the plan.

The clause about the worker is checked, not assumed, and what it is checked
against is the spawn record of [§FS-rhei-agents.8.4](rhei-agents.spec.md#84-spawn-records) — never the presence of a log
file. A log is opened, and its header written, *before* the subprocess starts,
so a `command:` naming a binary that does not exist leaves a log behind for a
worker that never ran; recording that such a worker "ran in that state earlier"
would be the same class of lie as recording that none did. A spawn record is
written when a subprocess **ends**, so its presence is proof one ran.

Where a record is found the recorded sentence names the worker — the agent id,
or the program's command — the log it wrote, how it ended, and that it wrote no
result: the fact the reader needs, and the opposite of what "no agent ran" would
have told them. The record is matched by its `task` and `state` fields, so a
state never inherits the account of a state whose name it is a prefix of. With
no record, the sentence says that no agent or program ran in that state, which
is then true.

The account also says whether the state's declared `outputs:` were verified on
this edge — not asserted, reported: the check either ran and passed immediately
before the result was recorded, or it was waived because the edge lands on the
reserved `cancelled` state ([§FS-rhei-states.1.4](rhei-states.spec.md#14-reserved-state-names)), and the sentence says which.
A state that declares no `outputs:` has nothing to report and the clause is
omitted.

The accounting record is deliberately not the evidence: it is written only for
agents that support accounting (step 6), so a missing one proves nothing.

`rhei run` does not transition out of [gating states](rhei-states.spec.md#12-per-state-fields) — exiting one requires an explicit human-initiated `rhei transition` call.

Gating states are a barrier, not an immediate global abort. If one task enters a
gating state while other non-gating tasks are already running, or while other
independent non-gating tasks remain ready, `rhei run` lets that remaining
non-gating work finish. The run halts for human input only when the remaining
non-terminal tasks are either themselves in gating states or blocked behind a
gating dependency. In other words: a gate waits for everyone else to complete,
then stops autonomous progress at the boundary.

### 3.1. Git Consistency After Subprocess Commits

The orchestrator-owned transition in step 8 is a durable-state write to the
authored plan or workspace task file. If a subprocess creates a Git commit
before that write, the commit cannot include the later Rhei-owned transition
without violating orchestrator authority.

When a non-dry-run execution starts inside a Git repository, `rhei run`
records the starting `HEAD`. If the final success path observes that `HEAD`
changed during the run, it must inspect tracked changes under the plan input
and `runtime/results` before returning success. The path check resolves the
actual plan or workspace path independent of the operator's current directory,
so `rhei run plan.rhei.md` from a subdirectory and `rhei run
path/to/plan.rhei.md` from the repository root are equivalent for this
postcondition.

If any of those Rhei-owned tracked paths remain dirty, `rhei run` exits
non-zero with a diagnostic that names the paths instead of silently reporting a
durable success. This check is read-only: it does not create commits, stage
files, or reject untracked runtime artifacts. Outside Git repositories, or
when `HEAD` does not move during the run, the check is a no-op.
[§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

### 3.2. Interruption and Process Ownership

`rhei run` owns the lifetime of every subprocess it starts. There is one
early-termination path and three reasons to take it: the invocation's own
deadline, an operator interrupt, and the supervisor's death. Timeout and
shutdown are two triggers of the same routine.

**What this covers.** Every subprocess `rhei run` starts itself to do a
ticket's work: agents, programs, and the snapshot redactor
([§FS-rhei-snapshots](rhei-snapshots.spec.md#fs-rhei-snapshots-rhei-session-snapshots-specification)). Three are deliberately outside it: a subprocess a
*callback* starts is that callback's own child and is governed by the callback
contract; the `git` queries of the consistency check of §3.1 are short
synchronous bookkeeping that has ended before the check returns; and the editor
the browser dashboard launches ([§FS-rhei-viz.5](rhei-viz.spec.md#5-running-execution-view)) is detached on purpose, because
it is the operator's and outliving the run is the point.

**Process groups.** Each such subprocess starts in its own process group, which
its descendants inherit — MCP servers, shell tools, background jobs.
Termination signals the **group**, never the direct child alone, so an
invocation cannot leave live processes behind by handing its work to a
grandchild. A subprocess never inherits the operator's terminal on standard
input: one that is not handed piped input gets `/dev/null`, so no agent
competes for the keystrokes meant for `rhei run`.

**One termination sequence.** A timeout ([§FS-rhei-agents.7.3](rhei-agents.spec.md#73-timeout-behavior)) and an
interruption both terminate the group with `SIGTERM`, a 10-second grace, then
`SIGKILL`. The invocation is reaped and its log footer closed either way. Every
early termination takes this sequence, including the ones no waiter reached —
an invocation abandoned by an error between its spawn and its wait is asked to
stop and given its grace like any other, because nothing about the way the run
is leaving makes that group less entitled to flush its work and unlink its
temporary files.

**A shutdown outranks a deadline.** An invocation can reach both at once — one
seconds from its timeout when the operator hits Ctrl+C. That invocation is
**interrupted**, not timed out: it fires no timeout transition, and the ticket
keeps the state a shutdown promised to leave it in.

This holds **whenever the shutdown arrives**, including inside the grace the
deadline itself opened. An invocation past its timeout is still owed its ten
seconds to flush and commit, and an operator may interrupt the run at any point
in them; the invocation that grace was running for is then interrupted, not
timed out. Deciding the cause on the way *into* the grace and not again on the
way out fired a timeout transition on a ticket the shutdown had promised to
leave alone, and left the run's own report calling the run interrupted while its
ledger called the ticket timed out.

**Interruption.** `SIGINT`, `SIGTERM`, and `SIGHUP` delivered to `rhei run`
interrupt the run. Ctrl+C under the TUI is the same event, because the TUI
restores the terminal and re-raises `SIGINT` on the process
([§FS-rhei-run-tui.1.8](rhei-run-tui.spec.md#18-failure-modes)). So is `rhei stop`, which delivers the same signal to a
detached run's pid and adds nothing to this contract
([§FS-rhei-run-headless.7](rhei-run-headless.spec.md#7-rhei-stop)). Ctrl+C in an *attached* surface is not this event at
all: it disconnects the surface and never reaches the run
([§FS-rhei-run-headless.5.1](rhei-run-headless.spec.md#51-attaching-does-not-drive)). On the first such signal `rhei run`:

1. schedules no further work — no new pass begins, no freed worker slot
   refills, and the scheduler's waits return at once instead of sleeping out
   their interval. A ticket the pass had already chosen but not yet started is
   **not** started: it is left exactly as an unselected ticket, with no
   subprocess, no log, and no journal entry. Only invocations already in flight
   when the signal arrived are terminated;
2. terminates each in-flight invocation's process group with the sequence above
   and reaps it;
3. **fires no transition for an interrupted invocation.** The ticket keeps the
   state it was in, its task file is not rewritten, and
   `runtime/state-transitions.log` gains no entry. An interruption is neither a
   failure nor a timeout: no error transition, no timeout transition, no
   missing-output stall. The next `rhei run` re-executes the state. A
   *supporting* subprocess that is interrupted — the snapshot redactor — fails
   the step that started it, reporting the interruption and not a timeout; the
   ticket is left in its state by that failure exactly as it is by any other
   redactor failure.
4. records the invocation as `interrupted` — in the run report's ledger and
   invocations ([§FS-rhei-run-report.4](rhei-run-report.spec.md#4-transition-ledger)), in the run journal, and in the agent or
   program log footer ([§FS-rhei-agents.8](rhei-agents.spec.md#8-log-capture)) — and names the log path;
5. exits `128 + signal`: `130` for `SIGINT`, `143` for `SIGTERM`, `129` for
   `SIGHUP`. A run interrupted with non-terminal tickets remaining reports the
   interruption, not the halt diagnostic of §3 step 9. It reports the
   interruption when the signal cut the run's loop short — which is what the
   points above describe. A run whose loop had already finished when the signal
   arrived, one parked on the TUI's finished screen ([§FS-rhei-run-tui.1.5.7](rhei-run-tui.spec.md#157-liveness-color-and-lifecycle)),
   reports its own result instead: nothing was interrupted. It still exits
   `128 + signal`, because a signalled process reports its signal.

A **second** signal skips the grace and `SIGKILL`s every live group at once.
Only the operator can ask for that, and only by signalling twice. A run tearing
itself down for its own reasons — an error return, a panic unwind — stops its
work without shortening anyone's grace, so a single Ctrl+C is never escalated
into an immediate kill by a failure somewhere else in the run.
The operator is told so once, while the first shutdown is in progress, as a
warning on the run's event stream; the frontend decides where it is legible
([§FS-rhei-run-tui.1.8](rhei-run-tui.spec.md#18-failure-modes)). One notice, whichever waiter notices the interrupt
first — not one per invocation:

```text
Interrupted — terminating 2 invocation(s) (auth.1@implement, auth.3@review); press Ctrl+C again to kill immediately.
```

**An interruption is the operator's, not the run's.** The teardown that ends
in-flight invocations serves both an operator's signal and a run unwinding from
its own failure, and the invocations it ends carry the same `interrupted`
record either way. Only the first is an *interrupted run*: only a signal makes
the run report the interruption, exit `128 + signal`, or tell the operator to
re-run to continue. A run that died of an error reports the error, and its
tickets are described by why the run failed — never by "re-run to continue",
which would point the operator away from the failure that killed them.

**Supervisor death.** Termination is not conditional on a signal `rhei run` can
handle. An early error return, a panic unwind, and a normal end all pass
through the same group-termination path before the command returns. On Linux
each subprocess additionally arms a parent-death signal, so a `SIGKILL`ed or
OOM-killed supervisor — which runs no code at all — still delivers `SIGTERM` to
what it started. That backstop is best-effort and Linux-only; the handled paths
above are the contract. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

**Lost console output.** When the run's own output disappears mid-run — the
reader of a pipe stopped (`EPIPE`), or the terminal it was printing to went
away (`EIO`) — `rhei run` ends the way a Unix filter does: quietly, with status
`141`, or `128 + signal` when a signal had already arrived. It ends that way
**after terminating every in-flight process group**, because an exit taken from
inside a failed print runs no destructors and none of the paths above can fire.
`EPIPE` says the reader is gone wherever it points. `EIO` says it only on a
terminal — on a redirected stdout it is an ordinary write failure, a full device
or a dropped mount, and must be reported as one. Which of the two a stream is,
is decided by a reading taken **at startup**: a pty whose master has closed
fails `isatty` with `EIO` like every other ioctl on it, so a stream asked
afterwards denies ever having been a terminal, at exactly the moment the answer
decides whether the run ends quietly or aborts mid-unwind.

No run report is written in that case: there is nowhere left to say so, and
what the run did is in the task files, which are already current. A terminal
that goes away *after* the run has ended finds the report already on disk
([§FS-rhei-run-tui.1.8](rhei-run-tui.spec.md#18-failure-modes)).

### 3.3. Provider-Limit Parking

A recognized Codex/OpenAI provider limit (§FS-rhei-agents.2) parks the
invocation instead of routing its non-zero exit as an agent failure. The task
keeps its authored state, no transition or callback fires, no snapshot is
emitted, and the invocation does not spend the state visit's `attempts:`
budget. Its log, spawn record, and any reported usage remain available.

The observation instant is the subprocess-completion wall clock captured before
classification. Rhei resolves the reported reset against that instant's local
date in the named zone. The safe boundary is the first instant of the minute following the
reported minute. If that boundary is not strictly later than the observation,
Rhei resolves the same local minute on the next local date. Both the reported
minute and its following-minute boundary must resolve uniquely; Rhei does not
guess through a daylight-saving overlap or gap. The resulting
`nextAttemptAt` is stored as an RFC 3339 UTC instant. A later valid deadline
for the same task, state, and identity replaces an earlier one; an earlier or
equal deadline does not shorten an active wait.

The wait is runtime-owned frontmatter metadata with this state-qualified
shape:

```yaml
metadata:
  tasks:
    <task-id>:
      providerLimits:
        <state-name>:
          identity:
            agent: codex
            provider: openai
          signal: "You've hit your session limit · resets 10:20pm (Europe/Zurich)"
          observedAt: "2026-09-16T19:03:12Z"
          nextAttemptAt: "2026-09-16T20:21:00Z"
```

The execution identity is the pair of resolved agent registry id and resolved
provider. Model, mode, and task are deliberately not part of it: the supported
signal describes the Codex/OpenAI provider session, while the current
transport exposes no reliable account identifier. Only the reporting task's
record is persisted, but every active record suppresses not-yet-started work
with that identity. Already-running work and work with any other identity
continue. If several active records share an identity, suppression lasts until
their latest applicable deadline.

The record survives interruption and process restart. It applies only while
the named task remains in the keyed state; a stale record never blocks or
describes another state. Rhei removes it when that task successfully resumes,
when the task transitions out of the state, or when `rhei reset` removes the
task's runtime metadata. Failed or repeated provider-limited resumptions retain
or update it under the rules above.

A parked task is in deliberate waiting, not stalled, failed, or in Attention.
When no other runnable work remains, a foreground or headless run stays alive
and sleeps until provider-limited work becomes eligible. An operator interrupt
ends the run under §FS-rhei-run.3.2 without losing the wait; a later run uses
the persisted deadline. Headless launch still returns its run id immediately,
and attach reads the same provider and deadline from the run event stream.

## 4. Dry Run

With `--dry-run`, `rhei run` performs the same scan and selection logic but prints each planned transition instead of executing subprocesses or callbacks. Output format:

```text
would transition: Task <ID>  <from> -> <to>
```

No file lock is acquired, no markdown is rewritten, and no runtime artifacts are created.

Plain and JSON dry runs use their selected run frontend and carry initial
validation warnings through it with the ordering and frequency in §3. JSON
stdout remains pure JSONL. The warnings do not change the predicted exit status,
and neither dry-run form creates a run descriptor, journal, event log, report,
or any other runtime file.

A dry run **reports** the manual-only condition of §3 instead of aborting on
the first task that hits it:

```text
manual-only: Task <ID>  <from> -> <to> (claim with `rhei next`, finish with `rhei complete`)
```

The scan continues, so one invocation lists every manual-only task alongside
every transition that would run; the command still exits non-zero when any
were reported. Aborting on the first one defeats the purpose of the flag —
under the built-in machine, whose initial state is manual-only, it made
`--dry-run` fail before printing anything at all.

**A dry run predicts the real run, including its exit status.** When the scan
finds nothing schedulable, it reports why each remaining in-scope ticket is not
moving — the same classification the halt path uses ([§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout)) —
and then ends the way `rhei run` would on the same state:

```text
Nothing to schedule. Why each remaining ticket is not moving:
  Task auth.1 (pending): claimed by alice — `rhei release auth.1` to hand it back, …
  Task auth.2 (pending): waiting on Task auth.1 (pending) — finish the prior first
```

A ticket the scheduler skips is invisible to the transition scan: it produces
no `would transition:` line and no `manual-only:` line. Without this report a
dry run over a project whose ready tickets were all claimed printed nothing at
all and exited zero, while `rhei run` on the identical state halted non-zero —
so the one command whose job is to answer "what happens if I run this?"
answered it wrongly, and a wedged queue behind a crashed worker's stale claim
read as "nothing to do". The dry run exits non-zero whenever the remaining
tickets need a human; gating states awaiting a decision are a deliberate pause
and do not by themselves fail it.

## 5. Parallel Execution

With `--parallel N`, up to `N` subprocesses run concurrently. The orchestrator:

- Assigns each spawn a slot index.
- Writes one line to `runtime/transitions.log` per `SlotAssigned` and one per
  `SlotReleased`; see [Run TUI Specification — Run Event Journal](rhei-run-tui.spec.md#17-journal-format).
- Serializes every state write through its own file lock, so two agents completing at once cannot corrupt the plan.
- Refills freed slots immediately: after any subprocess exits and its result is
  processed, the orchestrator performs the admission checkpoint in §3,
  recomputes the ready set, and starts newly ready work while the rest of the
  pool keeps running. Admitted work consumes the same ordinary slots; it never
  expands the pool. Each task
  selected during that refill is resolved from its reloaded task metadata with
  the normal execution precedence, including the full task `**Target:**`
  override defined by §FS-rhei-plan-language.3.11.

Tasks whose transitions would race on the same task node are never scheduled in
parallel: scheduling is driven by the ready set, which excludes tasks already in
flight. A dependent task only becomes schedulable after its `**Prior:**` task has
actually reached a successful terminal state; if sibling work finishes first,
the freed slot is filled only with work whose dependencies are already satisfied.

A supervising task's subtree additionally drains at each checkpoint: once a
checkpoint is delivered, no new descendant of that supervisor starts until the
supervisor has run and released the subtree again ([§FS-rhei-supervision.3.1](rhei-supervision.spec.md#31-the-rule)).

### 5.1. Polling States

States that declare a [`poll:`](rhei-states.spec.md#2-polling-states) block are time-triggered: each attempt spawns one subprocess, the engine evaluates transitions, and a self-loop transition means "retry after `poll.interval`". Between attempts, the orchestrator:

- Persists `metadata.tasks.<id>.pollNextAttemptAt.<state-name> = now() + interval` and `metadata.tasks.<id>.stateVisits.<state-name>` (the attempt counter).
- Releases the `--parallel` slot so other ready tasks may run.
- Does not hold a timer thread; the next pass re-scans and picks the task up again only once `pollNextAttemptAt` is in the past.

If, at the end of a pass, every remaining non-terminal task is either in a gating state, blocked behind a gating dependency, or blocked by a pending `pollNextAttemptAt`, `rhei run` sleeps until the earliest `pollNextAttemptAt` across all blocked poll tasks (bounded below by 1 s to avoid busy-looping) and then begins a new pass. If no poll deadline is pending and only gating remains, the run exits as it does today.

Provider-limit waits join this same deadline scan. For one task, eligibility is
the later of its applicable poll deadline and provider-limit deadline. The run
sleeps until the earliest such effective eligibility deadline among all
waiting tasks, bounded below by 1 s, and does not emit `run_finished` merely
because all remaining work is provider-limited. Poll attempt counters,
self-loop selection, and exhaustion are unchanged.

Once `stateVisits.<state-name>` reaches `poll.max_attempts`, the engine refuses to select a self-loop transition and picks the first matching non-self-loop instead. If no non-self-loop transition matches, the run halts that task with a "polling exhausted with no matching non-self-loop transition" error — `--continue-on-error` applies as with any other task failure. A non-self-loop exit at any attempt clears both `pollNextAttemptAt.<state-name>` and `stateVisits.<state-name>`.

A poll that declares `waiting_on` ([§FS-rhei-states.2.5](rhei-states.spec.md#25-waiting-on-a-person)) is scheduled by every
rule above, unchanged: same attempts, same slot release, same sleep to the
earliest `pollNextAttemptAt`, same exhaustion edge, and the same contribution
to whether the run ends non-zero (§4) — a poll inside its backoff window
already counted as deliberate waiting there. What the field adds is how the run
*reports* the ticket: it is named as waiting on that person rather than left to
read as work in flight ([§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout), [§FS-rhei-run-report.4](rhei-run-report.spec.md#4-transition-ledger)). Under
`--rhei`, the no-advancement line names it the same way — and under the same
condition, that the poll be the whole reason the ticket is not moving. The
claim answers first there as it does in the classification: a ticket with a
live `**Assignee:**` is named on the halt line above, with `rhei release` as
its remedy, and contributes nothing to this line rather than a second reason
contradicting the first. An unsatisfied `**Prior:**` is named as the prior it
is ([§FS-rhei-panta.6.1](rhei-panta.spec.md#61-readiness-and-rhei-next)).

`snapshot.inherit` is rejected on polling states in v1. Snapshot emit,
including auto-emit, is suppressed for self-loop attempts and runs only on a
terminal non-self-loop exit when the state is otherwise snapshot-capable.

### 5.2. Concurrent vs. Serial States

The [`concurrent`](rhei-states.spec.md#12-per-state-fields) flag on a `StateDef` determines whether multiple ready tasks in the same state may be scheduled together in one pass:

- `concurrent: true` — any number of ready tasks in this state may be scheduled together (bounded by `--parallel`).
- `concurrent: false` (the default) — at most one ready task in this state is scheduled per pass. Additional tasks remain ready and are picked up on the next pass.

The flag does not change state entry/exit semantics or transitions, and it does not affect within-task fanout (`all_targets` / `all_models`): every resolved invocation for a single scheduled task is still spawned together.

## Relationship to Other Commands

`rhei run` drives the full plan forward under orchestrator authority. It is mutually exclusive per execution with the manual-worker flow (`next` / `transition` / `complete`) — they never overlap on the same task because `rhei run` holds transition responsibility for the states it drives.

See [How Rhei Is Used — Command Surface](rhei-usage.spec.md#22-command-surface) for the full table comparing all five coordination commands.

## Related Specifications

- [Agents Specification](rhei-agents.spec.md) — completion authority, completion condition, timeout handling, environment variables
- [Program States Specification](rhei-programs.spec.md) — exit-code transitions and program-specific semantics
- [Snapshots Specification](rhei-snapshots.spec.md) — snapshot side effects and inheritance preload
- [Snapshot Operations Specification](rhei-snapshot-operations.spec.md) — snapshot commands, settings, and `--from-snapshot`
- [Run TUI Specification](rhei-run-tui.spec.md) — live terminal UI and transition journal
- [Run JSON Stream](rhei-run-json.spec.md) — `--json`, the event record contract, and `runtime/events.jsonl`
- [Detached Runs](rhei-run-headless.spec.md) — `--headless`, `rhei attach`, `rhei stop`, `rhei runs`
- [Cost Accounting Specification](rhei-cost-accounting.spec.md) — token/cost ledger and rollups
- [Transitions Specification](rhei-transitions.spec.md) — transition YAML schema and callbacks
- [Next Command](rhei-next.spec.md), [Complete Command](rhei-complete.spec.md), [Transition Command](rhei-transition-cmd.spec.md) — manual-worker counterparts
