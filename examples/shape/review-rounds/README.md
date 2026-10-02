# Review Rounds: a Counted Loop, Not a Task per Round

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

A streaming CSV export is implemented, then reviewed and fixed for two rounds,
then shipped. Each review writes findings and the next fix acts on them. Once
the rounds are over, what ships is the code the last fix left behind; nobody
after the loop reads what round 1 found.

## The two shapes

Both run one machine, `states.yaml`, whose `review → fix` loop is copied from
[`examples/review-fix-visits`](../../review-fix-visits/). Only where the rounds
live differs.

**Loop** — one task runs the rounds as a counted loop of states
(`loop/tasks/01-stream-the-export.md`):

```markdown
### Task 2: Review and fix the export
**State:** review
**Prior:** Task 1
```

`review` and `fix` each allow two visits. Each review pass writes its notes to
the `review-notes` handoff, which the next fix pass reads
([§FS-rhei-states.3.2](../../../docs/functional-spec/rhei-states.spec.md#32-state-handoffs)).

**Tasks** — every round is a task of its own
(`tasks/tasks/01-stream-the-export.md`):

```markdown
### Task 2: Review round 1
### Task 3: Fix round 1
**Prior:** Task 2
### Task 4: Review round 2
**Prior:** Task 3
### Task 5: Fix round 2
**Prior:** Task 4
```

## What the next task sees

A task added after the loop run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history loop -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task loop.1: Implement the streaming export — completed — The export streams its rows; a first cut, ready for review.
- Task loop.2: Review and fix the export — completed — Reviewed and fixed in two rounds: the writer flushes every 64 KB and writes the header once.
- Task loop.3: Ship the export — completed — Shipped: the streaming export is merged.
```
<!-- /rhei:plan-history loop -->

One line for the rounds, saying what the code now does. The tasks run tells the
same task four lines more, one per round:

<!-- rhei:plan-history tasks -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task tasks.1: Implement the streaming export — completed — The export streams its rows; a first cut, ready for review.
- Task tasks.2: Review round 1 — completed — Round 1: the writer flushes a whole page, not every 64 KB.
- Task tasks.3: Fix round 1 — completed — Fixed round 1: the writer flushes every 64 KB.
- Task tasks.4: Review round 2 — completed — Round 2: the header is written again after every flush.
- Task tasks.5: Fix round 2 — completed — Fixed round 2: the header is written once.
- Task tasks.6: Ship the export — completed — Shipped: the streaming export is merged.
```
<!-- /rhei:plan-history tasks -->

## What the person sees

The loop run's console task tree shows the rounds as the review task's four
agent visits
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree)):

<!-- rhei:task-tree loop -->
```text
   3 tasks · source order
  ✓ loop.1                     completed   agent  <t>
  ✓ loop.2                     completed   agent×4  <t>
  ✓ loop.3                     completed   agent  <t>
```
<!-- /rhei:task-tree loop -->

The tasks run gives each round a row, an identity and a result file:

<!-- rhei:task-tree tasks -->
```text
   6 tasks · source order
  ✓ tasks.1                    completed   agent  <t>
  ✓ tasks.2                    completed   agent  <t>
  ✓ tasks.3                    completed   agent  <t>
  ✓ tasks.4                    completed   agent  <t>
  ✓ tasks.5                    completed   agent  <t>
  ✓ tasks.6                    completed   agent  <t>
```
<!-- /rhei:task-tree tasks -->

## The ruling

**Loop**: every round's notes reach only the next state of the same task, so
the rounds are states
([§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test),
corollary 2). That is the first row of the decision table: *a counted
`review → fix` loop where only the final code is read* is **states** of one task
([§FS-rhei-shape.4](../../../docs/functional-spec/rhei-shape.spec.md#4-the-decision-table)).
Promoting each round buys four identities and four result files that nobody
reads.

## When the other shape is right anyway

When something outside the loop reads each round — a supervisor decides after
round 1 whether round 2 runs at all, or a person audits every round's findings
by name — each round's product is named from outside its state chain, and the
rounds are tasks
([§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test),
corollary 1).

## Run it

```bash
cargo xtask examples run shape-review-rounds-loop
cargo xtask examples run shape-review-rounds-tasks
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run.
