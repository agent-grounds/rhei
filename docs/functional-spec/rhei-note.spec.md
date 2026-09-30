# FS-rhei-note: `rhei note`

Leave one fact for whoever works next, anywhere in this project. `rhei note` is
the only writer of the project note store ([§FS-rhei-memory.2](rhei-memory.spec.md#2-the-store)), the one file
whose entries compose into `### Project Notes` in every ordinary prompt
([§FS-rhei-memory.3.1](rhei-memory.spec.md#31--position)). A task that finds out something the next ticket would
otherwise pay to rediscover spends its one slot at the moment it finds out, and
every prompt composed afterwards carries it. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

Every other channel rhei has runs in a fixed direction. A brief goes down from
a supervisor ([§FS-rhei-supervision.5.2](rhei-supervision.spec.md#52-the-brief)), a handoff goes between the states of
one task ([§FS-rhei-states.3.2](rhei-states.spec.md#32-state-handoffs)), an export goes along a declared prior
([§FS-rhei-plan-language.3.12](rhei-plan-language.spec.md#312-task-exports)), and a result reaches a task that did not declare
the edge as one line of Plan History ([§FS-rhei-memory.4.3](rhei-memory.spec.md#43-plan-history)). None of them
carries a fact **sideways into the future** — to a ticket that did not, and
could not, know to name its source. This verb is that channel, and the cost of
having it is one bounded section on every prompt.

What it is not: a second result file, a log, or a place to restate the plan.
The store is run state under `runtime/`, it dies with `rhei reset`
([§FS-rhei-reset.2](rhei-reset.spec.md#2-behavior)), and it does not replace a standing convention a person
writes into a project's own context — it spares one run from rediscovering
inside itself what one of its own tickets already paid for.

## 1. Usage

```bash
rhei note "<text>"              # record: leave a fact
rhei note --restate <TASK_ID>   # endorse an entry: make it the newest
rhei note --strike  <TASK_ID>   # take an entry out of composition
```

| Flag                 | Required | Description                                                          |
|----------------------|----------|----------------------------------------------------------------------|
| `--restate <TASK_ID>`| No       | Move the named task's live entry to this record's position.           |
| `--strike <TASK_ID>` | No       | Remove the named task's live entry from composition.                  |
| `--task <ID>`        | No       | Name the writing task. Defaults to `RHEI_TASK_ID`.                    |

Exactly one of the text positional, `--restate`, and `--strike` is given. Two
of them together, or none of them, is a usage error ([§FS-rhei-note.4](rhei-note.spec.md#4-refusals)).

The writing task is the value of `RHEI_TASK_ID`, which `rhei run` exports to
every agent it spawns ([§FS-rhei-agents.4](rhei-agents.spec.md#4-environment-variables)). A manual worker driving `rhei next`
has no such environment, and names the task with `--task <ID>`; `--task` wins
over the environment where both are set. Task-id arguments complete from the
plan like every other ([§FS-rhei-completions](rhei-completions.spec.md#fs-rhei-completions-rhei-completion-ux-specification)).

`rhei note` resolves the project from the working directory, the way
`rhei complete` resolves a bare ticket id ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)). It takes no plan
positional: the store is not plan markdown, and the verb never reads, locks, or
rewrites a plan.

## 2. The Slot

A task holds exactly **one slot**, keyed to the task and not to the invocation,
so a retry, a second visit, or a re-entered state cannot multiply one ticket's
cost to every later reader. The slot has three uses and they are exclusive:

1. **Record** a fact of its own.
2. **Restate** another task's live entry, which makes that entry the newest
   thing in the composed list.
3. **Strike** another task's live entry, which takes it out of composition.

Spending the slot on `--restate` or `--strike` leaves the task none of its own,
and a later call replaces whatever the task spent it on before. Because
endorsing or removing someone else's entry costs a task its own entry, no task
can endorse everything.

Removal authority is **project-wide**: a task in one rhei may strike an entry
written by a task in another. The store charges every plan in the project, so
whoever pays may stop the charge. A striking task judges the entry on its words
alone, plus the standing rules already in its own prompt; it is not given the
origin's history and must not act as though it were.

## 3. The Store

### 3.1. Where It Lives

One file, `runtime/notes.md`, at the **project execution root** — beside
`runtime/state-transitions.log` and the other cross-rhei rollups
([§AR-rhei-panta.5](../architecture/rhei-panta.spec.md#5-execution-root-and-per-rhei-runtime)). It is project-level rather than one file per rhei, because
a fact one ticket pays for is as often about the machine, the repository, or
the toolchain as it is about the rhei that found it.

For a bare rhei with no `index.panta.md`, the project is the rhei itself and
the file sits at its execution root, which is the same implicit Panta every
other memory section already resolves against ([§FS-rhei-panta](rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis)).

The file is created on the first successful write. A project that never calls
the verb has no store and composes no block ([§FS-rhei-memory.4.2](rhei-memory.spec.md#42-position)).

### 3.2. The Record Grammar

The file is **append-only**, and one markdown list item is one record. Three
forms, and no fourth:

```markdown
- [auth.2] Three concurrent cargo builds under /tmp fill the root disk; build under ~/ag/tmp with --target-dir.
- [billing.1 restates auth.2]
- [reporting.1 strikes auth.1]
```

The bracket is a **closed grammar** — `[<task-id>]`, `[<task-id> restates
<task-id>]`, `[<task-id> strikes <task-id>]` — read only at the start of a list
item, so a note whose own text begins with the word "strikes" is never mistaken
for a verb. Ids are project-qualified, as every id in a prompt already is
([§FS-rhei-memory.4.5](rhei-memory.spec.md#45-fencing-and-rendering)).

An entry is identified by the task that wrote it: a task has at most one live
entry, so there is no second identifier to carry. There is no timestamp — a
clock is not among composition's inputs ([§FS-rhei-memory.1.2](rhei-memory.spec.md#12-composition-is-algorithmic)), and file
position is the order ([§FS-rhei-memory.4.2](rhei-memory.spec.md#42-position)).

A record's text may run to three lines; continuation lines are indented under
the list item, as markdown already requires.

### 3.3. The Lock

A record is appended under the same discipline the transition ledger already
uses for a project-level file several processes append to
([§FS-rhei-complete.3.1](rhei-complete.spec.md#31-state-transition-ledger)): an exclusive `fs2` hold on a stable sidecar that lives
*outside* the replaceable runtime tree, the data file opened only after the
lock is taken, and a partial append truncated back to the length recorded at
open. The sidecar for `runtime/notes.md` is `runtime.notes.md.lock` at the same
root, the spelling the ledger's sidecar already follows.

`fs2` rather than an append-mode handle, so the behaviour is the same on
Windows ([§REQ-cross-platform](../requirements/cross-platform.md#req-cross-platform-one-tool-on-linux-macos-and-windows)). **Readers take no lock**: a record is appended
whole under the lock or rolled back, and composition skips a record it cannot
parse rather than failing ([§FS-rhei-memory.4.2](rhei-memory.spec.md#42-position)).

### 3.4. Nothing Rewrites It

A strike marks in place. Nothing in the store is ever deleted, edited, or
compacted, which is what makes the concurrent append safe and what keeps a
struck entry's bytes reachable by path ([§FS-rhei-memory.1.1](rhei-memory.spec.md#11-everything-before-is-reachable)).

An agent may not edit `runtime/notes.md` by hand ([§FS-rhei-memory.3.4](rhei-memory.spec.md#34--rhei-commands-additions)). The
verb is what enforces the slot, the line bound, the duplicate rule, and the
lock; an entry written around it holds none of them.

## 4. Refusals

Both refusals happen at write time, before anything is appended, and both exit
non-zero with nothing written.

**An over-long entry.** An entry is bounded at **3 lines**. Over that the
command refuses and names the bound and the count. It refuses rather than
truncating: an agent can retry in the same breath, and a silently halved fact
is worse than no fact. The bound is on lines rather than on entries alone
because the cost the cap exists to hold flat is lines read by every later
spawn ([§FS-rhei-memory.4.5](rhei-memory.spec.md#45-fencing-and-rendering)).

**A duplicate.** An entry whose text already appears — after whitespace
normalization — among the live entries, or in the writing task's own
`### Rhei Context` or `### Project Context`, is refused. The match is **exact**
after normalization and nothing fuzzier: selection by similarity would make the
refusal unpredictable and has no place this close to composition
([§FS-rhei-memory.1.2](rhei-memory.spec.md#12-composition-is-algorithmic)). `--restate` is the single exception, and it is one
precisely because restating costs the restater its own slot ([§FS-rhei-note.2](rhei-note.spec.md#2-the-slot)).

A `--restate` or `--strike` naming a task with no live entry is **not** a
refusal. It is accepted, appended, and inert in composition ([§FS-rhei-memory.4.2](rhei-memory.spec.md#42-position)) —
the entry may be struck already, or may arrive later, and a writer has no lock
on the fold.

## 5. Output and Exit Codes

A successful record says what was written and what it costs:

```console
$ rhei note "Three concurrent cargo builds under /tmp fill the root disk; build under ~/ag/tmp with --target-dir."
noted as auth.2 — 1 line, composed into later prompts from now on
```

A restate and a strike each name the entry they moved:

```console
$ rhei note --restate auth.2
restated auth.2 — now the newest entry, and this task's slot is spent

$ rhei note --strike auth.1
struck auth.1 — out of composition; its bytes stay in runtime/notes.md
```

Text goes to stdout and diagnostics to stderr ([§FS-rhei-usage.2.2](rhei-usage.spec.md#22-command-surface)).

| Exit | When |
|---|---|
| `0` | The record was appended. |
| `1` | A refusal of [§FS-rhei-note.4](rhei-note.spec.md#4-refusals), or the store could not be written. |
| `2` | Usage: no form given, two forms given, or no task id from `--task` or the environment. |

An unresolvable `--task` id is [§FS-rhei-errors.1.3](rhei-errors.spec.md#13-near-misses)'s near miss, under the help
line that names `rhei list`, exactly as every other ticket-taking command
reports one.

## Related Specifications

- [Mid-Term Memory](rhei-memory.spec.md) — the store, the block, and the fold
- [Reset Command](rhei-reset.spec.md) — what a reset takes with it
- [Complete Command](rhei-complete.spec.md) — the ledger this lock is borrowed from
- [Panta root architecture](../architecture/rhei-panta.spec.md) — the project execution root
