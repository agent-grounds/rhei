# FS-rhei-list: `rhei list`

Read-only listing of tasks in a plan, with filters for state, assignee, kind,
dependency, hierarchy, free-text, and readiness. Modeled after `bd list` from
beads, restricted to what Rhei stores in the plan — the markdown task fields and
the frontmatter an author wrote beside them — and never to anything Rhei does not
store at all. There are no labels and no timestamps because no plan holds one; a
`priority` reaches the listing where a plan authored one as task metadata
([§FS-rhei-transitions.2](rhei-transitions.spec.md#2-rhei-file-metadata-format)).

## 1. Usage

```bash
rhei list <RHEI_PLAN> [FILTERS] [--rhei <RHEI_ID>] [--limit N] [--json]
```

`<RHEI_PLAN>` is a single-file plan, a Directory Workspace, or a Panta project
directory. Listing is project-wide: filters apply across every rhei, and
`--rhei` narrows the listing to named rheis ([§FS-rhei-panta.6.4](rhei-panta.spec.md#64-reset-validate-list-viz)).

## 2. Options

| Flag                     | Description                                                                       |
|--------------------------|-----------------------------------------------------------------------------------|
| `--rhei <RHEI_ID>`       | Only tickets in the named rheis. Repeatable. An id that names no rhei in the project is an error listing the available ids. [§FS-rhei-panta.6.4](rhei-panta.spec.md#64-reset-validate-list-viz) |
| `--state <STATE>`        | Filter by state. Repeatable; comma-separated also accepted. Aliases are normalized per owning machine. A state no loaded machine declares is an error (§2.1). |
| `--assignee <ASSIGNEE>`  | Exact `**Assignee:**` match. Mutually exclusive with `--no-assignee`.            |
| `--no-assignee`          | Only tasks with no `**Assignee:**` field.                                         |
| `--kind <KIND>`          | Filter by node kind (e.g. `task`, `bug`, `spec`). Case-insensitive.               |
| `--has-prior <TASK_ID>`  | Only tasks that list `<TASK_ID>` in their `**Prior:**` dependencies.              |
| `--parent <TASK_ID>`     | Only direct children of `<TASK_ID>`. Mutually exclusive with `--root`.            |
| `--root`                 | Only top-level tasks (no parent).                                                 |
| `--contains <TEXT>`      | Case-insensitive substring match against task title and content body.             |
| `--terminal`             | Only tasks whose state is terminal in the resolved state machine.                 |
| `--non-terminal`         | Only tasks whose state is non-terminal. Mutually exclusive with `--terminal`.     |
| `--ready`                | Only tasks the ready set holds — descendants all terminal, `**Prior:**` set satisfied, state declared by the machine and neither terminal nor gating, the state's required `inputs:` on disk, and no `poll:` deadline still ahead (§3.1). Mutually exclusive with `--blocked`. |
| `--blocked`              | Only non-terminal tasks that are not ready — the complement of `--ready` (§3.1).  |
| `--limit <N>`            | Cap the number of printed tasks. `0` means no limit (default).                    |
| `--json`                 | Emit a JSON array instead of human-readable text.                                 |

Filters combine with logical AND. Empty result sets are not an error.

### 2.1. Filter values that name nothing

A filter *value* that cannot exist is a different thing from a filter that
matches nothing, and the two must not look alike. `--rhei` already draws that
line: an id naming no rhei is an error listing the available ids "rather than a
silently empty scope" ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)). `--state` follows the same rule — a
value no loaded machine declares is an error naming the states that do exist:

```text
unknown state 'in-reveiw'; states in this project: cancelled, completed, fix, pending, review
```

A silent `(no tasks match the given filters)` reads as "no work is in that
state", which is exactly the wrong conclusion to hand someone whose state was
renamed out from under a script, or who typed it slightly wrong.

Validation is against every machine the project loads, not the `--rhei` scope.
A project runs one machine per rhei ([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding)), so `--rhei billing
--state review` names a state that genuinely exists while no in-scope ticket
can hold it. That is an honest empty result, not a mistake.

`--assignee` takes no such check: any string is a legitimate assignee, and
there is no declared set to check it against.

## 3. Behavior

1. Load the plan and resolve each rhei's state machine the same way
   `rhei validate` does (auto-discovery, `**States:**` field,
   `--state-machine` override). A project resolves one machine per rhei
   ([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding)), and every state judgment below — normalization,
   terminality, gating — uses the machine of the rhei that owns the ticket.
2. Walk the task tree in source order, recording each task with its parent id.
3. Apply filters in order; normalize `--state` values and the task's own state
   through that ticket's machine so aliases match.
4. For `--ready` / `--blocked`, ask the ready set (§3.1) rather than
   re-deriving readiness; the other filters narrow the answer, they never
   change it.
5. Apply `--limit` after filtering.
6. Emit the result. The plan file is **not** modified and no lock is acquired.

### 3.1. What `--ready` means

`--ready` answers "what work could be picked up", so it lists exactly the set
`rhei next` *draws from*: `rhei list` asks the same ready-set scan that
`rhei next` ([§FS-rhei-next.3](rhei-next.spec.md#3-default-behavior-claim-mode)) and `rhei run` ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)) ask, and narrows
the answer with its other filters. It does not re-derive readiness from its own
copy of the conditions — a copy drifts, and the promise this section makes is
only true when there is one definition for it to be true to.

Drawn from, not equal to. `rhei next` narrows the ready set once more before it
claims, keeping only a ticket that carries no `**Assignee:**` and is in its
machine's initial state ([§FS-rhei-next.3](rhei-next.spec.md#3-default-behavior-claim-mode)). So `--ready` is the wider of the two:
everything `rhei next` would claim is listed here, and a listed ticket may still
be one `rhei next` refuses. That is not the two surfaces disagreeing about
readiness — it is `rhei next` asking a second question, about availability,
that `--ready` deliberately does not ask.

A ticket is ready when every descendant is terminal, every task in its
`**Prior:**` field is in a successful terminal state, its own state is one the
machine declares and is neither terminal nor gating, every required `inputs:`
artifact declared on that state exists on disk, and no `poll:` deadline recorded
for that state is still in the future. An input marked `optional: true` is not
part of it, exactly as it is not for the scheduler ([§FS-rhei-states.3](rhei-states.spec.md#3-artifact-contracts)).

The declared-state condition is one only `rhei list` can reach. Every command
that schedules validates the plan first and refuses it outright when a ticket
holds a state no machine declares, but listing is read-only and loads leniently,
so a misspelled state — or a machine edited after the plan was written — reaches
the readiness question intact. Nothing can be said about readiness in a state
that does not exist, so such a ticket is not ready, and `--blocked` is where it
is reported.

A ticket whose subtree is still open is not work anyone can be handed: its
children are. Once every descendant is terminal the parent is ordinary claimable
work and is listed like any other ticket, because a non-leaf ticket is a task in
its own right ([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)).

`--ready` reports *readiness*, not *availability*: a ready ticket that already
carries an `**Assignee:**` is still listed, because whether someone has claimed
it is a separate question with its own filters. Compose them for the narrower
answer — `rhei list --ready --no-assignee` is the unclaimed ready work.

`--blocked` is the exact complement of `--ready`: every non-terminal ticket is
one or the other, never both and never neither. An operator asking "why is this
not moving?" therefore gets an answer from a single flag, whether the reason is
an unsatisfied `**Prior:**`, a missing required input, a gating state awaiting a
human, a `poll:` retry whose deadline has not come round, a supervisor holding
the ticket ([§FS-rhei-supervision.3.2](rhei-supervision.spec.md#32-readiness)), a state no machine declares, or a
non-leaf whose subtree is still open.

## 4. Output

### 4.1. Text (default)

One task per line, indented two spaces per depth level *within its rhei* — the
Panta qualification segment adds no indentation, so top-level tickets stay
flush-left — in source order:

```text
Task release.1: Define pipeline contracts [pending]
  Task release.1.1: Capture deployment events [pending]
Task release.2: Bootstrap environments [pending] (prior: release.1)
Task release.3: Roll out release bot [in-progress] (prior: release.1, release.2) @claude-code
```

Ticket ids are project-qualified, including for a bare rhei loaded directly:
`release.rhei.md` is the single rhei of an implicit Panta with the id `release`
([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior), [§AR-rhei-panta.3](../architecture/rhei-panta.spec.md#3-identity-and-id-namespacing)).

Tickets print in the merged graph's source order: discovered rheis in
deterministic discovery order, with the `basin` rhei's tickets last because the
basin loads after every discovered rhei ([§FS-rhei-panta.4](rhei-panta.spec.md#4-invisibility)). No rhei-level
headings or visual de-emphasis are applied; rhei-level grouping is deferred
([§FS-rhei-panta.3](rhei-panta.spec.md#3-one-unified-view)).

A ticket whose current state is a **person-waiting poll** — a state whose
`poll` block declares `waiting_on` ([§FS-rhei-states.2.5](rhei-states.spec.md#25-waiting-on-a-person)) — carries a
`(waiting on <label>)` suffix directly after its state, so the one listing an
operator checks first distinguishes a ticket waiting on a human being from one
being worked:

```text
Task fix.approval: Get the plan approved [plan-approval] (waiting on author)
```

The suffix qualifies the state, so it precedes `(prior: …)` and `@<assignee>`,
which are properties of the ticket. It is absent for every other state,
including a poll that waits on a machine.

The `(prior: …)` suffix is omitted when the task has no prerequisites; the
`@<assignee>` suffix is omitted when the task is unclaimed.

A rhei holding no tickets is named after the ticket lines, in the wording
`rhei render --format progress` already uses for it:

```text
Task auth.1: Rotate signing keys [pending]

Billing (billing): (no tickets yet)
```

`rhei init` ends by telling the reader to run `rhei new "<title>"`, which makes
the very next `rhei list` the moment a project holds one rhei and no tickets —
and a listing that showed nothing at all would read as though the create had
not landed. Only the text output names them, and only when no filter is active:
a filter asks a question about tickets, and a rhei with none has no answer to
give. The JSON array is unchanged, because its shape is a contract and an empty
rhei is not a ticket. This is not rhei-level grouping, which stays deferred
([§FS-rhei-panta.3](rhei-panta.spec.md#3-one-unified-view)).

When no task matches, `rhei list` prints `(no tasks match the given filters)`
and exits 0.

### 4.2. JSON (`--json`)

A flat array of objects (no hierarchy nesting); the `parent` field carries the
parent id when present.

```json
[
  {
    "id": "release.2",
    "kind": "task",
    "title": "Bootstrap environments",
    "state": "draft",
    "assignee": null,
    "prior": ["release.1"],
    "parent": null,
    "depth": 1
  }
]
```

Fields are stable: `id`, `kind`, `title`, `state` (raw, as authored), `assignee`
(string or `null`), `prior` (array of id strings), `parent` (string or `null`),
`depth` (1-based depth within the owning rhei — a top-level ticket is `1`; the
Panta qualification segment does not count).

Two additive fields follow one rule between them: **presence is the
declaration**. Each is emitted only on a ticket it means something for, and
omitted rather than `null` everywhere else, so a plan that uses neither emits
exactly the object it emitted before. Their shape is as stable as the eight
fields above; what varies is whether a ticket carries them at all.

`waiting_on` is present **only** on a ticket whose current state is a
person-waiting poll, carrying that state's `poll.waiting_on` label
([§FS-rhei-states.2.5](rhei-states.spec.md#25-waiting-on-a-person)). It is omitted rather than `null` elsewhere, for the
same reason it is omitted on the state machine itself: its presence is the
declaration, and a plan that uses no such state emits exactly the object it
emitted before.

`metadata` is the ticket's **persisted author metadata** — the
`metadata.tasks.<id>` map the plan's frontmatter holds for it
([§FS-rhei-transitions.2](rhei-transitions.spec.md#2-rhei-file-metadata-format)),
keyed by the qualified id this listing prints, with every key spelled as the
author wrote it. Rhei renames nothing and nests nothing: the object is the stored
map, and the values are converted to JSON by the one conversion both JSON
surfaces share ([§FS-rhei-render.3.1](rhei-render.spec.md#31-json)).

What it carries is the author's layer and only that. The keys rhei writes into
the same map are registered once
([§FS-rhei-transitions.2.5](rhei-transitions.spec.md#25-keys-rhei-writes)) and
every one of them is filtered out here, because a read-only query is where a
field becomes a contract and rhei's loop accounting is not one. A key the
register names keeps its runtime meaning even on a plan that authored it, so it
is filtered out there too.

The field is therefore absent in three cases that a caller need not distinguish:
a plan with no frontmatter at all, a task with no entry in `metadata.tasks`, and
a task whose stored entry holds **only** registered keys. The last is the common
one on a plan that has been running — a supervising ticket whose map is
`stateVisits` and `supervision` emits no `metadata` key, not an empty object —
and it is the reason the rule is stated as absence rather than as emptiness. A
caller that wants the stored map whole, counters and all, asks `rhei render
--format json` ([§FS-rhei-render.3.1](rhei-render.spec.md#31-json)).

A ticket that carries both fields carries them independently — a
person-waiting poll with author metadata emits each for its own reason:

```json
{
  "id": "release.2",
  "kind": "task",
  "title": "Bootstrap environments",
  "state": "draft",
  "assignee": null,
  "prior": ["release.1"],
  "parent": null,
  "depth": 1,
  "metadata": { "context": "/checkouts/widget", "priority": "high" }
}
```

No filter selects on `metadata`. The field is emitted on whatever survives the
filters this command already has; `--contains` matches title and body and not
metadata. Filtering on an author's field is a question of its own and is not
answered here.

## Relationship to Other Commands

- `rhei list --ready` lists *all* currently ready tasks; `rhei next --peek`
  selects the *single* task that would be claimed next.
- `rhei list` never mutates plan state; for state changes use `rhei transition`,
  `rhei next`, or `rhei complete`.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — grammar and semantic constraints
- [States Specification](rhei-states.spec.md) — state machine format and terminal/gating semantics
- [Next Command](rhei-next.spec.md) — single-task claim with `--peek`
- [Transition Command](rhei-transition-cmd.spec.md) — atomic state change
