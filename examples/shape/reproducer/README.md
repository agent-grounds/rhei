# The Reproducer: a Sibling Task, Not a Step of Triage

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

Issue 87 says the parser panics on an integer literal wider than 64 bits.
Someone triages it, someone reproduces it, someone fixes the parser, and the
gate runs the test suite. The fix and the gate both need the reproduction
**by name**: the fix to know when it is done, the gate to prove it.

## The two shapes

Both run one machine, `states.yaml`, with a single `work` state. Only where the
reproduction sits differs.

**Flat** — the reproduction is a task beside triage
(`flat/tasks/01-fix-the-overflow.md`):

```markdown
### Task 2: Reproduce the overflow
**Prior:** Task 1
**Provides:** reproducer

### Task 3: Fix the parser
**Prior:** Task 2
**Consumes:** 2:reproducer
```

**Nested** — the reproduction is a step of triage, under it
(`nested/tasks/01-fix-the-overflow.md`):

```markdown
### Task 1: Triage the overflow report
#### Task 1.1: Search for a duplicate
#### Task 1.2: Reproduce the overflow
**Provides:** reproducer

### Task 2: Fix the parser
**Prior:** Task 1, Task 1.2
**Consumes:** 1.2:reproducer
```

A third shape tempts a template author more than either: the reproduction as a
*state* of triage, `triage → reproduce → summarize`. A state may write its
task's export, so triage could declare `**Provides:** reproducer` itself and the
fix consume `1:reproducer`. It is still the wrong shape.
[§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test)
tells a task's own outcome from an outcome of its own by one question: *could
the product be wrong while the task's outcome is right?* A script can fail to
reproduce what triage rightly judged real, so the reproduction is an outcome of
its own, and it is a task.

## What the next task sees

A task added after the nested run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history nested -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task nested.1: Triage the overflow report — completed — Issue 87 is real and new: no earlier report, and its input panics the parser. — 2 subtasks: 2 completed
- Task nested.2: Fix the parser — completed — The parser rejects the literal with E0412 instead of panicking; the reproducer exits 1.
- Task nested.3: Run the gate — completed — Gate green: 412 tests pass and the reproducer exits 1 with E0412.
```
<!-- /rhei:plan-history nested -->

The reproduction is gone from the history: triage speaks for its subtree, and
`2 subtasks: 2 completed` is all that is left of it. The flat run tells the
same task:

<!-- rhei:plan-history flat -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task flat.1: Triage the overflow report — completed — Issue 87 is real and new: no earlier report, and its input panics the parser.
- Task flat.2: Reproduce the overflow — completed — Reproduced: a 20-digit literal panics the parser; the script is the `reproducer` export.
- Task flat.3: Fix the parser — completed — The parser rejects the literal with E0412 instead of panicking; the reproducer exits 1.
- Task flat.4: Run the gate — completed — Gate green: 412 tests pass and the reproducer exits 1 with E0412.
```
<!-- /rhei:plan-history flat -->

## What the person sees

The nested run's console task tree folds the finished triage into its line
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree)):

<!-- rhei:task-tree nested -->
```text
   5 tasks · source order
  ✓ nested.1                   completed   agent  <t> — 2 subtasks: 2 completed
  ✓ nested.2                   completed   agent  <t>
  ✓ nested.3                   completed   agent  <t>
```
<!-- /rhei:task-tree nested -->

`rhei list --parent nested.1` opens the fold. The flat run's tree is one row
per task, the reproduction among them:

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

**Flat**: the reproduction is read by name from outside triage, so it is a task
([§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test)),
and triage neither steers, integrates nor speaks for it, so it is a sibling
rather than a child
([§FS-rhei-shape.3.3](../../../docs/functional-spec/rhei-shape.spec.md#33-promotion)).

## When the other shape is right anyway

When nothing after triage reads the reproduction — triage closes the issue as
a duplicate, or the reproduction only decides triage's own verdict — it is
triage's own evidence, and a state of triage, or a line of its result, is the
right place for it.

## Run it

```bash
cargo xtask examples run shape-reproducer-flat
cargo xtask examples run shape-reproducer-nested
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run.
