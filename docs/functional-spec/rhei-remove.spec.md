# FS-rhei-remove: `rhei remove`

Take back a ticket nothing has acted on yet. A ticket filed by mistake — a
`rhei new` into the wrong rhei, a typo in the title, a report that should never
have been made — is today removed by hand: the reporter deletes its section
from the plan, leaves its `runtime/` residue behind, and frees its number, so
the next `rhei new` hands the same id to an unrelated ticket. Nothing checks
that another ticket's `**Prior:**` or `**Consumes:**` still names it until the
next load fails.

`rhei remove` is that edit as one command: it deletes the ticket's definition
and its disposable residue under the locks every other writer takes, and it
**retires** the id, so no later create reissues it ([§FS-rhei-new.4](rhei-new.spec.md#4-ids)). It is
`rhei new` in reverse, for exactly the case where reversing is honest — a
ticket with no history to lose.

The boundary is **untouched** ([§FS-rhei-remove.2](rhei-remove.spec.md#2-untouched)), and it is deliberate. A ticket that has
been acted on has history, and history is kept: every terminal task's id,
title, final state and result stays reachable ([§FS-rhei-memory.1.1](rhei-memory.spec.md#11-everything-before-is-reachable)), and the
run record is append-only except under `rhei reset` ([§FS-rhei-run-report](rhei-run-report.spec.md#fs-rhei-run-report-per-run-report)). The
answer for such a ticket is its machine's cancellation-role state
([§FS-rhei-states.1.4](rhei-states.spec.md#14-reserved-state-names)), reached with `rhei transition`
([§FS-rhei-transition-cmd.6](rhei-transition-cmd.spec.md#6-operator-forced-missing-edge-recovery)), never deletion.

```console
$ rhei new "Check a finding" --under grund --kind bug
Created ticket grund.63 ...
$ rhei remove grund.63
removed grund.63; id retired
$ rhei new "Check another finding" --under grund --kind bug
Created ticket grund.64 ...
```

## 1. Usage

```bash
rhei remove <TICKET_OR_PLAN> [--task <ID>] [--rhei <ID>] [--dry-run]
```

### 1.1. Options

| Flag | Default | Description |
|---|---|---|
| `<TICKET_OR_PLAN>` | — | A ticket id, or a path to a plan, workspace or project whose ticket `--task` names. See [§FS-rhei-remove.1.2](rhei-remove.spec.md#12-resolution) |
| `--task <ID>` | — | The ticket to remove when the positional names a path |
| `--rhei <ID>` | — | The rhei a local ticket id is resolved in |
| `--dry-run` | false | Check eligibility and print what would be removed and retired, changing nothing. See [§FS-rhei-remove.7](rhei-remove.spec.md#7-dry-run) |

The explicit ticket is the consent. There is no bulk mode, no `--force`, and no
confirmation prompt: a command that removes one named ticket with no history has
nothing to confirm that the name did not already say, and a force flag would be
a way past the one rule ([§FS-rhei-remove.2](rhei-remove.spec.md#2-untouched)) that makes removal safe.

### 1.2. Resolution

Resolution is the positional ticket-or-path resolver the other ticket verbs
share ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)). An argument that names an existing path selects that
plan, workspace or project, and `--task` names the ticket in it; otherwise the
argument is a ticket id resolved in the project discovered from the working
directory.

- A **project-qualified** id (`grund.63`) names its ticket directly.
- A **local** id must match exactly one ticket among the selected rheis. An id
  that matches in two rheis is refused, naming both qualified ids and `--rhei`.
- `--rhei` narrows which rhei a local id is resolved in. It does not narrow
  what is loaded: the whole project is loaded and validated, because a
  dependent may live in any rhei ([§FS-rhei-remove.3.1](rhei-remove.spec.md#31-dependents)).

Exactly one **leaf ticket** is removed. A child ticket with no children of its
own is a leaf and may be removed. A ticket with children, a rhei node, and the
project root are refused ([§FS-rhei-remove.3.2](rhei-remove.spec.md#32-children-and-node-kinds)): removing them would remove what is under them.

## 2. Untouched

A ticket is **untouched** when nothing has recorded acting on it. Removal
requires it, and reads every one of these sources at both the ticket's owning
execution root and the project execution root:

- no row for the ticket in `runtime/state-transitions.log`, ordinary or forced;
- no spawn record and no agent or program log under `runtime/spawns/` or
  `runtime/logs/`, other than a provably pre-spawn log header ([§FS-rhei-remove.4.2](rhei-remove.spec.md#42-residue));
- no result: no `runtime/results/<id>.md` and no `> **Result:**` link in its
  body;
- no export file under `runtime/exports/<id>/` and no file the machine declares
  as a `{task_id}` input or output;
- no `**Assignee:**` line;
- no counted visit (`stateVisits`), no `supervision` block, no provider-limit
  or poll wait in its metadata;
- no snapshot session, worktree reference, accounting capture or accounting
  task record;
- no budget reservation, start or travel receipt for it in the project account
  ([§FS-rhei-budgets.5.2](rhei-budgets.spec.md#52-the-journal));
- no task note in the note store and no invocation event naming it.

A ticket in a **terminal** state is refused even when none of those files
exists. A terminal state is a recorded outcome whatever produced it, and terminal
memory keeps it ([§FS-rhei-memory.1.1](rhei-memory.spec.md#11-everything-before-is-reachable)).

An **identity binding alone is allowed**. A `budgetTicketId` in the ticket's
metadata, or a binding the account holds for it with no receipt, records an
identity rather than an act; removal keeps it ([§FS-rhei-remove.5.1](rhei-remove.spec.md#51-where-it-is-recorded)) and never releases it.

Evidence that cannot be read — an unreadable ledger, a log that cannot be
opened, a source whose ownership cannot be decided — **refuses** removal, naming
the path. Unknown is not untouched.

Releasing a claim or resetting a rhei does not turn a ticket back into an
untouched one while history about it survives anywhere above. Removal reads
what is there, not how it got there.

## 3. Refusals

Every refusal changes nothing — no definition, no metadata, no file under
`runtime/`, no retirement — and names each blocker with the task, path or run
it was found in. All blockers found are reported, not only the first.

### 3.1. Dependents

A ticket that another ticket names is refused. Every rhei of the project is
searched, in every field that names a ticket: `**Prior:**`, `**Consumes:**`
(the producer of a consumed export), and any other ticket reference the plan
language resolves. The diagnostic names the dependent and the field:

```text
error: grund.62 cannot be removed: grund.70 names it in **Prior:**
```

### 3.2. Children and node kinds

A ticket with children is refused, naming the children; remove them first. A
rhei id and the project root are refused as not tickets, pointing at the plan
file for a rhei.

### 3.3. A live run, held locks and guards

Removal requires the project to be idle. A held run lock on any execution root
of the project, a lock whose ownership cannot be verified, or an access guard
that cannot be taken ([§FS-rhei-recover.4](rhei-recover.spec.md#4-pending-root-interlock)) refuses removal, naming the run or the
lock path. Removal never proceeds unguarded.

### 3.4. A ticket with history

A ticket that fails [§FS-rhei-remove.2](rhei-remove.spec.md#2-untouched) is refused, naming each piece of evidence found and
where, and pointing at the alternative that keeps history:

```text
error: grund.62 cannot be removed: it has history
       transition:  runtime/state-transitions.log records grund.62 pending@triage
       result:      runtime/results/grund.62.md
       instead:     move it to its machine's cancellation state with
                    `rhei transition grund.62 <state>`
```

## 4. What is removed

### 4.1. The definition

Removal deletes exactly the selected ticket's Markdown section — its heading
through the line before the next sibling or ancestor heading — from the file
that defines it, and its `metadata.tasks.<id>` entry from the document that
holds it: the plan's writable metadata document, and for a Directory Workspace
task file the authored block of that file ([§FS-rhei-plan-language.1.4](rhei-plan-language.spec.md#14-directory-workspace-metadata)). Surrounding
prose, every sibling's definition and metadata, and every other byte of the file
stay as they were. A workspace task file left with no task in it is kept, not
deleted.

Deleting that one entry is the single exception to the rule that a workspace
task file's metadata block is authored and never written by a Rhei command
([§FS-rhei-plan-language.1.4](rhei-plan-language.spec.md#14-directory-workspace-metadata), rule 4): the entry describes a task that no longer
exists, and leaving it would fail that file's own-tasks-only rule on the next
load.

The project is validated after the edit, under the same locks, and a removal
whose result does not validate is rolled back.

### 4.2. Residue

Cleanup removes only what is **disposable and owned by the selected ticket**:

- an empty directory or empty placeholder file at a path the owning machine
  declares for that ticket under `runtime/` — such as `runtime/exports/<id>/` —
  at the owning execution root and, when it differs, at the project execution
  root;
- an agent or program log under `runtime/logs/task-<id>-*` whose content is
  provably only the pre-spawn header rhei writes before starting the process.

Ownership is exact: a path belongs to the ticket when the machine's `{task_id}`
pattern resolves to it for that id, never by a loose prefix, so removing
`auth.1` never touches `auth.10` or `auth.1x`. A target that is ambiguous,
shared with another ticket, escapes the execution root, or is a symlink is
refused, not cleaned. A path the machine does not declare is left alone.

A nonempty work product is evidence ([§FS-rhei-remove.2](rhei-remove.spec.md#2-untouched)), not residue, so removal never reaches
it. Removal deletes no history: shared ledgers, the note store, run reports,
accounting rollups, budget data, other tickets' files, and every permanent lock
sidecar and the directories that hold them stay byte-identical.

## 5. Retirement

### 5.1. Where it is recorded

A removed id is recorded under `metadata.retiredTickets`, a map keyed by the
ticket's project-qualified id ([§AR-rhei-panta.2](../architecture/rhei-panta.spec.md#2-load-model)). Each value is a map, empty
unless the ticket had a `budgetTicketId`, which moves into it unchanged:

```yaml
metadata:
  retiredTickets:
    grund.63: {}
    grund.61:
      budgetTicketId: 7f3c1a90-5e21-4d8b-9a6c-bc5de10f12e7
```

The map lives in one document per project: `index.panta.md` for a Panta
project ([§FS-rhei-plan-language.1.5](rhei-plan-language.spec.md#15-panta-project)), basin tickets included, and the plan's
own metadata document for a lone single-file plan or bare Directory Workspace —
its frontmatter, or `index.rhei.md` ([§FS-rhei-plan-language.1.4](rhei-plan-language.spec.md#14-directory-workspace-metadata)). It is
project bookkeeping rhei writes, not author task metadata
([§FS-rhei-transitions.2.5](rhei-transitions.spec.md#25-keys-rhei-writes)), and it survives `rhei reset` ([§FS-rhei-reset.2](rhei-reset.spec.md#2-behavior)) and
the replacement of a rhei within the project.

The key is reserved. A pre-existing `metadata.retiredTickets` that is not a
retirement map is a validation error naming it; removal never overwrites it.

### 5.2. What a retired id forbids

A retired id is never live again in its project:

- generated numbering counts retired siblings as taken ([§FS-rhei-new.4](rhei-new.spec.md#4-ids)), so the
  next `rhei new` under the same parent skips past it;
- an explicit `rhei new --id`, a template placement ([§FS-rhei-library.4](rhei-library.spec.md#4-placement-ids-tickets-and-frontmatter)), and a
  hand-written section that would declare it are refused, naming the
  retirement record ([§FS-rhei-validate.4](rhei-validate.spec.md#4-behavior));
- a reference to it in `**Prior:**` or `**Consumes:**` is diagnosed as a
  retired ticket, not an unknown one ([§FS-rhei-validate.4.1](rhei-validate.spec.md#41-unresolved-prior-references));
- its budget identity is never rebound to another live ticket
  ([§FS-rhei-budgets.5.2.1](rhei-budgets.spec.md#521-one-identity-one-live-ticket)).

Removal refunds nothing: the budget journal, receipts and bindings are
untouched ([§FS-rhei-budgets.11](rhei-budgets.spec.md#11-what-does-not-change)), as [§REQ-bounded-neural-work.4](../requirements/bounded-neural-work.spec.md#4-nothing-creates-capacity) requires.

## 6. Locks and interruption

### 6.1. Lock order

Removal try-acquires, without waiting, in this order:

1. the `.rhei/run.lock` of every execution root of the project
   ([§FS-rhei-run.2](rhei-run.spec.md#2-options));
2. the exclusive access guard of each owner root ([§FS-rhei-recover.4](rhei-recover.spec.md#4-pending-root-interlock)), in
   sorted canonical-root order;
3. the permanent metadata, task and ledger sidecars, in the order
   `rhei reset` and `rhei new` take them ([§FS-rhei-reset.2](rhei-reset.spec.md#2-behavior), [§FS-rhei-new.4](rhei-new.spec.md#4-ids)).

Any one it cannot take refuses removal ([§FS-rhei-remove.3.3](rhei-remove.spec.md#33-a-live-run-held-locks-and-guards)). Holding them, it re-reads and
validates the whole project and refuses if the set of roots it locked no longer
covers it. That excludes creates, resets, transitions, claims and run startup
from the first read to the last cleanup. It does not isolate raw editor reads.

### 6.2. Pending removal

Before its first effect, removal writes and syncs `.rhei/pending-removal.json`
at the project execution root, recording the ticket, the before and after image
of every file it will rewrite, and the exact disposable paths it will clean.
The effects then run in this order: persist the retirement; remove the
definition and its metadata; clean the residue; clear the marker once that
completion is synced.

An interrupted removal resumes through the same invocation, `rhei remove
<ticket>`. Resumption accepts only the recorded before or after image of each
file and the recorded residue; any other change stops it, naming the file. A
removal that already completed reports the id as already retired and exits 0.

While the marker exists, every other entry point that reads or writes the
project — `rhei new` and `rhei reset` included — refuses and prints the
`rhei remove` invocation that resumes it, through the same interlock that
pending forced recovery uses ([§FS-rhei-recover.4](rhei-recover.spec.md#4-pending-root-interlock)). Removal writes no
transition-ledger row and fires no callback.

## 7. Dry run

`--dry-run` runs every check in [§FS-rhei-remove.2](rhei-remove.spec.md#2-untouched) and [§FS-rhei-remove.3](rhei-remove.spec.md#3-refusals) and prints the qualified ticket, the
retirement it would record and every path it would delete, then exits without
taking a removal record or changing a byte. A dry run that would be refused is
refused the same way.

## 8. Output

On success removal prints one line to stdout:

```text
removed <qualified-id>; id retired
```

followed, when residue was cleaned, by one line per deleted path. Diagnostics
go to stderr in the shape `error: <qualified-id> cannot be removed: <reason>`
([§FS-rhei-errors.6](rhei-errors.spec.md#6-coverage)). The exit status is 0 on success and on a repeat of a
completed removal, and nonzero on every refusal.

## 9. What `rhei remove` does not do

- It does not remove a ticket with history, or offer a way to. That is
  cancellation through the machine.
- It does not remove a rhei or a project, or more than one ticket.
- It does not reissue, unretire or rename an id; retirement is permanent.
- It does not touch the budget account, the transition ledger, the note store
  or any run record.
- It does not change `rhei reset`'s scope or confirmation, any state machine,
  or any callback.
