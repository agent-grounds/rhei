# FS-rhei-reset: `rhei reset`

Return every task in a plan to the state it was **authored** in and remove runtime output. This is the inverse of a forward run for *authored state*: it restores the plan to a clean, pre-execution state so the same plan can be re-executed.

It is **not** the inverse of a forward run for *capacity*. Reset returns no
ticket travel and no project invocations: both are bounded over the lifetime of
an identity rather than over one execution, and an inverse that returned them
would make `rhei reset` the way to buy more ([§FS-rhei-budgets.4](rhei-budgets.spec.md#4-what-spends-a-count)).
Reset-and-rerun therefore converges on the travel bound instead of escaping it.
That is stated here, in the lead, because "re-executed from scratch" is what a
reader takes away from this page and it is no longer true of everything.

## 1. Usage

```bash
rhei reset <RHEI_PLAN_OR_WORKSPACE> [--rhei <RHEI_ID>] [--dry-run] [--yes]
```

`<RHEI_PLAN_OR_WORKSPACE>` may be a `.rhei.md` file (single-file plan), a directory workspace root (containing `index.rhei.md` and `tasks/`), or a Panta project directory.

### 1.1. Options

| Flag               | Default | Description                                                                    |
|--------------------|---------|--------------------------------------------------------------------------------|
| `--rhei <RHEI_ID>` | all     | Narrow the reset to the named rheis (repeatable). See [§FS-rhei-reset.2.1](rhei-reset.spec.md#21-narrowed-reset---rhei). [§FS-rhei-panta.6.4](rhei-panta.spec.md#64-reset-validate-list-viz) |
| `--dry-run`        | false   | Report what would be reset and deleted, then exit with every file as it was. What asking the filesystem still costs is in [§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse) |
| `--yes`, `-y`      | false   | Confirm without prompting; required when stdin is not a terminal. See [§FS-rhei-reset.1.2](rhei-reset.spec.md#12-confirmation)     |

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
   The check is the one `rhei complete` makes before it moves a ticket: the plan's
   semantics under the state machines reset resolved, as `rhei validate` checks
   them ([§FS-rhei-validate.4](rhei-validate.spec.md#4-behavior) step 2), over the whole project graph whatever `--rhei`
   names ([§FS-rhei-panta.6.4](rhei-panta.spec.md#64-reset-validate-list-viz)). Links to files are not checked, because the runtime
   output they may point at is what a reset deletes on purpose ([§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse)),
   and settings are not read, because a reset runs no agent. A plan that fails it
   is refused before any preview, `--dry-run` included, and nothing is changed: no
   `**State:**` line, no ledger row, no result, no log ([§FS-rhei-reset.4](rhei-reset.spec.md#4-output)).
   The plan judged is the plan as it stands, so a plan only the reset would make
   valid again is refused too. One written under another machine through
   `--state-machine` validates under that machine, and a reset given the same
   `--state-machine` goes through.
   A pending forced recovery ([§FS-rhei-recover.4](rhei-recover.spec.md#4-pending-root-interlock)) and a pending removal
   ([§FS-rhei-remove.6.2](rhei-remove.spec.md#62-pending-removal)) are refused first: each marker names the one command that
   resumes, and the operation it left unfinished may be what makes the plan invalid. A destructive
   reset decides again on the plan it re-reads under the locks of step 2.
2. Resolve the whole selected scope and acquire its canonical sibling sidecars
   in the shared order: sorted and deduplicated metadata paths, then distinct
   task paths, then ledger roots. Leave every replaceable destination unlocked.
   Re-read the current plan and ledger pathnames only after their sidecars are
   held, and retain the complete stack through preview, confirmation,
   restoration, scoped pruning or full cleanup, and the success summary.
   Before the preview, refuse with nothing changed a reset whose moves would
   leave a plan validation refuses, or that the filesystem would stop
   ([§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse)).
3. For every task node in the merged task graph (including all descendants),
   reading the plan's text by the fence rule, so that a fenced line is none of the
   lines this step names ([§FS-rhei-reset.2.4](rhei-reset.spec.md#24-fenced-lines)):
   - Recover the task's authored state ([§FS-rhei-reset.2.2](rhei-reset.spec.md#22-authored-state)) and rewrite the task's
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
     Both the deletions and that pruning are confined to the task's **writable**
     metadata document: the plan itself for a single-file rhei,
     `index.rhei.md` for a Directory Workspace ([§FS-rhei-plan-language.1.4](rhei-plan-language.spec.md#14-directory-workspace-metadata)),
     and the project manifest `index.panta.md` for a basin ticket, keyed by its
     project-qualified id (for example, `basin.1` or `basin.1.1`)
     ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)).
     Clear these keys for every in-scope ticket and descendant, including those
     already at their authored state, both project-wide and with `--rhei basin`.
     A reset basin ticket can therefore enter a counted state again with a fresh
     visit count. A narrowed reset leaves out-of-scope metadata in the shared
     manifest unchanged. Keys the register does not mark cleared, including
     `pollNextAttemptAt` and `budgetTicketId`, and authored or unknown keys survive
     ([§FS-rhei-transitions.2.5](rhei-transitions.spec.md#25-keys-rhei-writes));
     so does `metadata.retiredTickets` as specified below.
     A workspace task file's own metadata block is authored, holds none of the
     runtime keys named above, and is neither read nor rewritten by reset: it
     survives byte-identical.
4. After every plan file is rewritten — the ledger step 3 reads lives there — for a directory workspace, delete the `runtime/` directory at the workspace root if it exists. For a single-file plan, delete the `runtime/` directory next to the plan file if it exists. This removes result files, findings, logs, and journaled transition records.
   It removes the run-input seals under `runtime/seal/` with the rest
   ([§FS-rhei-seal.6](rhei-seal.spec.md#6-lifecycle)).
5. Write each modified task file atomically (temp file + rename). A file whose
   text the reset does not change is not written. Release the sidecar stack in
   reverse order.

Reset does **not**:

- Modify the `# Rhei:` title, content sections, `**Prior:**` lines, or task descriptions.
- Remove user-authored files outside of `runtime/`.
- Alter the state machine or template source of the plan.
- Touch the project's budget account. It lives at
  `.agent-grounds/rhei/budgets/`, which is outside `runtime/` and outside every
  deletion step 3 and step 4 enumerate ([§FS-rhei-budgets.5.1](rhei-budgets.spec.md#51-where-it-lives)).
- Delete the project's retirement record, `metadata.retiredTickets`
  ([§FS-rhei-remove.5.1](rhei-remove.spec.md#51-where-it-is-recorded)). A reset returns live tickets to their authored state;
  a retired id has no live ticket to return, and clearing the record would let
  the next `rhei new` reissue it. Reset is also refused while a removal is
  pending (`.rhei/pending-removal.json`), and its diagnostic prints the
  `rhei remove` invocation that resumes it ([§FS-rhei-remove.6.2](rhei-remove.spec.md#62-pending-removal)).
- Delete the ticket's `metadata.tasks.<id>.budgetTicketId`. Which of rhei's own
  metadata keys reset deletes is the register's `Cleared by rhei reset` column
  ([§FS-rhei-transitions.2.5](rhei-transitions.spec.md#25-keys-rhei-writes)), not a
  second list kept here; the budget identity is deliberately **not** among them,
  because the document must not be made to disagree with the ledger. Removing the
  key buys no travel: a ticket whose document names no identity is resolved from
  the binding the ledger already holds for it rather than minted a new one
  ([§FS-rhei-budgets.5.2](rhei-budgets.spec.md#52-the-journal)), so it converges
  on the bound it has already spent, which is what the lead says of
  reset-and-rerun. What it does leave is a ledger holding a binding the document
  no longer names, for a later reader to reconcile. That is also why a
  `metadata.tasks.<id>` entry holding only that key is **not** removed as empty:
  unlike an entry emptied by step 3, it is a record of something.

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
- the exact `runtime/exports/<ticket-id>/` directory and everything inside it,
  independently of any state-machine `inputs:`/`outputs:` declaration. These
  plan-level `**Provides:**` exports must be written anew after reset: a
  previous run's export must not satisfy the next completion check
  ([§FS-rhei-plan-language.3.12.3](rhei-plan-language.spec.md#3123-producer-completion-obligation))
- `runtime/logs/task-<ticket-id>-*`, and the spawn records beside them in
  `runtime/spawns/task-<ticket-id>-*`
- `runtime/reports/task-<ticket-id>-*`, the session reports rendered from those
  logs ([§FS-rhei-session-reports.1](rhei-session-reports.spec.md#1-report-artifact)), including any renamed aside as
  `.orphaned-<unix-ts>.md`; a report left behind would sit at the stem the
  ticket's next entry is about to use ([§FS-rhei-agents.8.1](rhei-agents.spec.md#81-log-file-naming))
- every artifact the resolved state machine declares as an `inputs:`/`outputs:`
  path containing `{task_id}` — a stale output left behind would otherwise
  satisfy a required input on the next run
- `runtime/snapshot-sessions/<ticket-id>-*`
- `runtime/worktree-refs/<ticket-id>.yaml`
- `runtime/accounting/captures/<ticket-id>-*` and
  `runtime/accounting/tasks/<ticket-id>.json`
- the ticket's lines in `runtime/state-transitions.log`, so a reset ticket's
  recorded history cannot claim a completion its plan no longer holds; its
  entry numbers are counted from those lines, so its next entry into any state
  is entry 1 again and takes the plain log name ([§FS-rhei-agents.8.1](rhei-agents.spec.md#81-log-file-naming))

A narrowed reset removes **no budget receipts**, for the same reason: the
account is per project, a receipt is not keyed by a ticket id, and a narrowing
that could drop a ticket's travel would make `--rhei` the faucet
([§FS-rhei-budgets.11](rhei-budgets.spec.md#11-what-does-not-change)).

A forced transition's `!force-v1` metadata row and immediately adjacent
movement row are one task-keyed record ([§FS-rhei-complete.3.1](rhei-complete.spec.md#31-state-transition-ledger)). Narrowed reset
validates the pair and removes both rows together; it never leaves exceptional
metadata orphaned or prunes only the movement.

Artifact paths that still carry unresolved placeholders after `{task_id}` is
substituted (`{state}`, `{visit_count}`, `{model}`, …) are matched by the
literal prefix up to the first remaining placeholder, so `auth.1` never matches
`auth.10`.

The export directory is matched by its complete ticket id, never by a prefix:
resetting `auth.1` does not remove `runtime/exports/auth.10/`. Pre-qualification
export directories named by a rhei-local id follow the same guarded legacy
sweep as other ticket-owned runtime records: remove them only at an execution
root whose every rhei is in scope; keep ambiguous shared-root local ids
([§FS-rhei-panta.6.4](rhei-panta.spec.md#64-reset-validate-list-viz)).

Reset is not the only command that deletes ticket-owned runtime files:
`rhei remove` does too, but only empty residue of a ticket that has no history
([§FS-rhei-remove.4.2](rhei-remove.spec.md#42-residue)). The two never overlap — reset prunes history and keeps
the definition, removal keeps every byte of history and deletes the definition.

Run-scoped output is **not** ticket-owned — the run report, the dashboard, and
the accounting rollups describe a run, not a ticket — so a narrowed reset keeps
it and says so on stdout. Reset without `--rhei` to clear it.

Run evidence ([§FS-rhei-evidence](rhei-evidence.spec.md#fs-rhei-evidence-per-invocation-evidence)) is run-scoped too, so a narrowed reset keeps
`runtime/evidence/` whole and says so. Because the ticket's next entry takes a
log name an earlier run's manifest names, it writes one reset event naming
every path it removed ([§FS-rhei-evidence.10.1](rhei-evidence.spec.md#101-the-reset-event)), and `rhei verify` reports
those paths as pruned for the earlier runs. A full reset deletes the store with
the rest of `runtime/` (step 4) and writes nothing.

The project note store is kept for the same reason and reported the same way
([§FS-rhei-note.3.1](rhei-note.spec.md#31-where-it-lives)). It is one shared journal charged to every plan, not a
ticket's artifact: it is not keyed by a ticket id, an in-scope ticket's entry
may be the only record of a fact an out-of-scope ticket is relying on, and
removing one ticket's lines would mean rewriting bytes that append-only exists
to protect. A narrowed reset therefore keeps `runtime/notes.md` whole and says
so on stdout; reset without `--rhei` removes it with the rest of `runtime/`.

The run-input seals under `runtime/seal/` are kept for the same reason and
reported the same way ([§FS-rhei-seal.6](rhei-seal.spec.md#6-lifecycle)). A seal is keyed by rhei, not by ticket;
records that survive the narrowed reset may name its generations; and a second
generation 1 at the same path would break its write-once rule. Generation
numbers therefore never restart while `runtime/` exists. For a fresh baseline
on one rhei, run `rhei seal --rhei <id>`.

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
[§FS-rhei-complete.3.1](rhei-complete.spec.md#31-state-transition-ledger). Authored-state reconstruction ignores metadata rows and
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
  state** ([§FS-rhei-reset.4](rhei-reset.spec.md#4-output)), which is the only recourse it can offer: with nothing recording
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
  task where it stands and says which state it could not restore ([§FS-rhei-reset.4](rhei-reset.spec.md#4-output)).

Reset therefore never invents a state a task did not hold. It moves a task only
where the ledger says that task has been.

### 2.3. A Reset Validation Would Refuse

Every destination [§FS-rhei-reset.2.2](rhei-reset.spec.md#22-authored-state) decides is one task's, read from that task's own
ledger rows and its own machine. A plan is more than its tasks one at a time, and
the destinations can add up to a plan `rhei validate` refuses although every one of
them is right for its task:

- A finished task the reset leaves where it stands, over a descendant it reopens.
  The finished task has no ledger row because its line was edited by hand, or the
  row names a state the machine no longer declares, which is also what happens when
  the run and the reset resolve different machines. Validation refuses a terminal
  task over a descendant that is not ([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)).
- A ledger row that sends a task back to a terminal state it started in, over a
  child that never moved and is not terminal.
- A rule that has nothing to do with the tree: a recorded state the machine still
  declares but the task's profile no longer allows.

So reset asks the validator rather than copying one of its rules. Before it
prints a preview, it validates the plan as it would leave it, under the machine
set it resolved: every in-scope task in the state step 3 writes, with no
`**Assignee:**` line and no `> **Result:**` link, and every other task as it
stands. An error that validation reports and that the plan as it stands does not
already carry is one the reset would **introduce**. An error the plan already
carries is not the reset's doing: it is the invalid plan step 1 refuses. Links to
files are not checked here, because the runtime output they may point at is what
a reset deletes on purpose.

When a reset would introduce an error, it refuses and changes nothing: no
`**State:**` line, no ledger row, no result, no log. It decides this under the
locks of step 2 and before its preview, so the preview never promises a deletion
that is not going to happen, and a cancelled or refused reset still releases
without mutation ([§FS-rhei-reset.3](rhei-reset.spec.md#3-safety)). `--dry-run` refuses the same way, with the same text,
and exits non-zero: a dry run is read as a promise about what the reset does.
A plan the reset cannot rewrite fails the same way, before any preview,
`--dry-run` included, and changes nothing. A rewrite can fail in two places, so
the check has two halves. The plan it would leave is written in memory before it
is validated, so a rewrite that cannot be computed fails there. A rewrite the
filesystem would refuse cannot be seen in memory, so the reset also establishes,
under the same decision, that it can make every entry it would make and remove
every one it would remove:

- every file whose text it would change, in the directory holding that file;
- every lock sidecar of step 2: one that is not there yet in the directory it would
  be created in, and one that is by opening it as the lock does, without truncating
  it or taking the lock;
- for a full reset, each `runtime/` tree it would delete: the directory holding the
  tree and every directory inside it, which has to be listable, and writable where
  it holds an entry, because removing a tree takes both;
- for a narrowed reset, what [§FS-rhei-reset.2.1](rhei-reset.spec.md#21-narrowed-reset---rhei) would remove, in the directory holding it, the
  ledger's lines included.

It asks each directory by doing what the write itself does, creating a temp file
there and dropping it, and that has a known cost, for `--dry-run` too ([§FS-rhei-reset.1.1](rhei-reset.spec.md#11-options)). A run
that finishes leaves no entry behind, and no file's name, mode or bytes change, but
the modification time of each directory it asks moves. A run killed while it asks
can leave a temp file there. A dry run alongside a real reset can stop that reset
part way, because its temp file can be in a directory the reset is emptying, and a
second reset then finishes. The refusal names every directory and lock file that
fails, in [§FS-rhei-reset.4](rhei-reset.spec.md#4-output). A directory with nothing to write, create or remove is not asked: it
does not matter whether a reset could write where it has nothing to write, so a
read-only directory holding only files the reset leaves as they are does not
refuse it, and neither does a directory a narrowed reset has no business in.

The check is not a promise about the future. A refusal it cannot foresee, a disk
filling or a permission changed between the check and the write, can still stop a
reset part way, and a reset stopped that way says so ([§FS-rhei-reset.4](rhei-reset.spec.md#4-output)). The plan it leaves
is a valid one, and a second reset finishes what the first left once the cause is
fixed, because every plan file is written before the ledger it reads is touched.

It does not reopen the finished task instead. That task has no recorded state to
go back to, and [§FS-rhei-reset.2.2](rhei-reset.spec.md#22-authored-state) moves a task only where the ledger says it has
been.

The refusal names every error it would introduce, in the validator's own
sentence, and keeps the line [§FS-rhei-reset.4](rhei-reset.spec.md#4-output) prints for a task whose recorded state
the machine no longer declares, which is the reason that task stays where it is.
Its help names a way on only where that way works. A finished task the reset
leaves where it stands is there because its `**State:**` line says so, so editing
that line lets the reset through. The help names that edit for each such task,
but only where making every one of those edits leaves nothing the reset would
introduce, and only with a state the edit can name: the task's profile's
`initial` state, when the machine declares it not terminal. Where such a task has
a `> **Result:**` link, the edit the help names deletes that line too, because
validation refuses a result block on a task that is not terminal, and the plan has
to be valid between the edit and the reset. Where the reset writes a state from
the ledger instead, no edit to a `**State:**` line changes where it goes, and the
help says that rather than offering one.

A message that offers the reset as a way back checks this first, by the same
computation, and does not offer a reset that would be refused ([§FS-rhei-reset.5.1](rhei-reset.spec.md#51-offering-reset-as-a-remedy)).

### 2.4. Fenced Lines

A reset rewrites a plan file's text line by line, and it reads that text by the
language's one fence rule ([§FS-rhei-plan-language.2.1](rhei-plan-language.spec.md#21-fenced-code-blocks)), as `rhei validate` does. It
recognizes a task heading, a `**State:**` line, an `**Assignee:**` line and a
`> **Result:**` line only outside a fenced code block. A plan `rhei validate`
accepts is therefore never refused over what a fence quotes, and the text of every
fenced line comes through a reset unchanged. Its line ending comes out as LF, as
with every plan writer:

- A quoted heading opens no task, so a reset looks for no `**State:**` line under
  it and a real line after the fence still belongs to the task above it.
- A quoted `**State:**` line is not rewritten, even under a quoted heading whose id
  is a task the reset moves. The real task's line is.
- A quoted `**Assignee:**` or `> **Result:**` line is not removed, and neither is
  the blank line before it.
- A quoted `> **Result:**` line is not a result link anywhere reset asks about one.
  It is no trace of a run ([§FS-rhei-reset.2.2](rhei-reset.spec.md#22-authored-state)) and not a line the help tells the
  reader to delete ([§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse)).

A fence that is never closed runs to the end of the text that opened it, which in
a Directory Workspace is one task file: the next file's lines are structure again.

Frontmatter is not text a fence is read in. A fence-shaped line in a YAML value
opens nothing, in a single-file plan, in `index.rhei.md`, and in a workspace task
file's own metadata block, because the parser does not read one there either.

A task file has no header, so its only frontmatter is a block that leads it. A
`# Rhei:` line anywhere in a task file is body text, and so is a `---` under it: the
tasks below them are tasks, and a reset moves them as `rhei validate` lists them.
Whether a file is a task file is what the load the reset chose it from read it as,
never where the file sits ([§FS-rhei-plan-language.2.1](rhei-plan-language.spec.md#21-fenced-code-blocks)): a plan given directly is a
plan even inside a workspace's `tasks/` directory.

`--dry-run`, the preview and the plan [§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse) judges read a fence the same way,
because each comes from the same rewrite.

## 3. Safety

Reset is destructive with respect to runtime state: it deletes results, exports, logs, and the transition ledger. It is not destructive with respect to authored plan content, and that takes two rules, because one task's history cannot see the others'. [§FS-rhei-reset.2.2](rhei-reset.spec.md#22-authored-state) moves a task only to a state that task's own history records, so a reset never invents a state. [§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse) refuses, with nothing changed, a reset whose moves would add up to a plan validation refuses, and one the filesystem would stop. Together they are why a reset can never be worse for the plan than not running it, short of the failure that point says no check can foresee, which a second reset recovers from. `--dry-run` previews both halves ([§FS-rhei-reset.1.1](rhei-reset.spec.md#11-options)) and confirmation gates the rest ([§FS-rhei-reset.1.2](rhei-reset.spec.md#12-confirmation)).

Because reset holds its whole selected-scope sidecar stack over the decision
snapshot, it is safe against concurrent `rhei next` / `rhei transition` /
`rhei complete` calls: those calls run wholly before the preview or after the
reset persistence boundary, never between consent and destruction. A cancelled
or refused reset releases without mutation. Persistent plan and ledger
sidecars, and the directories required to preserve their pathname identities,
survive scoped pruning and full runtime cleanup.

Reset first obeys the root-access interlock of [§FS-rhei-recover.4](rhei-recover.spec.md#4-pending-root-interlock). A pending
forced-recovery marker refuses both preview and mutation and prints the one
recovery invocation; reset never repairs or deletes it. With no marker, reset
holds shared root guards and its ordinary stable file locks, so concurrent
`rhei next`, `rhei transition`, `rhei complete`, and forced writers run wholly
before or after its decision snapshot.

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

A plan step 1 of [§FS-rhei-reset.2](rhei-reset.spec.md#2-behavior) refuses gets the diagnostic `rhei validate` prints for a
semantic validation failure ([§FS-rhei-validate.6](rhei-validate.spec.md#6-output)): every error in the validator's own
sentence, under the state-machine sources it used. It prints no preview and no
summary, and exits non-zero with or without `--dry-run`, because a dry run that
exits 0 reads as a promise that the reset will go through.

A reset [§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse) refuses prints no preview and no summary. It exits non-zero,
with or without `--dry-run`, and its message is the refusal, then one line per
error it would introduce, in the validator's words, then one line per task whose
recorded state the machine no longer declares:

```text
Refusing to reset: the plan it would leave fails validation, so nothing was changed.
  <error>
Task <id> started in '<recorded>', which this state machine no longer declares; a reset would leave it in '<current>'.
```

A reset the filesystem would stop ([§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse)) refuses the same way: no preview and no
summary, non-zero with or without `--dry-run`, and the same text for both. Its
message is the refusal, then one line per directory or lock file that fails, with
the operating system's own error. A lock file that exists and will not open is
named itself, because the directory holding it may be writable. A path prints in
one spelling throughout, relative to the working directory for a target typed
relative and absolute for a target typed absolute:

```text
Refusing to reset: it cannot write everywhere it needs to, so nothing was changed.
  <directory or lock file>: <error>
```

Its help is the one every failed plan write carries: rhei writes plan edits through
a temp file in the same directory, so check that directory is writable and has free
space. Where a line names a lock file, the help adds that the file itself has to
open for writing.

A write that fails after the first write has landed, which is the failure that
check cannot foresee, keeps the error it has and adds what has become true of the
plan:

```text
The reset stopped part way, so the plan is partly reset: fix the cause and run the reset again, which finishes what this one left.
```

The help of a reset [§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse) refuses for what it would leave has one line per finished
task the reset would leave where its line puts it, when those edits are the way
on. `<file>` is the file holding the task, printed
as [§FS-rhei-errors.2](rhei-errors.spec.md#2-copy-paste-safety) prints a path, and `<initial>` its profile's `initial` state. The
part in brackets is there when the task has a result link:

```text
a reset leaves Task <id> where its **State:** line in <file> puts it, because nothing records a state it could restore: edit that line to '<initial>'[ and delete the `> **Result:**` line under it], then run the reset again
```

Otherwise the help names no edit. It has one line per task an error names that
the reset moves, saying where the ledger sends it:

```text
a reset moves Task <id> to '<dest>', the state the ledger records it started in, whatever its **State:** line says
```

Where the help names no edit and no task an error names is one the reset moves,
it is this one line instead, so a refusal's help is never empty:

```text
no edit to a **State:** line is known to let this reset through: fix what the errors above name, then run the reset again
```

`--dry-run` and the pre-confirmation preview ([§FS-rhei-reset.1.2](rhei-reset.spec.md#12-confirmation)) print the same move list
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
Kept run evidence in runtime/evidence/; recorded the removed paths as reset event <uuid>.
```

## 5. Relationship to Other Commands

`rhei reset` inverts the forward commands (`next`, `transition`, `complete`, `run`): it returns every task to the state it was authored in ([§FS-rhei-reset.2.2](rhei-reset.spec.md#22-authored-state)) and removes the `runtime/` directory.

See [How Rhei Is Used — Command Surface](rhei-usage.spec.md#22-command-surface) for the full table comparing all five coordination commands.

### 5.1. Offering Reset as a Remedy

A message that offers `rhei reset` as the way out of something — a refusal, a
halt row, a note — names a command that runs as printed and says how far it
reaches. All of them print the same text, built in one place:

```text
`rhei reset <target> --rhei <rhei>`, which returns every ticket in that rhei to its authored state and deletes their results and logs
```

**It runs as printed**, from wherever the message is read, which may be a run
report, a log, or another directory:

- `<target>` is the target of the command that printed the message, because
  reset never infers one ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)). It is quoted as one shell word
  ([§FS-rhei-errors.2](rhei-errors.spec.md#2-copy-paste-safety)) but is always absolute, where that point would print it relative
  to the working directory: the working directory that message was printed in is
  not the one it is read from. It is not resolved through a symlink: a symlinked
  workspace's name is its rhei id, so a resolved path names a different rhei and
  `--rhei` is refused against it.
- `<rhei>` is the rhei that owns the ticket: the first segment of its qualified
  id, whichever way the ticket was named on the command line.
- A command that was given `--state-machine` passes it on, absolute, as
  `--state-machine <path>`. Without it the reset still runs, under whichever
  machine the target resolves to on its own. If that machine has no such states,
  the reset removes the results and logs and leaves the ticket where it stands
  ([§FS-rhei-reset.4](rhei-reset.spec.md#4-output)).
- It carries no `--yes`. The preview and the confirmation of
  [§FS-rhei-reset.1.2](rhei-reset.spec.md#12-confirmation) are part of running it, so a caller without a terminal gets
  reset's own refusal, which names both ways on.
- A diagnostic carrying this text, or the forced transition below, is not
  wrapped. Its target alone may be wider than the terminal, so a wrap would split
  the command over lines, and three lines do not run as printed. The renderer
  leaves the line whole and the terminal soft-wraps it, which copies back as one
  line. This is the exception [§FS-rhei-errors.2](rhei-errors.spec.md#2-copy-paste-safety) allows.

**It says how far it reaches.** `--rhei` is the narrowest scope reset has
([§FS-rhei-reset.2.1](rhei-reset.spec.md#21-narrowed-reset---rhei)), and a message about one ticket reads as an offer to clear
one ticket. So the text says "every ticket in that rhei", and a message never
describes the command as clearing the ticket it is about. Where there is a
cheaper way out the message can print, it comes first: a refused log name offers
moving the file away before the reset ([§FS-rhei-agents.8.1](rhei-agents.spec.md#81-log-file-naming)).

`rhei complete` on a finished ticket ([§FS-rhei-complete.4](rhei-complete.spec.md#4-behavior)) prints a cheaper way
too: the forced transition of [§FS-rhei-transition-cmd.6](rhei-transition-cmd.spec.md#6-operator-forced-missing-edge-recovery). Where no ancestor of the
ticket is finished, it reopens that ticket alone and keeps every other ticket's
state, results and logs. Under a finished ancestor the message starts higher up,
as the end of this point says. That command belongs to an operator at a terminal,
and agents are forbidden it, while an agent is the likeliest reader of this
refusal. So the text says both who may run it and that an agent may not.

What else it offers depends on `<dest>`, the state a reset would leave this ticket
in. `<dest>` follows [§FS-rhei-reset.2.2](rhei-reset.spec.md#22-authored-state), by the one computation reset itself uses: the
first `from` the ledger records for this ticket when the machine declares that
state, and otherwise the state the ticket stands in.

The forced transition goes only to a state the forced path would let the hop
enter: one the machine declares as not terminal, and whose `visits:` budget this
ticket has not spent. The forced path refuses a hop into a spent state before
its terminal check ([§FS-rhei-transition-cmd.6](rhei-transition-cmd.spec.md#6-operator-forced-missing-edge-recovery)), so a line into one never runs, whoever
runs it. Spent is what that refusal measures, the ticket's counted visits to the
state against its limit ([§FS-rhei-transitions.4.3](rhei-transitions.spec.md#43-counted-loops)), decided by the one check both use,
so the offer and the refusal cannot disagree. A state with no `visits:` is never
spent. Such a state **qualifies**. The line goes to `<dest>` when it qualifies,
and otherwise to the `initial` state of the ticket's own profile when that
qualifies.

When `<dest>` qualifies and the reset this message offers is not one
[§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse) refuses, the reset does reopen the ticket, and the two offers agree on
where it goes:

```text
nothing to do — the task is finished. An operator at a terminal can reopen this ticket alone with `rhei transition <target> --task <id> --from <from> --to <dest> --force --reason 'reopen this finished ticket'`, giving their own reason; an agent may not run that. Otherwise, run `rhei reset <target> --rhei <rhei>`, which returns every ticket in that rhei to its authored state and deletes their results and logs
```

When `<dest>` is not terminal but its budget is spent, the reset still reopens
the ticket, because it clears the ticket's counted visits along with its state
([§FS-rhei-reset.2](rhei-reset.spec.md#2-behavior)), so it stays offered where [§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse) does not refuse it. Where
`initial` qualifies, the line goes there, and the message says why it does not go back to `<dest>`, in the forced
path's own words ([§FS-rhei-errors.1.5](rhei-errors.spec.md#15-the-subject-a-check-measured)):

```text
nothing to do — the task is finished. An operator at a terminal can reopen this ticket alone with `rhei transition <target> --task <id> --from <from> --to <initial> --force --reason 'reopen this finished ticket'`, giving their own reason; an agent may not run that. That command goes to '<initial>' rather than '<dest>': visit budget for state '<dest>' is exhausted (<used>/<limit> visits). Otherwise, run `rhei reset <target> --rhei <rhei>`, which returns every ticket in that rhei to its authored state and deletes their results and logs
```

When `<dest>` is terminal, a reset is no way back: it would delete the rhei's
results and logs and leave this ticket finished. So the message prints no reset
command and says that a reset would not reopen the ticket. Where the ticket's
profile has an `initial` state that qualifies, the forced transition goes there:

```text
nothing to do — the task is finished. An operator at a terminal can reopen this ticket alone with `rhei transition <target> --task <id> --from <from> --to <initial> --force --reason 'reopen this finished ticket'`, giving their own reason; an agent may not run that. A reset would not reopen it: it would leave this ticket in '<dest>'
```

Where the profile has no `initial` the machine declares as not terminal, the
message prints no command:

```text
nothing to do — the task is finished. A reset would not reopen it: it would leave this ticket in '<dest>'
```

When `<dest>` is not terminal but [§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse) refuses the reset this message would
offer, a reset is no way back either: it would change nothing. So the message
prints no reset command and says the reset would be refused, and why, in the
validator's words. The question is about the reset that would be offered, which
is the whole rhei under `--rhei`, and not about this ticket's own tree: a ticket
whose tree is fine is still not offered a reset that another tree in the same
rhei gets refused. It is answered by the one computation the reset refuses with,
the way `<dest>` is. The forced transition still goes where the rule above sends
it, `<dest>` when that qualifies:

```text
nothing to do — the task is finished. An operator at a terminal can reopen this ticket alone with `rhei transition <target> --task <id> --from <from> --to <dest> --force --reason 'reopen this finished ticket'`, giving their own reason; an agent may not run that. A reset of its rhei would be refused: the plan it would leave fails validation, because <error>
```

Each error the reset would introduce is named, joined by `; `.

Where no state qualifies and a spent budget is why, the message prints no
command either, and says which budget is spent. It names every state the rule
passed over for its budget, `<dest>` first and each once, with the forced path's
own refusal for it. Where `<dest>` is not terminal and the reset is not refused,
the reset is offered, since it clears the count; with no line before it, its
lead-in says it is the way back rather than `Otherwise`:

```text
nothing to do — the task is finished. It cannot be forced back into '<spent>': visit budget for state '<spent>' is exhausted (<used>/<limit> visits). A reset reopens it with its visits cleared: run `rhei reset <target> --rhei <rhei>`, which returns every ticket in that rhei to its authored state and deletes their results and logs
```

Where `<dest>` is terminal, or the reset is refused, the message ends as the
others do for that case, with `A reset would not reopen it` or with `A reset of
its rhei would be refused` and the errors:

```text
nothing to do — the task is finished. It cannot be forced back into '<spent>': visit budget for state '<spent>' is exhausted (<used>/<limit> visits). A reset would not reopen it: it would leave this ticket in '<dest>'
```

When `<dest>` and `initial` are two states and both are spent, the sentence
names both and joins the two refusals with a semicolon:

```text
It cannot be forced back into '<dest>' or '<initial>': visit budget for state '<dest>' is exhausted (<used>/<limit> visits); visit budget for state '<initial>' is exhausted (<used>/<limit> visits).
```

The message goes no further than `<dest>` and `initial`, here or for an ancestor
below, because a profile may allow another non-terminal state, and which one is
the operator's to choose.

The forced transition is built beside the reset, with the same `<target>` and the
same `--state-machine`. `<id>` is the qualified id and `<from>` the ticket's base
state. The printed reason is there so the command runs as printed:
[§FS-rhei-transition-cmd.6](rhei-transition-cmd.spec.md#6-operator-forced-missing-edge-recovery) requires a reason on this invocation, and the audit row is only
worth what the operator writes, so the message tells them to replace it.

Under a finished ancestor this ticket's forced line cannot run. The forced path
refuses it until every terminal ancestor has been reopened
([§FS-rhei-transition-cmd.6](rhei-transition-cmd.spec.md#6-operator-forced-missing-edge-recovery)). Printing the line anyway is the defect
[§FS-rhei-errors.6](rhei-errors.spec.md#6-coverage) describes, so the message says where the way back starts instead.
`<ancestor>` is the qualified id of the topmost finished ancestor, the only one
whose line gets past that refusal, found the way the refusal finds a terminal
ancestor. The line is that ancestor's own: `<ancestor-from>` is its base state, and
`<ancestor-to>` is decided for it by the rule above, measured against the
ancestor's own counted visits: `<ancestor-dest>`, its own reset destination, when
that qualifies, otherwise its own profile's `initial` when that qualifies:

```text
nothing to do — the task is finished. Its ancestor <ancestor> is finished too and has to be reopened first, then each finished ticket below it down to this one: an operator at a terminal starts with `rhei transition <target> --task <ancestor> --from <ancestor-from> --to <ancestor-to> --force --reason 'reopen this finished ticket'`, giving their own reason; an agent may not run that. <tail>
```

Where that line passes over `<ancestor-dest>` for a spent budget, the message
says so after the line, in the words this ticket's own message uses:

```text
nothing to do — the task is finished. Its ancestor <ancestor> is finished too and has to be reopened first, then each finished ticket below it down to this one: an operator at a terminal starts with `rhei transition <target> --task <ancestor> --from <ancestor-from> --to <ancestor-to> --force --reason 'reopen this finished ticket'`, giving their own reason; an agent may not run that. That command goes to '<ancestor-to>' rather than '<ancestor-dest>': visit budget for state '<ancestor-dest>' is exhausted (<used>/<limit> visits). <tail>
```

An ancestor for which neither `<ancestor-dest>` nor its profile's `initial` is a
state the machine declares as not terminal gets no command, and the message says
why:

```text
nothing to do — the task is finished. Its ancestor <ancestor> is finished too and has to be reopened first, and neither a reset nor its profile's initial state would do that. <tail>
```

An ancestor no state qualifies for because of a spent budget gets no command
either, and the message names each spent state as this ticket's own does:

```text
nothing to do — the task is finished. Its ancestor <ancestor> is finished too and has to be reopened first, and it cannot be forced back into '<spent>': visit budget for state '<spent>' is exhausted (<used>/<limit> visits). <tail>
```

`<tail>` is how this ticket's own message above ends, unchanged, because it is still
about this ticket. It takes one of three forms: `Otherwise, run` and the reset text
when `<dest>` is not terminal and the reset is not refused; `A reset of its rhei would
be refused: the plan it would leave fails validation, because <error>` when
`<dest>` is not terminal and the reset is refused; and `A reset would not reopen
it: it would leave this ticket in '<dest>'` when `<dest>` is terminal. An ancestor
that neither a reset nor its profile's initial state would reopen is one a reset
leaves finished, so where this ticket's `<dest>` is not terminal, that reset is
refused and the tail says so rather than offering it. An ancestor named for a spent
budget may be one the reset reopens, since the reset clears the count, and its tail
is whichever form the reset's own computation gives.
A ticket with no state of its own to reopen into keeps its no-command message
whatever its ancestors are, the one naming a spent budget included: it offers no
line, so nothing in it is false. Each hop moves the start down: the next
`rhei complete` names the next finished ancestor, and once none is left, this
ticket's own line.

The message predicts two of the forced path's refusals, a finished ancestor and a
spent visit budget. A hop the forced path would refuse because of a claim gets that
path's own refusal; the message does not predict it. A claim differs from both: its
refusal names the holder and the remedy, nothing else has to be reopened, and the
printed line runs once the claim is released. A spent budget differs where it
matters: the printed line itself can never run. Its refusal does name a way on,
another declared edge or a higher limit from the workflow author, but neither runs
that line: `--force` cannot bypass the budget, and short of editing the machine
only a reset, which is the other offer, clears the count.

The other messages that offer the reset — a refused log name, a halt row, a note —
do not predict [§FS-rhei-reset.2.3](rhei-reset.spec.md#23-a-reset-validation-would-refuse) either. They offer the reset as the way out of
something else and claim nothing about reopening a ticket, and they are built where
no plan or machine has been loaded. On a plan where the reset would be refused,
their reader gets that refusal with nothing deleted, and that refusal is what keeps
every one of them harmless.

No message asks about step 1 of [§FS-rhei-reset.2](rhei-reset.spec.md#2-behavior), the refusal of a plan that is already
invalid. `rhei complete` refuses such a plan before it looks at the ticket
([§FS-rhei-complete.4](rhei-complete.spec.md#4-behavior) step 2), by the same check under the same machines, so a finished
ticket's message is only ever printed over a plan step 1 lets through. The other
messages leave their reader step 1's refusal, which deletes nothing, as they leave
the refusal above.

This covers messages that name a command for the reader to run. A refusal that
asks the reader to name a target, because none was given, keeps its placeholder:
the reset it answers has no target to print ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)).

## 6. Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — plan formats and semantic constraints
- [States Specification](rhei-states.spec.md) — profile resolution and `initial` state rules
- [Supervision Specification](rhei-supervision.spec.md) — the pre-authored chain [§FS-rhei-reset.2.2](rhei-reset.spec.md#22-authored-state) exists to preserve
- [Visualization Specification](rhei-viz.spec.md) — the transition ledger [§FS-rhei-reset.2.2](rhei-reset.spec.md#22-authored-state) recovers authored states from
- [Next Command](rhei-next.spec.md), [Complete Command](rhei-complete.spec.md), [Transition Command](rhei-transition-cmd.spec.md) — forward commands that reset inverts
- [Release Command](rhei-release.spec.md) — drops one claim without destroying runtime output
