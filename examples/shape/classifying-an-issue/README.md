# Classifying an Issue: States of the Triage, Not a Task

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

Issue 87 arrived with no kind. Triage judges whether it is real and which kind
it is — bug, feature, usability or tokens — the issue is labelled with that
kind, and a later step routes it to the queue the kind names. The router needs
the kind by name, and so does anyone who opens the issue later.

No forge is touched: the `labelling` program writes the label into
`forge/issue-87.md`, the stand-in issue each shape carries.

## The two shapes

Both run one machine, `states.yaml`: an agent state, `classify`, judges the
kind and hands it to a program state, `labelling`, through a state output
([§FS-rhei-states.3.2](../../../docs/functional-spec/rhei-states.spec.md#32-state-handoffs)).
Only which task runs them differs.

**State** — the task that triages runs `classify → labelling`, and the kind is
its export (`state/tasks/01-triage-issue-87.md`):

```markdown
### Task 1: Triage issue 87
**State:** classify
**Provides:** kind

### Task 2: Route the issue
**Prior:** Task 1
**Consumes:** 1:kind
```

**Task** — classifying is a task of its own, after triage
(`task/tasks/01-triage-issue-87.md`):

```markdown
### Task 2: Classify the issue
**State:** classify
**Prior:** Task 1
**Provides:** kind

### Task 3: Route the issue
**Prior:** Task 2
**Consumes:** 2:kind
```

## What the next task sees

A task added after the state run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history state -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task state.1: Triage issue 87 — completed — Issue 87 is a real bug: the parser panics on a valid 64-bit literal; labelled `bug`.
- Task state.2: Route the issue — completed — Routed issue 87 to the parser's bug queue.
```
<!-- /rhei:plan-history state -->

Triage's line says what the issue is, and the kind is part of that sentence.
The task run tells the same task:

<!-- rhei:plan-history task -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task task.1: Triage issue 87 — completed — Issue 87 is real: the parser panics on a valid 64-bit literal.
- Task task.2: Classify the issue — completed — Labelled issue 87 `bug`.
- Task task.3: Route the issue — completed — Routed issue 87 to the parser's bug queue.
```
<!-- /rhei:plan-history task -->

Triage's line now stops short of the one thing triage was for, and the next
line carries a single word of it.

## What the person sees

The state run's console task tree
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree))
gives triage one row, where `program×2` counts its two invocations, the judgement and the
label:

<!-- rhei:task-tree state -->
```text
   2 tasks · source order
  ✓ state.1                    completed   program×2  <t>
  ✓ state.2                    completed   agent  <t>
```
<!-- /rhei:task-tree state -->

The task run splits the same two invocations across two rows:

<!-- rhei:task-tree task -->
```text
   3 tasks · source order
  ✓ task.1                     completed   agent  <t>
  ✓ task.2                     completed   program×2  <t>
  ✓ task.3                     completed   agent  <t>
```
<!-- /rhei:task-tree task -->

Either way the person reads the kind where it is kept, as the label on the
issue.

## The ruling

**States of the task that triages**: the kind is that task's own outcome
([§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test),
corollary 3). The router reads it, but reads it *as triage's*: `labelling`
writes it into triage's export, keyed by the task and never by the state that
wrote it, and onto the forge as the label every later reader reads there.
Corollary 3's question separates it from a reproducer: *could the product be
wrong while the task's outcome is right?* A kind cannot — it is the outcome.
It is the decision table's row for *classifying an issue that carries no
kind*: an agent state, then a program state, of the task that triages it
([§FS-rhei-shape.4](../../../docs/functional-spec/rhei-shape.spec.md#4-the-decision-table)).

## When the other shape is right anyway

When classifying is not part of triaging — a separate pass that labels a
backlog triage already judged, or a classification someone reviews and
overrules on its own — its product is no other task's outcome, and it is a
task.

## Run it

```bash
cargo xtask examples run shape-classifying-an-issue-state
cargo xtask examples run shape-classifying-an-issue-task
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run.
