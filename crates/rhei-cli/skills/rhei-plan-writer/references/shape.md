# State, task, subtask, rhei or prose

Extracted from [§FS-rhei-shape](../../../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose),
byte for byte and checked: change the rule there, never here.

## The memory test

> If what a step writes is read by anyone but the next state of the same task,
> it is a task. Otherwise it is a state.

## The three reasons a task has children

A task has children when the parent **steers** them, **integrates** them, or
**speaks for** them:

- **steers** — the parent decides what each child is, on which tier, and in
  which order, between the children rather than before them. That judgement is
  the parent's own work and nothing else can hold it [§FS-rhei-supervision.3.1](rhei-supervision.spec.md#31-the-rule).
- **integrates** — the parent's deliverable is made *out of* the children's:
  the merge, the comparison, the report that is not any one child's.
- **speaks for** — the parent is the line a later reader gets instead of the
  subtree, so the subtree's outcome must be something the parent can state
  [§FS-rhei-memory.3.2](rhei-memory.spec.md#32--plan-history).

## The decision table

| The work | Construct | Because |
|---|---|---|
| a counted `review → fix` loop where only the final code is read | **states** of one task | every round's output reaches only the next round §2 |
| a reproducer a later step consumes by name | **task** | named from outside its own state chain §2 |
| a reading several checks are each owed to | **subtasks** under the reading | the parent speaks for the verdict and the children start inside it §3.2 |
| one child per item, where the item count is unknown when the plan is authored | **subtasks** appended by the parent | the parent steers what it cannot enumerate §3.2 |
| the parts of one feature — schema, endpoint, UI | **flat tasks** with `**Prior:**` | nothing is owed above them §3.1 |
| the same parts, plus an integration somebody reads | **subtasks** under the integrator | the parent's deliverable is made out of theirs §3.2 |
| a wait for something outside the plan | **state** | a wait writes nothing anyone reads §2 |
| a lookup that must happen before a verdict | **program state** | its product is read by the verdict, in the same task §2 |
| a supervisor's own decision between two steps | **no new node** | it is the parent's work, in the parent's body and brief §3.2 |
| a discussion with participants, rounds and a ruling | **its own rhei** | participants and a ruling are a plan, not a state §1 |
| a checklist one session finishes, nothing kept between items | **body prose** | nothing is memory, so nothing needs an identity §2 |

## The paired examples

1. [`examples/shape/reproducer`](../../../../../examples/shape/reproducer/README.md)
2. [`examples/shape/review-against-spec`](../../../../../examples/shape/review-against-spec/README.md)
3. [`examples/shape/cve-category`](../../../../../examples/shape/cve-category/README.md)
4. [`examples/shape/parts-of-a-feature`](../../../../../examples/shape/parts-of-a-feature/README.md)
