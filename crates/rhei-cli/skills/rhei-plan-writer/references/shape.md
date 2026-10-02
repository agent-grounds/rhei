# State, task, subtask, rhei or prose

Extracted from [§FS-rhei-shape](../../../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose),
byte for byte and checked: change the rule there, never here.

## The memory test

> If what a step writes is read by anyone but the next state of the same task,
> it is a task. Otherwise it is a state.

Three corollaries follow from the test.

1. **A phase whose product a later step names is a task.** A reproducer another
   step consumes by name, a reading three checks are owed to, a verdict a
   supervisor quotes — each is named from outside its own state chain, so each
   is a task.
2. **A phase whose product only the next phase reads is a state.** A counted
   `review → fix → review → fix` loop where only the final code matters is one
   task with a loop of states; promoting each round to a task buys four
   identities and four result files nobody reads.
3. **A phase whose product is its own task's outcome stays a state.** The pull
   request a task opened for its change, the kind it gave an issue, the answer
   to a question it asked: a later step reads each of them, but reads it *as
   that task's*. A state writes it into the task's export, which is keyed by the
   task and never by the state that wrote it [§FS-rhei-plan-language.3.12](../../../../../docs/functional-spec/rhei-plan-language.spec.md#312-task-exports),
   or onto the forge, as the label, pull request or comment every later reader
   reads there. One question separates this from corollary 1:
   *could the product be wrong while the task's outcome is right?* A URL, a
   label or an answer cannot, so the phase stays a state. A reproducer can — a
   script that fails to reproduce what triage rightly judged real — so it is an
   outcome of its own, and corollary 1 makes it a task.

## The three reasons a task has children

A task has children when the parent **steers** them, **integrates** them, or
**speaks for** them:

- **steers** — the parent decides what each child is, on which tier, and in
  which order, between the children rather than before them. That judgement is
  the parent's own work and nothing else can hold it [§FS-rhei-supervision.3.1](../../../../../docs/functional-spec/rhei-supervision.spec.md#31-the-rule).
- **integrates** — the parent's deliverable is made *out of* the children's:
  the merge, the comparison, the report that is not any one child's.
- **speaks for** — the parent is the line a later reader gets instead of the
  subtree, so the subtree's outcome must be something the parent can state
  [§FS-rhei-memory.3.2](../../../../../docs/functional-spec/rhei-memory.spec.md#32--plan-history).

## The decision table

| The work | Construct | Because |
|---|---|---|
| a counted `review → fix` loop where only the final code is read | **states** of one task | every round's output reaches only the next round §2 |
| a reproducer, a contract or a gate's verdict a later step consumes by name | **task** | named from outside its own state chain, and an outcome of its own §2 |
| claiming the work before anything is spent on it | **task** beside the root | a person checks who holds it, and it is no other task's outcome §2 |
| a reading several checks are each owed to | **subtasks** under the reading | the parent speaks for the verdict and the children start inside it §3.2 |
| one child per item, where the item count is unknown when the plan is authored | **subtasks** appended by the parent | the parent steers what it cannot enumerate §3.2 |
| the parts of one feature — schema, endpoint, UI | **flat tasks** with `**Prior:**` | nothing is owed above them §3.1 |
| the same parts, plus an integration somebody reads | **subtasks** under the integrator | the parent's deliverable is made out of theirs §3.2 |
| a wait for something outside the plan | **state** | a wait makes nothing; an answer it brings back is the waiting task's own outcome, kept in its export §2 |
| opening the pull request for a change a task committed | **program state** of that task | the URL is the task's own outcome, kept in its export and on the forge §2 |
| classifying an issue that carries no kind | **agent state**, then **program state**, of the task that triages it | the kind is the task's own outcome, and the label keeps it §2 |
| a lookup that must happen before a verdict | **program state** | its product is read by the verdict, in the same task §2 |
| a supervisor's own decision between two steps | **no new node** | it is the parent's work, in the parent's body and brief §3.2 |
| a discussion with participants, rounds and a ruling | **its own rhei** | participants and a ruling are a plan, not a state §1 |
| a checklist one session finishes, nothing kept between items | **body prose** | nothing is memory, so nothing needs an identity §2 |

## The paired examples

1. [`examples/shape/reproducer`](../../../../../examples/shape/reproducer/README.md)
2. [`examples/shape/review-against-spec`](../../../../../examples/shape/review-against-spec/README.md)
3. [`examples/shape/cve-category`](../../../../../examples/shape/cve-category/README.md)
4. [`examples/shape/parts-of-a-feature`](../../../../../examples/shape/parts-of-a-feature/README.md)
5. [`examples/shape/spec-first`](../../../../../examples/shape/spec-first/README.md)
6. [`examples/shape/review-rounds`](../../../../../examples/shape/review-rounds/README.md)
7. [`examples/shape/claiming-the-issue`](../../../../../examples/shape/claiming-the-issue/README.md)
8. [`examples/shape/waiting-on-a-person`](../../../../../examples/shape/waiting-on-a-person/README.md)
9. [`examples/shape/candidate-lookup`](../../../../../examples/shape/candidate-lookup/README.md)
10. [`examples/shape/running-the-gate`](../../../../../examples/shape/running-the-gate/README.md)
11. [`examples/shape/draft-pull-request`](../../../../../examples/shape/draft-pull-request/README.md)
12. [`examples/shape/supervisor-decision`](../../../../../examples/shape/supervisor-decision/README.md)
