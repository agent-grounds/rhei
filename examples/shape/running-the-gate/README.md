# Running the Gate: a Task, Not the Last State of Implement

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

Issue 87 says the parser panics on an integer literal wider than 64 bits. The
parser is fixed, the gate runs the suite, and the fix ships once the gate is
green. Whoever decides to ship — a supervisor, a person — wants the gate's
verdict **in the machine's own words**, by name. When the gate goes red for a
reason outside the change, they want to run the gate again alone.

No suite is run: the gate runs `gate.py`, the stand-in suite each shape
carries, and keeps its last line as the `verdict` export.

## The two shapes

Both run one machine, `states.yaml`, whose `gate` state is the same program in
both. Only where the gate sits differs.

**Task** — the gate is a task of its own (`task/tasks/01-fix-the-overflow.md`):

```markdown
### Task 2: Run the gate
**State:** gate
**Prior:** Task 1
**Provides:** verdict

### Task 3: Ship the fix
**Prior:** Task 2
**Consumes:** 2:verdict
```

**State** — the gate is the last state of implement, `build → gate`
(`state/tasks/01-fix-the-overflow.md`):

```markdown
### Task 1: Implement the overflow fix
**State:** build
**Provides:** verdict

### Task 2: Ship the fix
**Prior:** Task 1
**Consumes:** 1:verdict
```

A state may write its task's export, so the state shape still hands the
verdict to the ship step. It is the strongest form of the tempting shape, and
the pair compares against it on purpose.

## What the next task sees

A task added after the task run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history task -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task task.1: Implement the overflow fix — completed — The parser rejects the literal with E0412 instead of panicking.
- Task task.2: Run the gate — completed — Gate green, in its own words: test result: ok. 412 passed; 0 failed
- Task task.3: Ship the fix — completed — Shipped: the overflow fix is merged with the gate green.
```
<!-- /rhei:plan-history task -->

The gate's line is the suite's own last line. The state run tells the same
task:

<!-- rhei:plan-history state -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task state.1: Implement the overflow fix — completed — The parser rejects the literal with E0412; gate green: test result: ok. 412 passed; 0 failed
- Task state.2: Ship the fix — completed — Shipped: the overflow fix is merged with the gate green.
```
<!-- /rhei:plan-history state -->

The verdict is still there, but it is now a clause of the implementation's
line. It has no line, no result file and no cost of its own: the ship step
reads it as implement's export, and a reader of the history finds it after the
build's summary.

## What the person sees

The task run's console task tree gives the gate a row and a duration of its
own
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree)):

<!-- rhei:task-tree task -->
```text
   3 tasks · source order
  ✓ task.1                     completed   agent  <t>
  ✓ task.2                     completed   program  <t>
  ✓ task.3                     completed   agent  <t>
```
<!-- /rhei:task-tree task -->

The state run folds the gate into implement's row, where `program×2` counts
its two invocations, the build's agent and the gate's program: the label counts
them all and names only the driver of the last:

<!-- rhei:task-tree state -->
```text
   2 tasks · source order
  ✓ state.1                    completed   program×2  <t>
  ✓ state.2                    completed   agent  <t>
```
<!-- /rhei:task-tree state -->

## The ruling

**Task**: the gate's verdict is consumed by name by the ship step and quoted by
whoever decides to ship, so it is named from outside its state chain
([§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test),
corollary 1). It is the decision table's row for *a reproducer, a contract or
a gate's verdict a later step consumes by name*
([§FS-rhei-shape.4](../../../docs/functional-spec/rhei-shape.spec.md#4-the-decision-table)).
Corollary 3 does not keep it a state: *could the product be wrong while the
task's outcome is right?* A gate can go red on a flaky test while the change is
right, so the verdict is an outcome of its own. A task has a result file, a
line in Plan History and a cost of its own, and can be listed, priced or
cancelled by name; a state has none of these
([§FS-rhei-shape.1](../../../docs/functional-spec/rhei-shape.spec.md#1-the-constructs)).

## When the other shape is right anyway

When only the next state of implement reads the gate's result — the gate is
implement's own exit check, and a red gate sends implement back to work rather
than to a reader — it is a state, by corollary 2.

## Run it

```bash
cargo xtask examples run shape-running-the-gate-task
cargo xtask examples run shape-running-the-gate-state
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run.
