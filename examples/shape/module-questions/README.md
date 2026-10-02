# Questions About a Module: Subtasks Under the Report, Not Siblings

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

The storage module must survive a crash at any point. Three questions are
asked about it — when the write-ahead log is flushed, when a page is fsynced,
what recovery replays — and a report says how the answers add up. Somebody
reads the report; nobody reads one answer without the other two, because
crash safety is in how they fit together.

Nothing outside the plan is touched, and no code is read: the mock answers
each question from its title, so there is no stand-in file.

## The two shapes

Both run one machine, `states.yaml`, with one agent state. Only where the
report sits differs.

**Nested** — the report is the parent of the three questions
(`nested/tasks/01-storage-crash-safety.md`):

```markdown
### Task 1: Report how storage survives a crash
#### Task 1.1: When is the WAL flushed?
#### Task 1.2: When is a page fsynced?
#### Task 1.3: What does recovery replay?
```

**Flat** — the questions and the report are siblings, the report after the
three answers (`flat/tasks/01-storage-crash-safety.md`):

```markdown
### Task 1: When is the WAL flushed?
### Task 2: When is a page fsynced?
### Task 3: What does recovery replay?
### Task 4: Report how storage survives a crash
**Prior:** Task 1, Task 2, Task 3
```

## What the next task sees

A task added after the nested run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history nested -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task nested.1: Report how storage survives a crash — completed — Crash-safe: the WAL reaches disk before any page it covers, and recovery replays it from the last checkpoint. — 3 subtasks: 3 completed
```
<!-- /rhei:plan-history nested -->

One line, the report, which speaks for the three answers it was made of; the
answers are a `rhei list --parent nested.1` away. The flat run tells the same
task:

<!-- rhei:plan-history flat -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task flat.1: When is the WAL flushed? — completed — On every commit, before the commit returns.
- Task flat.2: When is a page fsynced? — completed — At a checkpoint, after the WAL records that cover it are flushed.
- Task flat.3: What does recovery replay? — completed — The WAL from the last checkpoint forward.
- Task flat.4: Report how storage survives a crash — completed — Crash-safe: the WAL reaches disk before any page it covers, and recovery replays it from the last checkpoint.
```
<!-- /rhei:plan-history flat -->

Four lines of equal rank, three of them fragments the fourth already
combined.

## What the person sees

The nested run's console task tree
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree))
folds the questions into the report's row:

<!-- rhei:task-tree nested -->
```text
   4 tasks · source order
  ✓ nested.1                   completed   agent  <t> — 3 subtasks: 3 completed
```
<!-- /rhei:task-tree nested -->

The flat run lists every question beside the report:

<!-- rhei:task-tree flat -->
```text
   4 tasks · source order
  ✓ flat.1                     completed   agent  <t>
  ✓ flat.2                     completed   agent  <t>
  ✓ flat.3                     completed   agent  <t>
  ✓ flat.4                     completed   agent  <t>
```
<!-- /rhei:task-tree flat -->

## The ruling

**Nested**: the report **integrates** the answers — its deliverable is made
*out of* theirs, and it is not any one answer
([§FS-rhei-shape.3.2](../../../docs/functional-spec/rhei-shape.spec.md#32-the-three-reasons-a-task-has-children)).
That is one of the three reasons a task has children, so the questions are
subtasks under the report. It is the decision table's row for *the same parts,
plus an integration somebody reads*
([§FS-rhei-shape.4](../../../docs/functional-spec/rhei-shape.spec.md#4-the-decision-table)).
The flat shape keeps the integration but puts it beside what it integrates, so
the one line that could speak for the subtree is the fourth of four.

## When the other shape is right anyway

When nothing is synthesized — each answer is read on its own, by whoever
asked that question, and no report is made out of them — no parent owes
anything, and the questions are flat siblings
([§FS-rhei-shape.3.1](../../../docs/functional-spec/rhei-shape.spec.md#31-the-default-is-flat)).

## Run it

```bash
cargo xtask examples run shape-module-questions-nested
cargo xtask examples run shape-module-questions-flat
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run.
