# Spec First: the Contract Is a Task, Not a Handoff

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

Issue 412 asks for a CSV export that streams its rows. The work starts by
writing what "done" means — a spec point and a test that fails today — and only
then builds the change. After the change, a review holds it to that contract
and the gate runs the contract's test **by name**. Three later steps read the
contract: the one that builds to it, and two that come after that one, the
review and the gate.

## The two shapes

Both run one machine, `states.yaml`. Only where the contract lives differs.

**Task** — the contract is the first task's export
(`task/tasks/01-stream-the-export.md`):

```markdown
### Task 1: Write the spec and the failing test
**Provides:** contract

### Task 2: Implement the streaming export
**Prior:** Task 1
**Consumes:** 1:contract
```

The review and the gate consume `1:contract` the same way.

**State** — the contract is a handoff between two states of one task,
`specify → implement` (`state/tasks/01-stream-the-export.md`):

```markdown
### Task 1: Specify and implement the streaming export
**State:** specify

### Task 2: Review the change against the contract
**Prior:** Task 1
```

`specify` declares the contract as an `outputs:` artifact and `implement` as an
`inputs:` one, so it reaches `implement` and nothing else
([§FS-rhei-states.3.2](../../../docs/functional-spec/rhei-states.spec.md#32-state-handoffs)).
The review has no `**Consumes:**` to write: there is no export to name, only a
file under `runtime/contract/` that the review's body would have to know about.

## What the next task sees

A task added after the task run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history task -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task task.1: Write the spec and the failing test — completed — Contract: point 3 of docs/export.spec.md, and `streams_rows_under_200_mb` failing at 340 MB.
- Task task.2: Implement the streaming export — completed — The export streams its rows through a 64 KB buffer; `streams_rows_under_200_mb` passes.
- Task task.3: Review the change against the contract — completed — Approved: the change holds point 3, and the test the contract names passes.
- Task task.4: Run the gate — completed — Gate green: 412 tests pass, `streams_rows_under_200_mb` among them.
```
<!-- /rhei:plan-history task -->

The contract has a line of its own, and its result says what it is. The state
run tells the same task:

<!-- rhei:plan-history state -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task state.1: Specify and implement the streaming export — completed — The export streams its rows through a 64 KB buffer; `streams_rows_under_200_mb` passes.
- Task state.2: Review the change against the contract — completed — Approved: the change holds point 3, and the test the contract names passes.
- Task state.3: Run the gate — completed — Gate green: 412 tests pass, `streams_rows_under_200_mb` among them.
```
<!-- /rhei:plan-history state -->

The contract is gone. Its spec point and its test survive only as words in
the lines of the steps that came after it.

## What the person sees

The task run's console task tree gives the contract a row and a duration of
its own, and a cost of its own once a real agent's spend is accounted
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree)):

<!-- rhei:task-tree task -->
```text
   4 tasks · source order
  ✓ task.1                     completed   agent  <t>
  ✓ task.2                     completed   agent  <t>
  ✓ task.3                     completed   agent  <t>
  ✓ task.4                     completed   agent  <t>
```
<!-- /rhei:task-tree task -->

The state run folds it into the first task's two agent visits:

<!-- rhei:task-tree state -->
```text
   3 tasks · source order
  ✓ state.1                    completed   agent×2  <t>
  ✓ state.2                    completed   agent  <t>
  ✓ state.3                    completed   agent  <t>
```
<!-- /rhei:task-tree state -->

## The ruling

**Task**: the contract is consumed by name by implement, by the review and by
the gate, so it is named from outside its own state chain
([§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test),
corollary 1), and it is the decision table's row for *a reproducer, a contract
or a gate's verdict a later step consumes by name*
([§FS-rhei-shape.4](../../../docs/functional-spec/rhei-shape.spec.md#4-the-decision-table)).
Corollary 3 does not keep it a state: *could the product be wrong while the
task's outcome is right?* A test can pin the wrong behaviour while the change is
right, so the contract is an outcome of its own.

## When the other shape is right anyway

When nothing but the change itself ever reads the contract — no review holds
the change to it and no gate runs its test by name — it reaches only the next
state, and `specify → implement` is the right shape
([§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test),
corollary 2).

## Run it

```bash
cargo xtask examples run shape-spec-first-task
cargo xtask examples run shape-spec-first-state
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run.
