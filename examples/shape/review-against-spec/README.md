# Review Against a Spec: the Checks Are Subtasks of the Reading

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

Pull request 412 replaces the in-memory CSV export with a streaming one. It
claims to meet three points of its spec: requirement R1, goal G1 and non-goal
N1. Somebody reads the change once, holds it to each point, and gives one
verdict. The verdict is what anybody reads later; each check is owed to it, and
each starts from what the reading found.

## The two shapes

Both run one machine, `states.yaml`.

**Flat** — the reading, each check and the verdict are siblings, joined by
exports (`flat/tasks/01-review-pull-request-412.md`):

```markdown
### Task 1: Read pull request 412
**Provides:** reading

### Task 2: Check R1, exports stream their rows
**State:** check
**Prior:** Task 1
**Consumes:** 1:reading
**Provides:** finding

### Task 5: Write the verdict
**Prior:** Task 2, Task 3, Task 4
**Consumes:** 2:finding, 3:finding, 4:finding
```

**Nested** — the reading is a supervisor with one child per point, and its own
last visit writes the verdict (`nested/tasks/01-review-pull-request-412.md`):

```markdown
### Task 1: Review pull request 412 against its spec
**State:** supervising

#### Task 1.1: Check R1, exports stream their rows
**State:** check
```

The `supervising` state is what makes the parent steer
([§FS-rhei-supervision](../../../docs/functional-spec/rhei-supervision.spec.md#fs-rhei-supervision-subtree-supervision-specification)):

```yaml
supervising:
  execute_on: child-terminal
  visits: 8
```

Its edges are tried in order — `visitCount >= visits` to a human, then
`openDescendants < 1` to `completed`, then the self-loop that releases the next
check — so the visit that finds every check in is the one that writes the
verdict. Each check is briefed under `runtime/supervise/` with the part of the
reading it needs; the flat checks each consume the whole reading instead.

## What the next task sees

A task added after the nested run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history nested -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task nested.1: Review pull request 412 against its spec — completed — Request changes: G1 fails, a 1 GB export peaks at 340 MB; R1 and N1 hold. — 3 subtasks: 3 completed
```
<!-- /rhei:plan-history nested -->

One line, and it is the verdict. The flat run tells the same task five lines,
of which the reader wants the last:

<!-- rhei:plan-history flat -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task flat.1: Read pull request 412 — completed — Read pull request 412: a streaming CSV writer behind the existing export command.
- Task flat.2: Check R1, exports stream their rows — completed — R1 holds: rows go to the file through a 64 KB buffer, never a whole file.
- Task flat.3: Check G1, a 1 GB export stays under 200 MB — completed — G1 fails: the 1 GB fixture peaks at 340 MB; the writer buffers a page before flushing.
- Task flat.4: Check N1, no new export format — completed — N1 holds: `--format` still accepts csv and json, nothing else.
- Task flat.5: Write the verdict — completed — Request changes: G1 fails, a 1 GB export peaks at 340 MB; R1 and N1 hold.
```
<!-- /rhei:plan-history flat -->

## What the person sees

The nested run's console task tree folds the finished review into its line
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree)):

<!-- rhei:task-tree nested -->
```text
   4 tasks · source order
  ✓ nested.1                   completed   agent×4  <t> — 3 subtasks: 3 completed
```
<!-- /rhei:task-tree nested -->

`agent×4` is the reading's four visits: one to brief, one after each check.
`rhei list --parent nested.1` opens the fold. The flat run's tree is one row
per task:

<!-- rhei:task-tree flat -->
```text
   5 tasks · source order
  ✓ flat.1                     completed   agent  <t>
  ✓ flat.2                     completed   agent  <t>
  ✓ flat.3                     completed   agent  <t>
  ✓ flat.4                     completed   agent  <t>
  ✓ flat.5                     completed   agent  <t>
```
<!-- /rhei:task-tree flat -->

## The ruling

**Subtasks**: the verdict is the memory, each check is owed to it, and the
checks start inside the reading, so the reading steers, integrates and speaks
for them
([§FS-rhei-shape.3.2](../../../docs/functional-spec/rhei-shape.spec.md#32-the-three-reasons-a-task-has-children)).

## When the other shape is right anyway

When a check is read by name on its own — a requirement whose owner signs it
off, a finding a later task consumes — it is memory outside the verdict, and
that check is a task beside the reading
([§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test)).

## Run it

```bash
cargo xtask examples run shape-review-against-spec-flat
cargo xtask examples run shape-review-against-spec-nested
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run. A real reviewer would also give `supervising` and `check` a
`snapshot:` block, so each check continues the reading's transcript instead of
reading the change again; the mock agent has no transcript to continue.
