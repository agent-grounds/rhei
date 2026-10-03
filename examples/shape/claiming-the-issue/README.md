# Claiming the Issue: a Task Beside the Ticket, Not Its First State

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

Issue 87 says the parser panics on an integer literal wider than 64 bits.
Before anything is spent on it, the issue is assigned to this machine, so that
no other machine or person works it too. Then the parser is fixed and the gate
runs, and the ticket speaks for both. Whoever wonders whether issue 87 is taken
looks at the claim, not at the fix.

No forge is touched: the claim program writes the assignee to
`forge/issue-87.md` in the run's copy, the stand-in for assigning the issue,
and notes beside it what it found when it ran.

## The two shapes

Both shapes have a `work` state for the agent and a `claim` state that runs a
program, and only where the claim sits differs. Each keeps its own
`states.yaml`, because the claim leads somewhere different: the sibling's claim
task ends at `completed`, and the root's claim hands over to the ticket's own
`work`.

**Sibling** — the claim is a task beside the ticket, and the ticket's work
starts after it (`sibling/tasks/01-work-issue-87.md`):

```markdown
### Task 1: Claim issue 87
**State:** claim

### Task 2: Work issue 87
**Prior:** Task 1
#### Task 2.1: Fix the parser
**Prior:** Task 1
#### Task 2.2: Run the gate
```

The first child names the claim in its own `**Prior:**`, because a parent's
Prior does not hold back its children: without that line the fix starts in the
same pass as the claim.

**Root state** — the claim is the first state of the ticket itself, and the
ticket's own `work` follows it, `claim -> work`, to speak for the fix and the
gate (`root-state/tasks/01-work-issue-87.md`):

```markdown
### Task 1: Work issue 87
**State:** claim
#### Task 1.1: Fix the parser
#### Task 1.2: Run the gate
```

The ticket is an ordinary parent, and an ordinary parent runs only once its
subtree is closed. So the claim on the root runs **last**, after the fix and
the gate it was meant to guard. That is a scheduling fact, not the rule; the
claim program finds it out by counting the results already on disk, and the
run's `forge/issue-87.md` reads `claim: Claimed issue 87 after 2 tasks had
already finished: the claim guarded nothing.`

## What the next task sees

A task added after the sibling run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history sibling -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task sibling.1: Claim issue 87 — completed — Claimed issue 87 for this machine before anything was spent on it.
- Task sibling.2: Work issue 87 — completed — Issue 87 fixed: the parser rejects the literal with E0412, and the gate is green. — 2 subtasks: 2 completed
```
<!-- /rhei:plan-history sibling -->

The claim has a line of its own, and it came first. The root-state run tells
the same task:

<!-- rhei:plan-history root-state -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task root-state.1: Work issue 87 — completed — Issue 87 fixed: the parser rejects the literal with E0412, and the gate is green. — 2 subtasks: 2 completed
```
<!-- /rhei:plan-history root-state -->

The ticket keeps its line, and that line is the fix and the gate. The claim
has no line of its own: it ran last, after both tasks had finished, and nothing
a later task is told says so. Only the stand-in issue does.

## What the person sees

The sibling run's console task tree gives the claim a row a person can check,
with its own duration
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree)):

<!-- rhei:task-tree sibling -->
```text
   4 tasks · source order
  ✓ sibling.1                  completed   program  <t>
  ✓ sibling.2                  completed   agent  <t> — 2 subtasks: 2 completed
```
<!-- /rhei:task-tree sibling -->

The root-state run has one row, the ticket's, where `agent+program` names the
ticket's own agent and the claim's program, once each. The claim has no row
and no duration of its own:

<!-- rhei:task-tree root-state -->
```text
   3 tasks · source order
  ✓ root-state.1               completed   agent+program  <t> — 2 subtasks: 2 completed
```
<!-- /rhei:task-tree root-state -->

## The ruling

**Sibling**: who holds the issue is read by someone other than the next state
of the ticket — a person checks it, and so does every other machine that might
take the issue — so the claim is a task
([§FS-rhei-shape.2](../../../docs/functional-spec/rhei-shape.spec.md#2-the-memory-test),
corollary 1). It is the decision table's row for *claiming the work before
anything is spent on it*: a **task** beside the root, because a person checks
who holds it and it is no other task's outcome
([§FS-rhei-shape.4](../../../docs/functional-spec/rhei-shape.spec.md#4-the-decision-table)).
The ticket does not steer, integrate or speak for the claim, so the claim is a
sibling, not a child
([§FS-rhei-shape.3.1](../../../docs/functional-spec/rhei-shape.spec.md#31-the-default-is-flat)).

## When the other shape is right anyway

Never for a claim. A supervising ticket, whose first visit runs before its
children, would run a claim state first — but the claim would still be read
from outside the ticket, and the assignee on the forge would still be no other
task's outcome, so it would still be a task. Corollary 3 keeps a product a
state only when it is its own task's outcome, which the claim is not.

## Run it

```bash
cargo xtask examples run shape-claiming-the-issue-sibling
cargo xtask examples run shape-claiming-the-issue-root-state
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run.
