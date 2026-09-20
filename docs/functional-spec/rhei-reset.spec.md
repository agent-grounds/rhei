# FS-rhei-reset: `rhei reset`

Return every task in a plan to the state it was **authored** in and remove runtime output. This is the inverse of a forward run: it restores the plan to a clean, pre-execution state so the same plan can be re-executed from scratch.

## 1. Usage

```bash
rhei reset <RHEI_PLAN_OR_WORKSPACE> [--rhei <RHEI_ID>] [--dry-run] [--yes]
```

`<RHEI_PLAN_OR_WORKSPACE>` may be a `.rhei.md` file (single-file plan), a directory workspace root (containing `index.rhei.md` and `tasks/`), or a Panta project directory.

### 1.1. Options

| Flag               | Default | Description                                                                    |
|--------------------|---------|--------------------------------------------------------------------------------|
| `--rhei <RHEI_ID>` | all     | Narrow the reset to the named rheis (repeatable). See §2.1. [§FS-rhei-panta.6.4](rhei-panta.spec.md#64-reset-validate-list-viz) |
| `--dry-run`        | false   | Report what would be reset and deleted, then exit without changing anything    |
| `--yes`, `-y`      | false   | Confirm without prompting; required when stdin is not a terminal. See §1.2     |

An id that names no rhei in the project is an error listing the available rhei
ids.

### 1.2. Confirmation

Reset deletes result artifacts and ledgers that have no other copy: `rhei init`
gitignores `panta/` by default ([§FS-rhei-init](rhei-init.spec.md#fs-rhei-init-rhei-init)), so the destroyed material is
typically absent from version control.

Before acting, reset prints what it would reset and the runtime directories it
would delete. That preview precedes every destructive reset, including `--yes`
and non-interactive invocations: they destroy the same artifacts, so printing
only on the path that stops to ask would make exactly the unattended runs the
silent ones.

Unless `--yes` was passed, reset then asks for confirmation and treats any
answer other than `y`/`yes` as a cancellation, changing nothing. When stdin is
not a terminal there is no one to ask, so reset **fails** rather than assuming
consent, naming `--yes` to confirm and `--dry-run` to preview. Unattended
callers — scripts, CI, and agents driving the CLI — cannot see a prompt, so
inferring agreement from their silence turned every accidental invocation into
irreversible loss of material that is typically absent from version control.
Deliberate automation states the intent once with `--yes`.

## 2. Behavior

1. Load the state machine and plan. Validate the plan (reset refuses to operate on an invalid plan).
2. Resolve the whole selected scope and acquire its canonical sibling sidecars
   in the shared order: sorted and deduplicated metadata paths, then distinct
   task paths, then ledger roots. Leave every replaceable destination unlocked.
   Re-read the current plan and ledger pathnames only after their sidecars are
   held, and retain the complete stack through preview, confirmation,
   restoration, scoped pruning or full cleanup, and the success summary.
3. For every task node in the merged task graph (including all descendants):
   - Recover the task's authored state (§2.2) and rewrite the task's
     `**State:**` line to it. A task that never moved is already in that state,
     so the line keeps its state name — but it is still rewritten in normalized
     form, because the counted-visit suffix below is runtime state and is
     cleared whether or not the state name changes.
   - Remove the `**Assignee:**` line if present.
   - Remove the `> **Result:**` link block from the task body if present.
   - Clear any counted-visit suffix; `stateVisits` entries and provider-limit
     waits for the task in frontmatter `metadata.tasks.<id>.stateVisits` and
     `metadata.tasks.<id>.providerLimits` are deleted, together with the task's
     `supervision` block ([§FS-rhei-supervision.3.3](rhei-supervision.spec.md#33-supervision-metadata)). A `metadata.tasks.<id>` entry left empty by those deletions is removed as well, and so are `metadata.tasks` and `metadata` when nothing else remains in them: an empty entry is a record of nothing, and the next reader would have to decide whether it meant something.
4. After every plan file is rewritten — the ledger step 3 reads lives there — for a directory workspace, delete the `runtime/` directory at the workspace root if it exists. For a single-file plan, delete the `runtime/` directory next to the plan file if it exists. This removes result files, findings, logs, and journaled transition records.
5. Write each modified task file atomically (temp file + rename). Release the
   sidecar stack in reverse order.

Reset does **not**:

- Modify the `# Rhei:` title, content sections, `**Prior:**` lines, or task descriptions.
- Remove user-authored files outside of `runtime/`.
- Alter the state machine or template source of the plan.

Reset is project-wide by default. Because it destroys runtime state across
every in-scope rhei, it reports its resolved scope and the affected rheis
before acting; a one-rhei project has no fan-out to report and stays quiet
([§FS-rhei-panta.6.4](rhei-panta.spec.md#64-reset-validate-list-viz)).

### 2.1. Narrowed Reset (`--rhei`)

A narrowed reset must never delete whole `runtime/` trees: sibling single-file
rheis share one execution root, so removing the tree would destroy an
out-of-scope rhei's state. Instead it removes exactly what is **keyed by an
in-scope ticket id**, under that ticket's owning rhei execution root — and,
when they differ, under the project execution root as well, because
run-orchestrated logs and captures land there even for tickets owned by a
subdirectory rhei:

- `runtime/results/<ticket-id>.md`
- `runtime/logs/task-<ticket-id>-*`
- every artifact the resolved state machine declares as an `inputs:`/`outputs:`
  path containing `{task_id}` — a stale output left behind would otherwise
  satisfy a required input on the next run
- `runtime/snapshot-sessions/<ticket-id>-*`
- `runtime/worktree-refs/<ticket-id>.yaml`
- `runtime/accounting/captures/<ticket-id>-*` and
  `runtime/accounting/tasks/<ticket-id>.json`
- the ticket's lines in `runtime/state-transitions.log`, so a reset ticket's
  recorded history cannot claim a completion its plan no longer holds

A forced transition's `!force-v1` metadata row and immediately adjacent
movement row are one task-keyed record (§FS-rhei-complete.3.1). Narrowed reset
validates the pair and removes both rows together; it never leaves exceptional
metadata orphaned or prunes only the movement.

Artifact paths that still carry unresolved placeholders after `{task_id}` is
substituted (`{state}`, `{visit_count}`, `{model}`, …) are matched by the
literal prefix up to the first remaining placeholder, so `auth.1` never matches
`auth.10`.

Run-scoped output is **not** ticket-owned — the run report, the dashboard, and
the accounting rollups describe a run, not a ticket — so a narrowed reset keeps
it and says so on stdout. Reset without `--rhei` to clear it.

The reported moves are the moves this invocation will actually make, resolved
over in-scope tickets only. A dry run is read as a promise about what changes,
so it must not describe work outside its own scope: an earlier summary of every
machine's initial state announced `(pending, review)` under `--rhei billing`
while `review` belonged to rheis the command was about to leave alone. Naming
the moved tickets keeps that impossible — a ticket outside the scope has no
line to print. With machines per rhei ([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding)), each in-scope ticket
resolves its authored state through its own rhei's ledger and machine.

### 2.2. Authored State

A task's **authored state** is the state its plan gave it before anything ran.
For most plans that is the resolved profile's `initial` state and the two are
the same thing. They are not the same thing for a pre-authored chain — the
shape [§FS-rhei-supervision.7](rhei-supervision.spec.md#7-example) documents and the `supervised-delivery` template
ships — where one task sits in `supervising` and its children are authored in
`implement`, `review`, `fix`, … Those children never were in `supervising`, so
sending them there is not a reset.

Nothing in a state machine can express that chain as profiles, either:
`node_policy` resolves a profile from a node's **kind and level**
([§FS-rhei-states.9.2](rhei-states.spec.md#92-resolution)), and these children share both. The authored state is
per-task, so reset recovers it per-task.

The record it recovers from is the central transition ledger,
`runtime/state-transitions.log` — the one place every verb that moves a ticket
appends to ([§FS-rhei-viz.4](rhei-viz.spec.md#4-surroundings-inspector)). Ordinary movement rows are
`<task-id> <from>@<to>`; forced metadata rows are paired as specified by
§FS-rhei-complete.3.1. Authored-state reconstruction ignores metadata rows and
reads each validated pair's movement exactly once, so the **first `from`
recorded for a task is the state that task started in**. Reset reads the ledger
before it deletes it, and for each in-scope task:

- **The task has ledger lines.** Its authored state is the first `from`. Reset
  rewrites `**State:**` back to it.
- **The task has no ledger lines.** It never moved, so it is still in its
  authored state and reset leaves the line untouched. This is what keeps a
  pre-authored chain intact through a reset that follows a run in which only
  the supervisor moved.
- **The task's execution root has no ledger** — a plan that never ran, or one
  whose `runtime/` was removed by hand. These are the same picture from here,
  so the task is left where it is. Ledger presence is judged **per execution
  root**: one rhei's history says nothing about another's, and a project where
  only some rheis ran must not treat the others as having stood still.

  Reset then **names the tasks it left outside their profile's `initial`
  state** (§4), which is the only recourse it can offer: with nothing recording
  where they came from, only the operator knows. It names only tasks a run
  plausibly touched — those whose execution root still holds runtime output, or
  that carry a claim, a result link, or a counted-visit suffix. A pre-authored
  chain's children are authored outside `initial` by construction, so listing
  every one of them on an ordinary reset of a plan that never ran would cry
  wolf on each and bury the one task that is genuinely stale.

  That test is deliberately best-effort. A `rhei transition` into a non-final
  state leaves nothing behind but the state itself, which is exactly what a
  hand-authored plan looks like, so a task moved that way and then stripped of
  its whole `runtime/` directory goes unnamed. Missing one costs a line of
  report; naming one wrongly would put every child of every supervised chain on
  the list, which is the failure that makes a report stop being read.

  A task named this way keeps its state while its results and logs are removed
  with the rest of the runtime output, so it can be left reading `completed`
  with nothing behind it. That is the honest end of an ambiguous case, and it
  is why the report says so: a task left in a state the operator can edit is
  recoverable, a task silently moved to a state it was never authored in is
  not.

- **The recorded state is one the machine no longer declares** — it was renamed
  since the run. Writing it back would leave a plan that no longer validates,
  and the ledger that explained it is deleted moments later, so reset keeps the
  task where it stands and says which state it could not restore (§4).

Reset therefore never invents a state a task did not hold. It moves a task only
where the ledger says that task has been.

## 3. Safety

Reset is destructive with respect to runtime state: it deletes results, exports, logs, and the transition ledger. It is not destructive with respect to authored plan content — §2.2 moves a task only to a state that task's own history records, so a reset can never invent a state and can never be worse for the plan than not running it. `--dry-run` previews both halves (§1.1) and confirmation gates the rest (§1.2).

Because reset holds its whole selected-scope sidecar stack over the decision
snapshot, it is safe against concurrent `rhei next` / `rhei transition` /
`rhei complete` calls: those calls run wholly before the preview or after the
reset persistence boundary, never between consent and destruction. A cancelled
or refused reset releases without mutation. Persistent plan and ledger
sidecars, and the directories required to preserve their pathname identities,
survive scoped pruning and full runtime cleanup.

Reset first obeys the root-access interlock of §FS-rhei-recover.4. A pending
forced-recovery marker refuses both preview and mutation and prints the one
recovery invocation; reset never repairs or deletes it. With no marker, reset
holds shared root guards and its ordinary stable file locks, so concurrent
`rhei next`, `rhei transition`, `rhei complete`, and forced writers run wholly
before or after its decision snapshot.

Reset never removes or rewrites `budgetProjectId`, `budgetTicketId`, the
`.agent-grounds/rhei/budgets/` journal or lock, consumed travel, settled spend,
outstanding exposure, adjustments, breaches, or qualification history. A full
or narrowed reset does not replenish any bound; reset and admission use the
global lock order in §AR-neural-admission.3.

## 4. Output

On success, reset reports how many tasks it cleared, **which tasks it moved and
where from**, and what runtime output it removed:

```text
Reset <N> task(s) to their authored states.
Moved <K> task(s) back:
  Task <id>: <current> → <authored>
Removed runtime output.
```

A count alone cannot be checked. `Reset 7 task(s) to initial state
'supervising'` was true of the run that corrupted a supervised chain and true
of the run that did nothing, so the operator had no way to tell those apart
until the next dispatch. Naming each move makes the two read differently.

When no task had moved from its authored state — a plan whose only moves were
the supervisor's self-loops, say — the middle block is one line instead:

```text
No task had moved from its authored state.
```

Tasks reset could not account for are named after the move list, whether or
not anything moved:

```text
Nothing records where these <K> task(s) came from, so they were left as they stand, without the results and logs the rest of this reset removed:
  Task <id>: <state>
Edit their **State:** lines directly if that is not where they should be.
```

and a state the machine has since dropped gets one line per task:

```text
Task <id> started in '<recorded>', which this state machine no longer declares; left in '<current>'.
```

When the task graph contains child tasks, the first line also reports the
descendant count:

```text
Reset <N> task(s) (and <M> descendant task(s)) to their authored states.
```

The last line is `No runtime output was present.` when the `runtime/`
directory did not exist.

`--dry-run` and the pre-confirmation preview (§1.2) print the same move list
under `Would move <K> task(s) back:`, so what the preview promises and what the
reset reports are the same text.

A project-scoped invocation prints its resolved scope first:

```text
Scope: `rhei reset` operates project-wide across <N> rheis: <ids>
```

and a narrowed one reports what it deliberately kept after the two summary
lines:

```text
Scope: `rhei reset` narrowed to <N> rheis: <ids>
Reset <N> task(s) to their authored states.
Moved <K> task(s) back:
  Task <id>: <current> → <authored>
Removed runtime output.
Kept run-scoped output not owned by any ticket (run report, dashboard, accounting rollups). Reset without `--rhei` to clear it.
```

## Relationship to Other Commands

`rhei reset` inverts the forward commands (`next`, `transition`, `complete`, `run`): it returns every task to the state it was authored in (§2.2) and removes the `runtime/` directory.

See [How Rhei Is Used — Command Surface](rhei-usage.spec.md#22-command-surface) for the full table comparing all five coordination commands.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — plan formats and semantic constraints
- [States Specification](rhei-states.spec.md) — profile resolution and `initial` state rules
- [Supervision Specification](rhei-supervision.spec.md) — the pre-authored chain §2.2 exists to preserve
- [Visualization Specification](rhei-viz.spec.md) — the transition ledger §2.2 recovers authored states from
- [Next Command](rhei-next.spec.md), [Complete Command](rhei-complete.spec.md), [Transition Command](rhei-transition-cmd.spec.md) — forward commands that reset inverts
- [Release Command](rhei-release.spec.md) — drops one claim without destroying runtime output
