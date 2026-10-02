# Opening the Draft Pull Request: a Program State, Not a Task

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

Issue 87 asks for a CSV export that streams its rows. The spec point and the
failing test are committed on `fix/issue-87`, a draft pull request is opened
for them, and the change is implemented on that pull request. The
implementation needs to know which pull request it works on, and so does
anyone who looks at the issue later.

No forge is touched: the `opening` program appends the pull request to
`forge/pulls.md`, the stand-in forge each shape carries, and numbers it from
what is already there.

## The two shapes

Both run one machine, `states.yaml`, whose `opening` state is the same program
in both. Only where it sits differs.

**State** — opening is a program state of the task that committed, `commit →
opening`, and the pull request is that task's export
(`state/tasks/01-stream-the-export.md`):

```markdown
### Task 1: Write the spec and the failing test
**State:** commit
**Provides:** pr

### Task 2: Implement the change
**Prior:** Task 1
**Consumes:** 1:pr
```

**Task** — opening is a task of its own (`task/tasks/01-stream-the-export.md`):

```markdown
### Task 2: Open the draft pull request
**State:** opening
**Prior:** Task 1
**Provides:** pr

### Task 3: Implement the change
**Prior:** Task 2
**Consumes:** 2:pr
```

## What the next task sees

A task added after the state run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history state -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task state.1: Write the spec and the failing test — completed — Spec and failing test committed; draft PR #412, from fix/issue-87.
- Task state.2: Implement the change — completed — Implemented the streaming export on the draft pull request; `streams_rows` passes.
```
<!-- /rhei:plan-history state -->

The pull request is a clause of the line of the task that committed: the
commit is what it was for, and the pull request is where that commit now
lives. The implementation consumes it as `1:pr`, keyed by that task. The task
run tells the same task:

<!-- rhei:plan-history task -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task task.1: Write the spec and the failing test — completed — Spec point 3 and the failing test `streams_rows` committed on fix/issue-87.
- Task task.2: Open the draft pull request — completed — Opened draft PR #412, from fix/issue-87.
- Task task.3: Implement the change — completed — Implemented the streaming export on the draft pull request; `streams_rows` passes.
```
<!-- /rhei:plan-history task -->

The extra line says only that a pull request was opened, which the committing
task's own line could have said.

## What the person sees

The state run's console task tree gives the work two rows
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree)),
the first labelled `program×2` for its two invocations, the commit and the
opening:

<!-- rhei:task-tree state -->
```text
   2 tasks · source order
  ✓ state.1                    completed   program×2  <t>
  ✓ state.2                    completed   agent  <t>
```
<!-- /rhei:task-tree state -->

The task run adds a row whose whole content is a program that took a moment:

<!-- rhei:task-tree task -->
```text
   3 tasks · source order
  ✓ task.1                     completed   agent  <t>
  ✓ task.2                     completed   program  <t>
  ✓ task.3                     completed   agent  <t>
```
<!-- /rhei:task-tree task -->

Either way the person reads the pull request where it is, on the forge.

## The ruling

**State**: the pull request is the committing task's own outcome
([§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test),
corollary 3). A later step reads it, but reads it *as that task's*: the state
writes it into the task's export, which is keyed by the task and never by the
state that wrote it, and onto the forge, where every later reader finds it.
Corollary 3's question separates it from a reproducer: *could the product be
wrong while the task's outcome is right?* A pull request opened for the right
commit cannot. It is the decision table's row for *opening the pull request
for a change a task committed*
([§FS-rhei-shape.4](../../../docs/functional-spec/rhei-shape.spec.md#4-the-decision-table)).

## When the other shape is right anyway

When opening the pull request is work of its own that can be wrong while the
commit is right — a pull request that bundles the commits of several tasks,
written up for a reviewer, whose description someone reads and corrects — its
product is an outcome of its own, and corollary 1 makes it a task.

## Run it

```bash
cargo xtask examples run shape-draft-pull-request-state
cargo xtask examples run shape-draft-pull-request-task
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run.
