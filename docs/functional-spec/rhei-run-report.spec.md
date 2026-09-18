# FS-rhei-run-report: Per-Run Report

`rhei run` writes a durable Markdown report at the end of every run so an
operator can understand what happened without replaying the browser dashboard,
reading raw logs, or knowing the internal artifact-reuse rules. The report is a
commit-friendly UI: plain Markdown first, with enough structure for dashboards
and future JSON export to render the same facts. It complements the live TUI and
Flow dashboard; it does not replace either surface. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

The report answers four questions:

1. What final state did every task reach?
2. Which transitions were taken, and why were those transitions allowed?
3. Did Rhei spawn an agent or program, or did it advance because existing
   artifacts already satisfied the state contract?
4. Which tasks did not advance, and what concrete condition kept them there?

## 1. Report Artifact

At the end of `rhei run`, Rhei writes:

- `runtime/run-report.md` - latest report for quick inspection.
- `runtime/run-reports/<timestamp>-<run-id>.md` - immutable history entry.

The latest report may be overwritten by the next run. Timestamped reports are
append-only unless `rhei reset` removes `runtime/`, matching the existing runtime
artifact lifecycle. A failed run still writes a report with the information
available up to the failure point. A `--dry-run` is the one exception: it is a
side-effect-free preview and writes no report file (§3.5).

The timestamped history entry is the durable completion evidence for an exact
`rhei summary --run <ID> --prices <BOOK>` selection
(§FS-rhei-summary.1). Its `Run:` header associates the full report with that
exact id; a filename prefix, the overwriteable latest report alone, or an
invocation record alone does not establish that the run ended. “Completed” in
this selection sense means the run reached its immutable end-of-run report,
not that every task succeeded: interrupted and failed runs remain finished
runs with the measurements their reports and records preserve. Live descriptor
or held-lock evidence for the same id overrides a history entry until the run
ends.

The report uses relative links for logs and artifacts so it remains useful after
the workspace is moved, committed, archived, or pasted into an issue.

## 2. Markdown UI

The report layout is optimized for scan-first reading:

1. **Header** - plan title, workspace path, run id, start/end time, duration,
   command mode, parallelism, dashboard artifact path, and overall result.
2. **Outcome Strip** - small summary table for final task states, transitions,
   spawned agents, spawned programs, callback-only advances, reused-output
   advances, dry-run transitions, terminal-at-start tasks, and blocked tasks.
   When accounting exists, a second strip shows what **this run** spent: cost,
   explicitly inclusive input/output totals, their cache-read and cache-write
   parts, and coverage (§2.1).
3. **Attention** - the shortest path to action: blocked tasks, gating tasks, and
   halted tasks with the exact reason.
4. **Transition Ledger** - one row per transition attempt or no-op observation,
   in chronological order.
5. **Task Final States** - source-order task tree with final state, start state,
   terminal-at-start marker, and last transition.
6. **Artifacts Checked** - grouped artifact evidence for readers investigating
   reuse, missing inputs, or missing outputs.
7. **Invocations** - spawned agents/programs with command labels, targets,
   models, exit status, duration, logs, accounting record if present, and output
   paths.
8. **Task Costs** - direct task cost and token totals when accounting was
   reported. The task cost is the sum of agent states spawned directly for that
   task; subtree accounting remains available through `rhei cost`. [§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)

The top of the report must expose reuse counts and blocked counts without
scrolling. A run that performs no spawned work because every required output
already existed must be visibly different from a run that spawned work quickly.

### 2.1. Accounting Strip

The accounting strip answers one question — *what did this run spend* — and it
must not be readable as any other. It reports the invocation records that name
this run. [§FS-rhei-cost-accounting.3.5](rhei-cost-accounting.spec.md#35-run-attribution)

```text
| Accounting (this run) | Value |
| --- | ---: |
| source | rollup |
| cost | unpriced |
| total tokens | 1.2k |
| input tokens (incl. cache) | 1.0k |
| input cache read | 800 |
| input cache write | 100 |
| output tokens (incl. cache) | 234 |
| output cache read | - |
| output cache write | 0 |
| coverage | Partial |
| workspace total tokens | 1.7M |
```

Three rules make it honest.

1. **The heading names the scope.** `Accounting (this run)`, not `Accounting` —
   which a reader completes as whatever they came to the report looking for.
2. **`source` says which quantity this is.** The report produces the strip two
   ways, and they are different numbers. `rollup` is the end-of-run rollup over
   the run's invocation records. `run events` is the fallback taken when no
   rollup could be produced, summarizing only the `UsageReported` events this run
   observed. [§FS-rhei-cost-accounting.7](rhei-cost-accounting.spec.md#7-run-events) Two tables that look identical and mean
   different things are the defect, not the number.
3. **The workspace lifetime total has a row of its own.** `workspace total
   tokens` carries `workspace_total` [§FS-rhei-cost-accounting.6](rhei-cost-accounting.spec.md#6-rollups) and appears
   under `source | rollup`. The events fallback has no rollup to read it from, so
   the row is absent there rather than guessed.

The token rows always use the order in the example: `total tokens`, then the
inclusive input total followed by its cache-read and cache-write parts, then
the inclusive output total followed by its cache-read and cache-write parts.
Cache parts are already included in their side's total
([§FS-rhei-cost-accounting.3.1](rhei-cost-accounting.spec.md#31-token-dimensions));
the presentation does not add them again. An unavailable cache dimension reads
`-`, while a measured zero reads `0`.

A run that spawned no agent still gets the strip, and its token rows read `0`.
Putting the workspace's lifetime total there instead says this run spent what
every run before it spent together — directly above the report's own note that
no agent ran.

A workspace with no accounting history at all is the one case that gains no
table. A strip of zeros there would say measurement happened and found nothing,
where nothing was ever measured; the report stays silent, as `rhei cost` says
`(no accounting records found)` rather than printing a total of zero.

The end-of-run console summary carries the same quantity under the same scope
label (§3.1), grouping cache parts with their inclusive whole. For an input
total of 1,100 with 700 cache-read and 300 cache-write tokens, and an output
total of 50 whose cache parts are unavailable, it reads `In 1.1k (incl. cache:
read 700, write 300)` and `Out 50 (incl. cache: read -, write -)`.

### 2.2. Task Costs

The Task Costs table uses the same symmetric token presentation for every
task's direct accounting. After `Task` and `Cost`, and before `Coverage`, its
columns are ordered exactly as `Total`, `Input (incl. cache)`, `Input cache
read`, `Input cache write`, `Output (incl. cache)`, `Output cache read`, and
`Output cache write`.

These are the same inclusive totals and cache parts as §2.1: cache parts are
not added to either side or to `Total`. An unavailable cache dimension reads
`-`, while a measured zero reads `0`. The table's task scope, direct-cost
meaning, cost, and coverage placement are unchanged.

## 3. End-of-Run Console Summary

When `rhei run` exits, the last thing it prints is a compact summary block, then
the path to the durable report. This is the at-a-glance sibling of the Markdown
report: it shows the same Outcome Strip counts and the head of the Attention
list so an operator learns the result and the next action without opening a
file, then points at the report and dashboard for the full forensic read. The
file is the durable explanation; the console summary is the pointer to it.

On an interactive terminal the summary supersedes the flat `Run complete: … /
Final states: … / one line per task` dump. It obeys the console-first visual
language: monospace, dark, near-zero chrome, and saturated color reserved for
states that need attention, so the terminal, the TUI tiles, and the dashboard
name the same state the same way. [§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux) When stdout is not a TTY, the
existing line-oriented output is preserved verbatim so scripts and tests that
match it keep working (§3.4).

### 3.1. Layout

After the TUI restores the terminal - or after plain-stdout execution finishes -
the summary prints five stacked groups:

1. **Result line** - plan title, run id, duration, and overall result, using the
   same result vocabulary as the report header.

   A run the operator interrupted ([§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership)) reads as its own outcome
   and not as a halt: the result line is `interrupted — re-run to continue`,
   taking precedence over every other phrase, and the Attention row of a ticket
   whose worker was interrupted names the interruption as the blocker with
   `re-run to continue` as the next action. Neither surface tells the operator
   to inspect logs or to cancel the task: nothing about the ticket failed, the
   run was stopped, and re-running it is the whole recovery. The console prints
   no `Run complete:` line for such a run.
2. **Counts** - two dense lines: final task states, then run activity (agents,
   programs, reused-output, callback-only, terminal-at-start, could-not-advance).
   The states line is preceded by a static state-distribution bar whose segments
   are sized by count and colored by state hue; the labeled counts beside it are
   the authoritative, non-color readout. Reuse and blocked counts are always on
   screen, never below a fold. The bar is drawn once and never animates
   [§FS-rhei-viz-ux.4](rhei-viz-ux.spec.md#4-motion-and-liveness).
3. **Attention** - up to five highest-priority halted tasks (gated first, then
   blocked, then halted), each with its state, the proven blocker, and the next
   action, ordered exactly as the report's Attention section. A trailing
   `… N more in the report` line appears when the list is truncated. The group is
   omitted entirely when no task is halted, rather than printing an empty heading.

   A ticket **held by a supervisor** ([§FS-rhei-supervision.3.4](rhei-supervision.spec.md#34-manual-workers)) is not in this
   group. Its own next action is "nothing to do on this ticket", so listing it
   beside rows a person must act on dilutes them; it goes under a **Waiting**
   group of its own — same columns, calm marker, omitted when empty — and the
   durable report gets a matching `## Waiting` section after `## Attention`.
   Held tickets are therefore counted in neither the `N gated · M blocked`
   header nor `could not advance` (§2), for the same reason a parent held open
   by its own subtree is not: the ticket that *is* someone's problem — the
   supervisor, or the human gate holding it — reports for itself.

   A ticket sitting in a **person-waiting poll** — a poll state declaring
   `waiting_on` ([§FS-rhei-states.2.5](rhei-states.spec.md#25-waiting-on-a-person)) — joins it there, for the same reason
   and with the same treatment: it is somebody's turn, it resumes itself when
   the answer lands, and there is nothing for the operator to do. Its reason
   names the label (`waiting on author`) and its next action says so plainly.
   The group's count line names each kind it holds rather than one of them —
   `2 held · 1 waiting on a person` — and reads exactly as before when only
   held tickets are in it. Without this the run's only reading of such a ticket
   was work in flight, and one plan-approval poll held a concurrency slot for
   hours looking indistinguishable from an agent that was running.

   It joins the group **only when the poll is the whole reason it is not
   moving**. A live `**Assignee:**` or an unsatisfied `**Prior:**` stops the
   ticket from ever reaching its next attempt, so the calm reading would be a
   false promise: the poll cannot resume itself out of a claim, and `rhei
   release` — not the author's answer — is the remedy. Such a ticket is
   classified by the claim or the prior, stays in Attention with its ordinary
   marker, and keeps counting toward `could not advance`. This is where a
   person-waiting poll parts from a gate: a gate outranks both because nothing
   but a human moves it either way, while a poll is machinery that a claim or a
   prior really does stop.

   Nothing about the run's exit status follows from this. Whether work remains
   that a human must act on is the judgment in [§FS-rhei-run.4](rhei-run.spec.md#4-dry-run), unchanged: a
   person-waiting poll is a poll, and it already counted as deliberate waiting
   there while its backoff window was open.

   A task parked by a recognized provider limit (§FS-rhei-run.3.3) also
   belongs in **Waiting**, never **Attention**. Its row names `provider openai`,
   the reporting task, and the UTC `nextAttemptAt`, with automatic resumption as
   the next action. It does not count as failed or `could not advance`; an
   assignee, prior, or gate that independently prevents it from reaching the
   retry keeps the existing higher-priority classification.

   The blocker and next action come from a **plan-wide classification** of why
   each non-terminal task node is not moving, in this order: an open descendant
   subtree; a gating state awaiting a decision; a live `**Assignee:**`; an
   unsatisfied `**Prior:**`; a poll state that declares it waits on a person; a
   worker interrupted mid-flight by an *operator's*
   shutdown ([§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership)) — a run that tore its own workers down while
   failing describes their tickets by the failure instead, never by "re-run to
   continue"; a manual-only initial state; a worker that ran and
   left a required artifact unwritten — agent or program alike, since both are
   workers and both stall the same way; a required `inputs:` artifact of the
   ticket's current state that is not on disk ([§FS-rhei-states.3](rhei-states.spec.md#3-artifact-contracts)), named with
   the path readiness looked for; a ticket the run never scheduled; no
   declared outgoing transition; anything else. The person-waiting poll sits
   behind the claim and the prior, and the gate ahead of them, for the reason
   given above: both really do stop a poll from coming back, and neither can
   stop a gate. Interruption is classified
   ahead of the missing-artifact class because it explains it: a worker the run
   killed had no chance to write what it owed, and telling the operator to
   write those files by hand would be advice about a stall that did not
   happen. A missing required input is classified ahead of "never scheduled"
   for the same reason: it *is* why the ticket was never scheduled, and
   "rerun to pick it up" sends the operator back to a run that halts
   identically — but only when the missing file is what readiness stopped at:
   a `poll:` state whose next attempt is still ahead, and a supervisor still
   draining its subtree, are refused before inputs are read at all, and writing
   the file releases neither. The path is named, and named absolutely, because
   the file the operator wrote and the file readiness looked for can differ — a
   rhei that is a member of a Panta project resolves its inputs against its own
   execution root ([§AR-rhei-panta.5](../architecture/rhei-panta.spec.md#5-execution-root-and-per-rhei-runtime)), not the project's, so *which root* is the
   whole question a relative path leaves open.

   The classification is one answer per run, not one per surface: it is made
   under the artifact roots that run's own ready-set scan resolved against —
   which the run has, and hands to it — so the halt message, this group, and
   every section of the durable report give one ticket one reading. A surface
   classifying without those roots would contradict the halt message ten lines
   above it.

   Non-leaf tasks are classified alongside leaves
   because a non-leaf task is a task in its own right
   ([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)); a parent whose subtree is still open reads as
   waiting on that subtree, which is a structural consequence rather than
   something a human must fix, so — like a gate — it does not by itself make
   the run exit non-zero. The descendants named in it do that on their own
   account.

   The artifacts a missing-artifact row names are the ones the ticket's most
   recent worker left unwritten **in the state the ticket is still sitting
   in**. A ticket that was spawned again, or that moved on and later halted for
   an unrelated reason, is classified from scratch: an old stall never explains
   a new one.

   A parent held open **only** by its own subtree is therefore not halted work
   and takes **no Attention row**: it is counted in neither the `N gated ·
   M blocked` header, nor `could not advance` (§2), nor the Transition Ledger
   (§4). The open descendant is the halted work and is already counted there.
   Counting the parent as well multiplied one gated leaf by the height of the
   tree above it — three ancestors made four Attention rows, `4 gated`, and
   four blocked ledger entries for a single human decision, with the topmost
   parent's reason text repeating the whole transitive subtree. The parent
   stays visible in the task tree instead, with its own calm marker and its own
   `waiting on open descendant …` detail (§3.2). A parent that is itself
   gating, `blocked`, or `failed` keeps its Attention row: those are things to
   act on whatever its children are doing, even though the open subtree
   outranks them in the classification order above.

   A **gating** parent that still carries a `held` supervision block
   ([§FS-rhei-supervision.3.1](rhei-supervision.spec.md#31-the-rule)) is classified ahead of the open-subtree reading,
   because it is not waiting on its descendants — it is holding them. Its row
   says so and names them, and the next action is the human's: move it back into
   its supervising state to resume supervision, or anywhere else to release the
   subtree. The held descendants keep their own held rows, and those rows name
   the human rather than a supervisor visit that is not coming.

   The run's exit status and `--dry-run`'s are one judgment, not two readings
   of this classification: a run ends non-zero when work remains that is
   waiting on neither a human gate, a poll backoff, nor a `**Prior:**` that is
   itself waiting on one of those ([§FS-rhei-run.4](rhei-run.spec.md#4-dry-run)). Deriving the prediction
   from the per-ticket causes instead made `--dry-run` exit non-zero on plans
   the run itself exits zero on — a ticket whose prior chain ends in a gate is
   blocked by that gate, however its own line reads.
   Each names the command that clears it — `rhei release <id>` for a claim,
   `rhei next` then `rhei complete <id>` for manual-only work, finishing the
   prior for a dependency.

   A ticket the run actually spawned work for is a different kind of halt: its
   problem is the work, not the scheduling. When that work stopped without
   writing required artifacts the run knows the names of — including the
   ticket's terminal result, reported under the artifact name `result` like any
   other ([§FS-rhei-agents.3.2.1](rhei-agents.spec.md#321-runtime-semantics)) — the row names them: `worker exited
   <code> without <name> (<path>), …`, and the next action is to write those
   files, or to record the outcome by hand with `rhei transition <id> --from …
   --to … --result …`. `<code>` is the code that worker actually exited with,
   which is not always `0`: a program that took a declared route
   ([§FS-rhei-programs.3.2](rhei-programs.spec.md#32-evaluation-order)) out of a non-zero exit owes the ticket's result
   the same way, and a row reading `0` would contradict the same report's own
   Transition Ledger (§4) two sections below it and send the operator looking
   for an exit that never happened. A row whose exit the run has no code for —
   a worker a signal ended — names none, and reads `worker exited without …`. That is the whole difference between a report an operator can
   act on and one that says the task "stalled" and suggests reading logs that
   name nothing the operator did not already know. A ticket that ran and left
   nothing identifiable behind keeps the generic stalled reading; a ticket the
   run never scheduled says so, rather than borrowing it.

   The same classification feeds the live halt message and `--dry-run`
   ([§FS-rhei-run.4](rhei-run.spec.md#4-dry-run)), so the durable report and the terminal never disagree
   about one run. They did: the report described a claimed ticket, a
   prior-blocked ticket, and a manual-only ticket alike as `stalled in
   non-terminal state <state>` with `inspect logs or mark the task cancelled`,
   while stderr named the real cause precisely. That advice was wrong three
   ways — it proposed cancelling work that only needed its claim released or
   its prior finished, and it pointed at logs that a run spawning nothing never
   wrote. The report is what survives after the terminal scrolls away, so it is
   the copy that has to be right.
4. **Tasks** - the source-order task tree, every task on one aligned row, so the
   operator sees the whole plan's outcome, not only the halted tail. This mirrors
   the report's Task Final States section (§5). Tree and collapse rules are in
   §3.2.
5. **Pointers** - the latest report path, the timestamped history entry, and the
   dashboard artifact when one was written.

```text
Run Report  Rhei UI Canonical Test                       9f24c6 · 12m04s
  stopped for human attention

  States    █████████ ██ █████ █     9 completed · 2 human-gate · 5 blocked · 1 cancelled
  Work      7 agents · 15 programs · 0 reused · 0 callback-only · 2 terminal-at-start
  Cost      $1.23 · In 2.4M · In cached 1.5M · Out 180k · Out cached - · Coverage partial

Attention  3 gated · 4 blocked
  ! ui-test-canonical-example.full-pipeline              human-gate  counted fix loop finished
        → inspect runtime/fixes/ui-test-canonical-example.full-pipeline-visit-2.md and transition manually
  ! ui-test-canonical-example.poll-exhaustion            blocked     pollAttempts reached pollMaxAttempts
        → inspect runtime/logs/task-ui-test-canonical-example.poll-exhaustion-poll-exhaust.log
  … 5 more in the report

Tasks   17 tasks · source order
  ✓ ui-test-canonical-example.collect-inputs             completed    agent     3.4s
  ⏸ ui-test-canonical-example.full-pipeline              human-gate   counted fix loop finished
  │ ✓ ui-test-canonical-example.script-normalize         completed    program   0.2s
  │ ✓ ui-test-canonical-example.mock-implement           completed    agent     8.1s
  │ ✓ ui-test-canonical-example.parallel-review          completed    agent×2   5.0s
  │ ! ui-test-canonical-example.live-failure-blocked     blocked      program exited 42
  ✓ ui-test-canonical-example.snapshot-child             completed    agent     2.7s
  · ui-test-canonical-example.terminal-completed         completed    —         terminal at start
  ⏸ ui-test-canonical-example.human-gate                 human-gate   seeded gating state
  ! ui-test-canonical-example.poll-exhaustion            blocked      pollAttempts reached pollMaxAttempts
  ! ui-test-canonical-example.skill-unavailable-blocked  blocked      required skill absent-lens unavailable
  ⊘ ui-test-canonical-example.cancelled-task             cancelled    —
  … 3 completed tasks collapsed

Report     runtime/run-report.md
History    runtime/run-reports/2026-06-05T09-22-31Z-9f24c6.md
Dashboard  runtime/dashboard.html
```

Only the result line, the `!` attention rows, and each task's state column carry
saturated color, and the hue is the state color defined in [§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux).
Every other line is plain chrome.

### 3.2. Task tree

The Tasks group lists every task in source order, preserving hierarchy with a
leading `│` gutter for child rows, so a reader recognizes the same shape the
report, TUI, and dashboard show. Each row is four aligned columns:

`<marker> <task-id>   <state>   <driver-or-detail>`

- **marker** - a fast scan glyph that degrades gracefully; color and the state
  label remain the primary signal [§FS-rhei-viz-ux.3](rhei-viz-ux.spec.md#3-color), so the marker is never the
  only cue and an ASCII fallback is used where the glyph is unavailable.

  | Marker | Fallback | Meaning |
  | --- | --- | --- |
  | `✓` | `+` | terminal-success state (`completed` or a custom terminal-success state) |
  | `⏸` | `=` | deliberately paused - a gating state awaiting a human (`human-gate`), or a non-leaf task held open only by its own subtree (§3.1) |
  | `!` | `!` | blocked or failed - needs attention |
  | `⊘` | `~` | `cancelled` |
  | `·` | `.` | terminal at the start of the run |

  A non-leaf task is halted for as long as any descendant is open, which is the
  eligibility rule working ([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)), not a fault: one gated
  leaf otherwise turns every ancestor above it into its own attention row. Such
  a parent takes the paused marker, and the task tree is the only place it
  appears — it is counted in no Attention, header, or ledger tally (§3.1). Its
  detail column still reads `waiting on open descendant …`, so the tree shows
  the shape of the wait without inflating any count. A parent that is itself
  `blocked` or `failed` keeps the `!` marker: that is wrong independently of
  its children.

- **state** - the final state label, colored by its state hue.
- **driver-or-detail** - for advanced tasks, the driver and timing
  (`agent 3.4s`, `program 0.2s`, `agent×2 5.0s`, `reused`); for halted tasks, the
  same proven blocker shown in Attention; for terminal-at-start rows, `terminal
  at start`.

Only `!` rows take saturated attention color. `✓`, `·`, and `⊘` rows are calm
chrome so a healthy run reads as quiet.

To keep large plans scannable, a subtree whose every descendant is in a
terminal-success state collapses to a single trailing `… N completed tasks
collapsed` line. Every halted, gated, cancelled, or otherwise non-terminal task
is always shown, never collapsed, so nothing that needs a human disappears. The
report's Task Final States section (§5) is the un-collapsed source of truth.

### 3.3. Reused-output runs look different

The central UI requirement of this spec applies to the console too: a run that
spawned no work because every required output already existed must not look like
a fast successful run. When `agents == 0 && programs == 0` and at least one
transition was driven by `reused-output`, the result line reads `completed — no
work spawned` and a dedicated Reuse line names the cause:

```text
Run Report  Rhei UI Canonical Test                       a13f02 · 0.4s
  completed — no work spawned

  States    █████████████████████   22 completed
  Work      0 agents · 0 programs · 22 reused · 2 terminal-at-start
  Reuse     every required output already existed; no agent or program ran

Tasks   22 tasks · source order
  ✓ ui-test-canonical-example.collect-inputs             completed    reused
  ✓ ui-test-canonical-example.full-pipeline              completed    reused
  │ ✓ ui-test-canonical-example.script-normalize         completed    reused
  │ ✓ ui-test-canonical-example.mock-implement           completed    reused
  · ui-test-canonical-example.terminal-completed         completed    terminal at start
  … 17 completed tasks collapsed

Report     runtime/run-report.md
```

The `reused` detail on every row, not a duration, is what tells the operator no
subprocess ran. A reused run never collapses to a bare `✓` with a fast timing.

### 3.4. Plain / non-TTY mode

When stdout is not a TTY (piped, redirected, CI), the rich summary is suppressed
and `rhei run` keeps its existing line-oriented output verbatim, so scripts and
tests that match it keep working. [§FS-rhei-run-tui.1.4](rhei-run-tui.spec.md#14-frontend-selection) The frontend's TTY
detection decides this once, the same way the TUI/stdout frontend is chosen. The
preserved output keeps the greppable `Final states: <state>=<count>` prefix, the
`N/N tasks in terminal state` summary line, and one `- Task <id>: <title>
[<state>]` line per task:

```text
Run complete: 7 agent(s), 15 program(s) spawned, 9/24 tasks in terminal state.
Final states: blocked=5, cancelled=1, completed=9, human-gate=2
  - Task ui-test-canonical-example.full-pipeline: Full pipeline [human-gate]
  - Task ui-test-canonical-example.polling: Polling loop [completed]
  - Task ui-test-canonical-example.poll-exhaustion: Poll exhaustion [blocked]
  …
```

The durable Markdown report (§1, §4) now backs the non-TTY path: after the
preserved line-oriented output, `rhei run` prints a greppable `Report:
runtime/run-report.md` pointer to the report it just wrote.

### 3.5. Dry runs and empty runs

- Under `--dry-run`, the run is side-effect-free: it writes no report file and
  leaves `runtime/` untouched. The summary keeps the existing terminal phrase
  `Dry run complete - no agents were spawned.` and, on a TTY, shows the rich
  summary's counts for the simulated transitions (every output status
  `not-checked`). It prints no `Report:` pointer, since no report was written.
- When nothing advanced and nothing was reused, the result line reads `no tasks
  could be advanced` and the Attention group lists the blockers - the
  explanation the current bare `No tasks could be advanced.` line cannot give.

## 4. Transition Ledger

Each ledger row represents one task-state decision made by the scheduler. The
columns are:

| Column | Meaning |
| --- | --- |
| `task` | Task id, preserving hierarchy in source order. |
| `from` | State before the decision. |
| `to` | Selected destination state, or `-` when no transition was taken. |
| `driver` | `agent`, `program`, `callback-only`, `reused-output`, `dry-run`, `terminal-at-start`, or `blocked`. |
| `inputs` | Compact `name: ok/missing/optional-missing` list with resolved paths in the detail block. |
| `outputs` | Compact `name: created/reused/missing/not-checked` list with resolved paths in the detail block. |
| `invocation` | Agent/program label and log path, or `none`. |
| `reason` | The condition that selected the row: exit code, callback, output reuse, prior dependency, gate, poll delay, missing input, missing output, missing skill/MCP, terminal state, interruption, or run option. |

`driver: reused-output` means the current state's required outputs existed
before any subprocess was spawned for that decision, so Rhei was able to evaluate
the outgoing transition without running the autonomous state. This is the
high-signal label for artifact-reuse investigations. The row must still list
the checked outputs and mark them `reused`.

`driver: callback-only` means the transition was made through callbacks or
transition rules while autonomous spawning was disabled or not applicable. It is
not used for existing-output reuse, because that case needs its own visual
treatment.

An invocation the operator interrupted ([§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership)) reports `interrupted`
as both its exit status in **Invocations** and its ledger `reason`: the run
stopped the worker before it could finish, so the row records the state it ran
in and no transition out of it. It is deliberately distinct from `timed out`
and from a failing exit code — nothing about the ticket is known to be wrong,
and the marker used for it in live surfaces (`⏹`) is calm chrome rather than
attention color, for the same reason. The ticket's own row keeps whatever
marker its unchanged state earns.

A provider-limited invocation has `driver: agent`, destination `-`, and reason
`provider openai limited until <nextAttemptAt>`. Its invocation entry retains
the real exit code and log while its outcome is `provider_limited`; the ledger
does not call it blocked or failed and records no transition. Repeated parked
invocations remain separate auditable invocations even though none consumes an
agent attempt.

`driver: blocked` is used for non-terminal tasks that remain in place at run end.
The row's `reason` must name the first concrete blocker Rhei can prove, such as
`waiting for prior ui-test-canonical-example.polling`, `gating state human-gate`, `waiting on author`, `missing input
runtime/build/ui-test-canonical-example.full-pipeline-report.md`, `poll next attempt at ...`, or
`required skill absent-lens unavailable`.

## 5. Artifact Evidence

The report records both sides of artifact checking:

- **Inputs checked before work** - every declared input for the state, the
  resolved path, whether it existed, and whether it was optional.
- **Outputs checked for transition** - every declared output for the source
  state, the resolved path, whether it existed before work, whether it existed
  after work, and whether it was required.

The UI uses four output statuses:

| Status | Meaning |
| --- | --- |
| `created` | Missing before invocation, present after invocation. |
| `reused` | Present before invocation or before reuse-based transition. |
| `missing` | Required after invocation or transition check and not present. |
| `not-checked` | No output check applied, such as terminal-at-start or dry-run. |

This makes accidental artifact reuse visible. `{task_id}` renders the
project-qualified ticket id, so two rheis sharing an execution root no longer
collide on paths like `runtime/milestones/{task_id}.md` ([§AR-rhei-panta.5](../architecture/rhei-panta.spec.md#5-execution-root-and-per-rhei-runtime)); the
remaining reuse case is an artifact left behind by an earlier run of the same
ticket. Such a run shows `driver: reused-output` and `output: milestone reused`
instead of implying that an agent completed the work.

**Implementation status.** The durable report (§1, §2) and the Transition Ledger
(§4) are emitted from the run event stream and the run-start state snapshot: every
spawned agent/program transition carries its real driver, exit, log path, and the
synthesized callback-only, terminal-at-start, and blocked rows complete the
picture. A report is written for every run except a `--dry-run` preview (§3.5),
including runs that abort with an error mid-execution — the latter via a
best-effort write of the data available up to the failure (§1). The per-artifact
Inputs/Outputs evidence columns (§5) and the `reused-output` driver are not yet
captured — distinguishing reuse from a callback advance needs a pre-run
output-existence snapshot from the engine, so no-spawn advances are reported as
`callback-only`. Until that lands, the report makes the artifact-reuse case
visible the coarse way: a run whose Outcome Strip shows `agent invocations: 0 ·
program invocations: 0` while tasks advanced is flagged in the report body as "no
agent or program ran," which is the signal the motivating issue asked for.

## 6. Canonical Rhei Example

The canonical fixture `examples/ui-test-canonical-example` should be the first
design target. It exercises agent states, program states, polling, failures,
gates, terminal-at-start rows, nested tasks, snapshot inheritance, generated
follow-up tasks, and fan-out review targets.

A completed canonical report starts like this:

```markdown
# Run Report: Rhei UI Canonical Test

Run: 2026-06-05T09:22:31Z / 9f24c6
Workspace: examples/ui-test-canonical-example
Command: rhei run . --parallel 4 --dashboard
Result: stopped for human attention
Latest dashboard: runtime/dashboard.html
Latest report: runtime/run-report.md

| Final states | Count |
| --- | ---: |
| completed | 9 |
| human-gate | 2 |
| blocked | 5 |
| cancelled | 1 |

| Activity | Count |
| --- | ---: |
| agent invocations | 7 |
| program invocations | 15 |
| callback-only transitions | 0 |
| reused-output transitions | 0 |
| terminal at start | 2 |
| could not advance | 7 |
```

The attention section should make the current stopping condition obvious:

```markdown
## Attention

| Task | State | Reason | Next action |
| --- | --- | --- | --- |
| ui-test-canonical-example.full-pipeline | human-gate | counted fix loop finished | inspect runtime/fixes/ui-test-canonical-example.full-pipeline-visit-2.md and transition manually |
| ui-test-canonical-example.human-gate | human-gate | seeded gating state | transition manually when reviewed |
| ui-test-canonical-example.poll-exhaustion | blocked | pollAttempts reached pollMaxAttempts | inspect runtime/logs/task-ui-test-canonical-example.poll-exhaustion-poll-exhaust.log |
| ui-test-canonical-example.live-failure-blocked | blocked | program exited 42 and matched blocked transition | inspect runtime/failures/ui-test-canonical-example.live-failure-blocked.md |
| ui-test-canonical-example.skill-unavailable-blocked | blocked | required skill absent-lens unavailable | install skill or mark task cancelled |
| ui-test-canonical-example.mcp-unavailable-blocked | blocked | required MCP server mock-mcp unavailable | start server or mark task cancelled |
| ui-test-canonical-example.blocked-seeded | blocked | seeded blocked state remained non-terminal | inspect task owner |
```

The transition ledger should make spawned work and artifact checks compact:

```markdown
## Transition Ledger

| Task | From | To | Driver | Inputs | Outputs | Invocation | Reason |
| --- | --- | --- | --- | --- | --- | --- | --- |
| ui-test-canonical-example.full-pipeline | collect-inputs | script-normalize | agent | - | raw-inputs: created, raw-notes: created | mock-agent[yolo]:mock:ui-implementer / runtime/logs/task-ui-test-canonical-example.full-pipeline-collect-inputs-mock-agent-yolo-mock-ui-implementer.log | exit 0, outputs present |
| ui-test-canonical-example.full-pipeline | script-normalize | mock-implement | program | raw-inputs: ok, raw-notes: ok | normalized-inputs: created, io-map: created | python3 ./bin/mock-program.py normalize / runtime/logs/task-ui-test-canonical-example.full-pipeline-script-normalize.log | exit 0 |
| ui-test-canonical-example.full-pipeline | parallel-review | aggregate | agent | build-report: ok | review-findings: created x2 | 2 targets / runtime/logs/task-ui-test-canonical-example.full-pipeline-parallel-review-*.log | all target outputs present |
| ui-test-canonical-example.polling | script-poll | script-poll | program | - | poll-ready: missing | python3 ./bin/mock-program.py poll / runtime/logs/task-ui-test-canonical-example.polling-script-poll.log | exit 75, retry scheduled |
| ui-test-canonical-example.poll-exhaustion | poll-exhaust | blocked | program | - | poll-pending: created | python3 ./bin/mock-program.py poll-exhaust / runtime/logs/task-ui-test-canonical-example.poll-exhaustion-poll-exhaust.log | pollAttempts >= pollMaxAttempts |
| ui-test-canonical-example.terminal-completed | completed | - | terminal-at-start | - | not-checked | none | already terminal |
```

To model the motivating artifact-reuse case, the canonical fixture should
also have a report test variant with pre-created outputs. The ledger would show:

```markdown
| Task | From | To | Driver | Inputs | Outputs | Invocation | Reason |
| --- | --- | --- | --- | --- | --- | --- | --- |
| ui-test-canonical-example.full-pipeline | collect-inputs | script-normalize | reused-output | - | raw-inputs: reused, raw-notes: reused | none | required outputs existed before run |
| ui-test-canonical-example.full-pipeline | script-normalize | mock-implement | reused-output | raw-inputs: ok, raw-notes: ok | normalized-inputs: reused, io-map: reused | none | required outputs existed before run |
```

This is the central UI requirement: no reader should confuse reused artifacts
with fast agent work.

## 7. Dashboard Affordance

The live and frozen Flow dashboard may link to the latest report, but the report
is the durable source for run explanation. The dashboard should add a small
`Report` action near the existing dashboard/run links:

- During a run, the action is disabled or points to a draft only if the draft is
  already durable.
- After the run finishes, it opens `runtime/run-report.md` through the
  same local open route used for artifacts and logs.
- In a frozen dashboard, it points to the relative report path if the report
  exists beside the dashboard artifact.

The dashboard should not duplicate the full Markdown report. It should surface
the same counts as a compact run summary and keep detailed forensic reading in
the report file.

## 8. Data Requirements

The execution engine must retain enough per-run facts to render the report:

- run id, command line, workspace root, start/end timestamps, duration, options,
  and final exit category;
- task ids, titles, hierarchy, state at run start, state at run end, and whether
  the task was terminal at start;
- every selected transition with source state, destination state, trigger
  reason, callbacks, and whether the transition was committed;
- every input/output artifact resolved for each decision, with pre/post
  existence status and optionality;
- every spawned agent/program, including resolved agent/target/model label,
  command label, exit status, duration, log path, accounting path, and output
  paths;
- every halted task with the most concrete blocker known to the scheduler;
- the run's own accounting rollup and the workspace lifetime total, told apart,
  along with which of the two sources produced the strip (§2.1).

These facts already overlap heavily with the run event surface and transition
journal, but the report should not be reconstructed only from
`runtime/transitions.log`. The journal is a tail-friendly event log; the report
is an end-of-run explanation with artifact and blocker context. [§FS-rhei-run-tui](rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal)

## 9. Non-Goals

- No interactive controls in the Markdown report.
- No remote report server.
- No hidden dependence on the browser dashboard being enabled.
- No content validation of artifacts beyond the existing file-existence
  contract.
- No attempt to summarize complete agent transcripts; the report links to logs.

## Related Specifications

- [Rhei Run](rhei-run.spec.md) - scheduler, transition, and completion behavior
- [Run TUI](rhei-run-tui.spec.md) - live event surface and transition journal
- [Flow Visualization](rhei-viz.spec.md) - browser dashboard that may link to the report
- [Agents](rhei-agents.spec.md) - completion authority and output contracts
- [Program States](rhei-programs.spec.md) - deterministic subprocess states
