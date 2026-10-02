# Candidate Lookup: a Program State, Not a Search Task

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

Issue 87 says the parser panics on an integer literal wider than 64 bits.
Before anyone fixes it, triage decides whether it duplicates an earlier issue.
A program searches the tracker for candidates; an agent judges them and gives
the verdict. Everything after triage reads the verdict, and nothing reads the
list of candidates except the judgement made from it.

No forge is touched: the lookup searches `forge/issues.md`, the stand-in
tracker each shape carries.

## The two shapes

The `lookup` program is the same in both. Because the step after it is exactly
what differs, each shape keeps its own machine beside its plan.

**State** — triage runs `lookup → verdict`, and the candidates reach the verdict
as a handoff between the two states
([§FS-rhei-states.3.2](../../../docs/functional-spec/rhei-states.spec.md#32-state-handoffs))
(`state/tasks/01-triage-and-fix.md`):

```markdown
### Task 1: Triage issue 87
**State:** lookup

### Task 2: Fix the parser
**Prior:** Task 1
```

**Task** — the lookup is a task of its own, and the verdict is the task after it
(`task/tasks/01-triage-and-fix.md`):

```markdown
### Task 1: Search for duplicate candidates
**State:** lookup
**Provides:** candidates

### Task 2: Judge whether issue 87 is a duplicate
**Prior:** Task 1
**Consumes:** 1:candidates
```

## What the next task sees

A task added after the state run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history state -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task state.1: Triage issue 87 — completed — Issue 87 is new: #52 is about float literals and #61 was a lexer crash fixed in 0.4.
- Task state.2: Fix the parser — completed — The parser rejects the literal with E0412 instead of panicking.
```
<!-- /rhei:plan-history state -->

Triage's line is the verdict, and the verdict names the candidates it ruled
out. The task run tells the same task:

<!-- rhei:plan-history task -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task task.1: Search for duplicate candidates — completed — Found 2 candidates for issue 87: #52 and #61.
- Task task.2: Judge whether issue 87 is a duplicate — completed — Issue 87 is new: #52 is about float literals and #61 was a lexer crash fixed in 0.4.
- Task task.3: Fix the parser — completed — The parser rejects the literal with E0412 instead of panicking.
```
<!-- /rhei:plan-history task -->

The search's line says how many candidates there were. The line after it says
which of them mattered, and why.

## What the person sees

The state run's console task tree has one row for triage
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree)),
and `agent×2` counts its two invocations, the lookup's program and the
verdict's agent: the label counts them all and names only the driver of the
last:

<!-- rhei:task-tree state -->
```text
   2 tasks · source order
  ✓ state.1                    completed   agent×2  <t>
  ✓ state.2                    completed   agent  <t>
```
<!-- /rhei:task-tree state -->

The task run gives the search a row of its own:

<!-- rhei:task-tree task -->
```text
   3 tasks · source order
  ✓ task.1                     completed   program  <t>
  ✓ task.2                     completed   agent  <t>
  ✓ task.3                     completed   agent  <t>
```
<!-- /rhei:task-tree task -->

## The ruling

**State**: the candidates are read by the verdict, the next state of the same
task, and by nothing else, so the lookup is a state
([§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test),
corollary 2). That is the decision table's row for *a lookup that must happen
before a verdict*: a **program state**, because its product is read by the
verdict in the same task
([§FS-rhei-shape.4](../../../docs/functional-spec/rhei-shape.spec.md#4-the-decision-table)).
The verdict lands in triage's result, where every later step finds it.

## When the other shape is right anyway

When something outside triage reads the candidates themselves — a person
audits what the search turned up, or a later step merges every candidate the
verdict confirmed — the list is named from outside its state chain, and
corollary 1 makes the lookup a task with `**Provides:** candidates`.

## Run it

```bash
cargo xtask examples run shape-candidate-lookup-state
cargo xtask examples run shape-candidate-lookup-task
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run.
