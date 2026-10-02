# Waiting on a Person: a Poll State, Not a Wait Task

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

Which encoding the CSV export writes is the author's call. The work asks them
on issue 412, waits for the reply, and then implements the export the way they
answered. The implementation reads the answer **by name**, so it has to be kept
somewhere a later task can reach.

No forge is touched: the author's reply is already in `forge/issue-412.md`, the
stand-in issue each shape carries, so the wait's first look finds it.

## The two shapes

Both run one machine, `states.yaml`. Its `wait` state is a polling program
that declares `waiting_on: author`
([§FS-rhei-states.2.5](../../../docs/functional-spec/rhei-states.spec.md#25-waiting-on-a-person)):
a reply ends the wait, and no reply yet is exit 75, another look after the
interval. Only where the wait sits differs.

**State** — the asking task runs `ask → wait`, and the answer is its export
(`state/tasks/01-stream-the-export.md`):

```markdown
### Task 1: Ask the author which encoding to use
**State:** ask
**Provides:** answer

### Task 2: Implement the export
**Prior:** Task 1
**Consumes:** 1:answer
```

**Task** — the wait is a task of its own between the question and the work
(`task/tasks/01-stream-the-export.md`):

```markdown
### Task 1: Ask the author which encoding to use
### Task 2: Wait for the author's answer
**State:** wait
**Provides:** answer
### Task 3: Implement the export
**Consumes:** 2:answer
```

## What the next task sees

A task added after the state run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history state -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task state.1: Ask the author which encoding to use — completed — The author answered: UTF-8 with a byte-order mark, so Excel opens the file.
- Task state.2: Implement the export — completed — The export writes UTF-8 with a byte-order mark, as the author answered.
```
<!-- /rhei:plan-history state -->

The question's line is the answer to it. The task run tells the same task:

<!-- rhei:plan-history task -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task task.1: Ask the author which encoding to use — completed — Asked the author on issue 412 which encoding the export should write.
- Task task.2: Wait for the author's answer — completed — The author answered: UTF-8 with a byte-order mark, so Excel opens the file.
- Task task.3: Implement the export — completed — The export writes UTF-8 with a byte-order mark, as the author answered.
```
<!-- /rhei:plan-history task -->

Two lines for one exchange: a question whose line no longer says anything a
later task needs, and a wait whose line is the answer.

## What the person sees

The state run's console task tree has one row for the exchange
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree)),
and `program×2` counts its two invocations, the question's agent and the
look's program: the label counts them all and names only the driver of the last:

<!-- rhei:task-tree state -->
```text
   2 tasks · source order
  ✓ state.1                    completed   program×2  <t>
  ✓ state.2                    completed   agent  <t>
```
<!-- /rhei:task-tree state -->

The task run gives the wait a row of its own:

<!-- rhei:task-tree task -->
```text
   3 tasks · source order
  ✓ task.1                     completed   agent  <t>
  ✓ task.2                     completed   program  <t>
  ✓ task.3                     completed   agent  <t>
```
<!-- /rhei:task-tree task -->

## The ruling

**State**: a wait makes nothing. What it brings back is the answer to the
question its task asked, read as that task's, so it stays a state of the asking
task and the answer goes into that task's export
([§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test),
corollary 3). The answer cannot be wrong while the question was asked right:
it is whatever the author said. That is the decision table's row for *a wait
for something outside the plan*
([§FS-rhei-shape.4](../../../docs/functional-spec/rhei-shape.spec.md#4-the-decision-table)).

## When the other shape is right anyway

When the wait is not for the answer to anything its task asked — a release
window, a freeze that someone else lifts — and a later task names what the wait
established, that product is not the asking task's outcome, and corollary 1
makes it a task. A wait whose result only the next state reads stays a state
by corollary 2.

## Run it

```bash
cargo xtask examples run shape-waiting-on-a-person-state
cargo xtask examples run shape-waiting-on-a-person-task
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. Delete the `author:` line of `forge/issue-412.md` in the
copy to watch the wait look three times and give up: every look that finds no
reply keeps an `answer` saying so and exits 75, so the poll budget ends the
waiting task in `unanswered` with its export written, and the run still exits
0. The export task runs anyway, because a final state satisfies the
`**Prior:**` that names it, and reads the no-reply answer by name. `<t>` above
stands for a duration, which differs on every run.
