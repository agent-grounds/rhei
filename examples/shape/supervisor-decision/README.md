# A Supervisor's Decision: the Parent's Visit, Not a Child

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

Issue 87 says the toolchain panics on an integer literal wider than 64 bits. A
supervising parent has the crash reproduced, then decides where the fix goes —
the reproducer shows the panic is in the lexer, not the parser where the issue
put it — and has it fixed there. The decision is made **between** the two
children, from what the first one left, and the second one acts on it.

The parent's supervising state is copied from
[`examples/subtree-supervision`](../../subtree-supervision/README.md): it is
woken after each child finishes and releases the next. Nothing outside the
plan is touched, so there is no stand-in file; each brief is written under
`runtime/supervise/`.

## The two shapes

Both run one machine, `states.yaml`. Only where the decision is made differs.

**Visit** — the parent decides in its visit between the children and writes
the decision into the fix's brief (`visit/tasks/01-fix-the-crash.md`):

```markdown
### Task 1: Fix the crash on a 64-bit literal
**State:** supervising

#### Task 1.1: Reproduce the crash
#### Task 1.2: Fix the crash
**Prior:** Task 1.1
```

**Child** — the decision is a child of its own, between the reproducer and the
fix (`child/tasks/01-fix-the-crash.md`):

```markdown
#### Task 1.1: Reproduce the crash
#### Task 1.2: Decide where the fix goes
**Prior:** Task 1.1
#### Task 1.3: Fix the crash
**Prior:** Task 1.2
```

## What the next task sees

A task added after the visit run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history visit -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task visit.1: Fix the crash on a 64-bit literal — completed — Fixed the crash in the lexer, not the parser: the reproducer panicked in its literal scan, so I sent the fix there. — 2 subtasks: 2 completed
```
<!-- /rhei:plan-history visit -->

The parent's line is the decision, because the parent made it: it speaks for
its subtree, and what it decided is part of what it says. The child run tells
the same task:

<!-- rhei:plan-history child -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task child.1: Fix the crash on a 64-bit literal — completed — Fixed the crash in the lexer, as Task 1.2 decided. — 3 subtasks: 3 completed
```
<!-- /rhei:plan-history child -->

The decision has moved out of the one line a later reader gets and into the
folded subtree, behind `rhei list --parent child.1`. The decision is no longer
the parent's to state: its line can only repeat what the child decided.

## What the person sees

Both console task trees fold the subtree into the parent's row
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree)).
The visit run's parent took three visits, one before each child and one to
finish:

<!-- rhei:task-tree visit -->
```text
   3 tasks · source order
  ✓ visit.1                    completed   agent×3  <t> — 2 subtasks: 2 completed
```
<!-- /rhei:task-tree visit -->

The child run's took four, and briefed a child to make the judgement it was
woken to make:

<!-- rhei:task-tree child -->
```text
   4 tasks · source order
  ✓ child.1                    completed   agent×4  <t> — 3 subtasks: 3 completed
```
<!-- /rhei:task-tree child -->

## The ruling

**Visit**: a supervisor's decision between two steps is the parent's own work,
so it is no new node; it lives in the parent's body, its brief and its result.
It is the decision table's row for *a supervisor's own decision between two
steps*
([§FS-rhei-shape.4](../../../docs/functional-spec/rhei-shape.spec.md#4-the-decision-table)).
The parent has children because it **steers** them — it decides what each
child is, between the children rather than before them, and nothing else can
hold that judgement
([§FS-rhei-shape.3.2](../../../docs/functional-spec/rhei-shape.spec.md#32-the-three-reasons-a-task-has-children)).
A decision child takes that judgement away from the one task that holds the
checkpoints, and leaves the parent briefing someone else to steer for it.

## When the other shape is right anyway

When the decision is work of its own that the parent only reads — an
investigation whose report somebody quotes, or a judgement on another tier
than the parent's, with a verdict that can be wrong while the parent's
steering is right — it has a product named from outside its state chain, and
it is a task.

## Run it

```bash
cargo xtask examples run shape-supervisor-decision-visit
cargo xtask examples run shape-supervisor-decision-child
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run.
