# A Discussion to a Ruling: Its Own Rhei, Not Children

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

The merge queue needs a policy: squash every pull request, keep its commits in
a merge commit, or something between. Two participants argue it — claude from
readable history, codex from bisect — round after round, answering each other,
until a judge rules. Nobody knows when the plan is written how many rounds
that takes. The ruling is then applied, and the work that applies it needs it
by name.

The discussion's machine is copied from
[`examples/agent-discussion`](../../agent-discussion/README.md), with two
participants instead of four and no `apply` state, since applying the ruling
is the other rhei's work. Nothing outside the plan is touched; the positions
and digests are written under the discussion's `runtime/discussion/`.

## The two shapes

**Rhei** — the discussion is a rhei of its own, and the shape is a Panta
project of two: `rhei/index.panta.md`, the default machine `rhei/states.yaml`,
and the members `ticket/` and `discussion/`. The discussion runs its own
machine, `rhei/discussion/states.yaml`, whose `collect` state fans out to both
participants and whose judge loops back for another round until it rules. The
ticket names the ruling task across rheis
([§FS-rhei-panta.3](../../../docs/functional-spec/rhei-panta.spec.md#3-one-unified-view)):

```markdown
### Task 1: Apply the merge policy
**State:** work
**Prior:** Task discussion.1
**Consumes:** discussion.1:ruling
```

**Children** — the discussion is the children of a parent that rules, in the
ticket's own plan and on its one machine, and every position and judgement is
a child authored in advance (`children/tasks/01-decide-the-merge-policy.md`):

```markdown
### Task 1: Rule on the merge policy
**Provides:** ruling

#### Task 1.1: Round 1: claude's position
#### Task 1.2: Round 1: codex's position
#### Task 1.3: Round 1: judge the round
**Prior:** Task 1.1, Task 1.2
#### Task 1.4: Round 2: claude's position
**Prior:** Task 1.3
#### Task 1.5: Round 2: codex's position
**Prior:** Task 1.3
#### Task 1.6: Round 2: judge the round
**Prior:** Task 1.4, Task 1.5

### Task 2: Apply the merge policy
**Prior:** Task 1
**Consumes:** 1:ruling
```

## What the next task sees

A task added to the ticket's rhei after the project run is told this under
`## Plan History` (`rhei next --peek`):

<!-- rhei:plan-history rhei -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task ticket.1: Apply the merge policy — completed — Applied the ruling: the merge queue rebase-merges a pull request whose every commit builds, and squashes the rest.
```
<!-- /rhei:plan-history rhei -->

The ticket's history is the ticket's work. The ruling reached it as an export
across rheis, and the rounds stay in the discussion's rhei, on its machine and
under its own execution root. The children run tells the same task:

<!-- rhei:plan-history children -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task children.1: Rule on the merge policy — completed — Ruled in round 2: rebase-merge a pull request whose every commit builds; squash one whose commits do not. — 6 subtasks: 6 completed
- Task children.2: Apply the merge policy — completed — Applied the ruling: the merge queue rebase-merges a pull request whose every commit builds, and squashes the rest.
```
<!-- /rhei:plan-history children -->

The ticket's plan now carries the discussion: a parent that speaks for six
children, every position and judgement a task of the ticket's own plan.

## What the person sees

The project run's console task tree gives the discussion one row
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree)),
where `agent×6` counts the two rounds of two positions and a judgement each,
however many rounds it took:

<!-- rhei:task-tree rhei -->
```text
   2 tasks · source order
  ✓ discussion.1               converged   agent×6  <t>
  ✓ ticket.1                   completed   agent  <t>
```
<!-- /rhei:task-tree rhei -->

The children run's tree folds eight tasks into two rows, the discussion's six
under the ticket's parent:

<!-- rhei:task-tree children -->
```text
   8 tasks · source order
  ✓ children.1                 completed   agent  <t> — 6 subtasks: 6 completed
  ✓ children.2                 completed   agent  <t>
```
<!-- /rhei:task-tree children -->

## The ruling

**Its own rhei**: a discussion with participants, rounds and a ruling is a
plan, not a state. It is the decision table's row for *a discussion with
participants, rounds and a ruling*
([§FS-rhei-shape.4](../../../docs/functional-spec/rhei-shape.spec.md#4-the-decision-table)).
A rhei is a plan of its own, with its own execution root and its own state
machine, and work becomes one when it has participants, rounds or a ruling that
the parent plan would otherwise have to model as states
([§FS-rhei-shape.1](../../../docs/functional-spec/rhei-shape.spec.md#1-the-constructs)).
Here those states are the discussion's machine: `collect` fans out to the
participants, `judge` loops back for another round until it rules, and the
ruling is the discussion's export. As a rhei, that machine and every round it
runs stay under the discussion's own execution root, while the ticket keeps its
one machine and still consumes the ruling by name
([§FS-rhei-panta.3](../../../docs/functional-spec/rhei-panta.spec.md#3-one-unified-view)).
As children, the ticket's plan carries the discussion itself, every position
and judgement a task of the ticket. The two fixed rounds are not the point: a
parent could append a round whenever its judge asks for one, as the table
allows for children whose count is unknown when the plan is authored
([§FS-rhei-shape.3.2](../../../docs/functional-spec/rhei-shape.spec.md#32-the-three-reasons-a-task-has-children)),
and the discussion would still be modelled in the ticket's plan. What makes it
a rhei is that it has participants, rounds and a ruling at all.

## When the other shape is right anyway

When the "discussion" is a fixed panel — every reviewer writes once, there are
no rounds, and the parent compares what they wrote into a report somebody
reads — the parent integrates its children, and they are subtasks
([§FS-rhei-shape.3.2](../../../docs/functional-spec/rhei-shape.spec.md#32-the-three-reasons-a-task-has-children)).

## Run it

```bash
cargo xtask examples run shape-discussion-to-a-ruling-rhei
cargo xtask examples run shape-discussion-to-a-ruling-children
```

The children shape keeps its machine in the `states.yaml` beside its
`index.rhei.md`; the project keeps its default beside `index.panta.md` and the
discussion's beside that rhei. Either copy runs with `rhei run <copy> --no-tui`
and no `--state-machine` flag. `<t>` above stands for a duration, which
differs on every run.
