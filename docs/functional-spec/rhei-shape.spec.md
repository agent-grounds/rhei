# FS-rhei-shape: State, Task, Subtask, Rhei or Prose

One question, asked once, about every unit of work an author writes down:
**who reads what this step writes?** The answer decides which construct the
work becomes, and this page is the only place that answer is written
normatively. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

Before this page, two shipped skills answered it opposite ways for the same
work. `rhei-state-machine-writer` claimed every distinct workflow phase for a
state; `rhei-plan-writer` told the author to decompose into independently
completable tasks and to default to child tasks for progressive disclosure.
Both ship in one binary, so a review-and-fix chain authored as one task looping
`review → fix` and the same chain authored as four sibling tasks were both
correct, and which one an author produced was a matter of habit. A plan's shape
decides what every later agent is told, so a coin flip there is a coin flip
over the whole run.

The rule is not a preference. It is a consequence of where rhei already keeps
things, and §2 derives it.

## 1. The constructs

Four constructs, and the option of no construct at all.

- A **state** is a position without identity. It carries the *how* — the
  instructions, the target, the tier, the exit condition — and it has no result
  of its own, no title a reader sees, no `**Prior:**`, and no cost line. It
  cannot be listed, priced or cancelled by name. A state's declared `outputs:`
  reach exactly one reader, the next state of the same task [§FS-rhei-states.3.2](rhei-states.spec.md#32-state-handoffs).
- A **task** is a unit of work with identity. It has a title, a body, a
  deliverable, a `**State:**`, optional `**Prior:**` and exports, a result file
  at `runtime/results/<task-id>.md` [§FS-rhei-complete.3.2](rhei-complete.spec.md#32-result-file-format), a row in
  `rhei list`, a line in Plan History, and a cost of its own. Its identity
  survives a reset, a copy of its document and a relocation of it
  [§FS-rhei-budgets.1](rhei-budgets.spec.md#1-the-two-counts-and-the-days-spend).
- A **subtask** is a task whose node is written under another task's node. It is
  a task in every respect §2; what is added is the relationship, and §3 says
  when that relationship is earned.
- A **rhei** is a plan of its own, with its own execution root and its own
  state machine. Work becomes a rhei when it has participants, rounds or a
  ruling that the parent plan would otherwise have to model as states
  [§FS-rhei-panta.1](rhei-panta.spec.md#1-what-panta-is).
- **Body prose** is the fifth answer and the right one more often than it looks.
  A checklist one session finishes, in order, with nothing between the items
  worth reading later, is lines in a task body.

## 2. The memory test

Everything rhei keeps for longer than one state entry is keyed by **task**:
results, exports, briefs, `rhei list`, Plan History, the transition log, the
cost ledger [§FS-rhei-memory.2](rhei-memory.spec.md#2-the-store). Everything a state keeps reaches its own task's
next state and nothing else [§FS-rhei-states.3.2](rhei-states.spec.md#32-state-handoffs).

So **"is this memory?" and "is this a task?" are the same question**, and the
test is one line:

> If what a step writes is read by anyone but the next state of the same task,
> it is a task. Otherwise it is a state.

[§FS-rhei-plan-language.3.12](rhei-plan-language.spec.md#312-task-exports) already says this for exports — they sit outside the
state machine *because the dependency graph, not the workflow, is what orders
them*. This section is that sentence generalized from exports to the whole
shape decision.

Two corollaries the test settles without further argument.

1. **A phase whose product a later step names is a task.** A reproducer another
   step consumes by name, a reading three checks are owed to, a verdict a
   supervisor quotes — each is named from outside its own state chain, so each
   is a task.
2. **A phase whose product only the next phase reads is a state.** A counted
   `review → fix → review → fix` loop where only the final code matters is one
   task with a loop of states; promoting each round to a task buys four
   identities and four result files nobody reads.

## 3. Children

### 3.1. The default is flat

**Every task is a flat sibling, chained with a prior, unless the parent owes
something of its own.** The chain is written with `**Prior:**`. The specification's own worked example authors an
avatar column, an upload endpoint and the UI that shows them as three siblings
with `**Prior:**`, not as three children of a parent named for the feature, and
that is the shape to copy.

Progressive disclosure is **not** a reason for a child. A parent added only so
that a reader meets three items one level down adds an identity, a travel
bound §5, a result file and a prompt that has to say something, in exchange for
an indentation. `rhei list --parent <id>` and the folded lines of
[§FS-rhei-memory.3.2](rhei-memory.spec.md#32--plan-history) and [§FS-rhei-run-report.3.2](rhei-run-report.spec.md#32-task-tree) already give a reader depth on
demand.

### 3.2. The three reasons a task has children

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

A parent is a ticket, not a folder: it must have something to finish. A parent
whose body could only ever say *"the children below are done"* fails all three
and the children are siblings.

### 3.3. Promotion

Promotion runs in one direction and is checked by §2, never by how a plan
looks. Author the *how* as a state machine, the *what* as flat tasks, and add
depth only where a parent owes one of §3.2's three. Where §2 and §3.2 disagree
— what a phase writes is read from outside, but no parent owes anything — §2
wins and the result is a flat sibling.

## 4. The decision table

Each row is the test of §2 applied to a case this project has actually argued
about.

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

## 5. What promoting a phase costs

Travel is the number of applied transitions one ticket may make over the
lifetime of its identity, and it is a property of the **ticket**
[§FS-rhei-budgets.1](rhei-budgets.spec.md#1-the-two-counts-and-the-days-spend). So shape decides how many travel bounds a pipeline holds:
one pipeline authored as one task with six states holds one travel bound; the
same pipeline with three phases promoted to tasks holds three.

This section is **descriptive**. It names the coupling so that an author
promoting a phase knows what the promotion buys, and so that a reader of
[§REQ-bounded-neural-work.1](../requirements/bounded-neural-work.spec.md#1-the-five-levels) knows that level 3 bounds transitions per identity
and that this page decides how many identities there are. It says nothing about
whether buying travel that way is legitimate; §2 decides the construct, and the
travel follows.

## 6. `subtask` is a spelling, not a promise

This page states its rule over the **relationship** — a child task node under a
parent task node — and defines no new construct called a subtask. `subtask`
stays what it already is across these documents: a loose synonym for a child
task node, and one of the free `nodeKinds` spellings a plan may declare
[§FS-rhei-validate.4.4](rhei-validate.spec.md#44-workspace-task-file-metadata-diagnostics). Nothing here adds a validation rule, narrows an existing
use of the word, or makes a `subtask` heading mean anything a `Task` heading
would not.

## 7. The one normative copy

Four surfaces carry this rule and exactly one of them is edited by hand.

- **This page** is the normative copy. A change to the rule is a change here.
- **[§FS-rhei-authoring.3](rhei-authoring.spec.md#3-tasks-and-child-tasks-choosing-the-shape-then-spelling-it)** points at this page and states the flat default where
  a plan author lands. It is a usage guide, so it carries no copy of §2 or §4
  [§AR-rhei-language-reference.2](../architecture/language-reference.spec.md#2-documentation-boundary).
- **`references/shape.md`**, shipped under `rhei-plan-writer`,
  `rhei-state-machine-writer` and `rhei-template-writer`
  [§FS-rhei-install-skills](rhei-install-skills.spec.md#fs-rhei-install-skills-rhei-install-skills), is a **mechanically checked extract**: the memory
  test of §2, the three reasons of §3.2 and the table of §4, byte for byte as
  this page writes them, followed by the links to the paired examples. It is
  not authored prose and nothing is added to it that this page does not say.
- **The authoring skills** state the rule and link a runnable example of it. A
  rule stated in a skill without a link to its example is not finished.

The three installed copies of `references/shape.md` are byte-identical to each
other and their extracted block is byte-identical to this page's. That is a
gate, not a convention: three real files ship rather than one file and two
symlinks, because the skills are embedded at build time and a symlink is not
portable [§REQ-cross-platform.2](../requirements/cross-platform.md#2-parity).

## Related Specifications

- [Mid-Term Memory](rhei-memory.spec.md) — what is keyed by task, and the fold a parent's line carries
- [Plan Language Usage Guide](rhei-authoring.spec.md) — where a plan author lands
- [State Machine Writer](rhei-state-machine-writer.spec.md) — the phase rule this page qualifies
- [Budgets](rhei-budgets.spec.md) — travel, which is per identity
- [Supervision](rhei-supervision.spec.md) — the parent that steers
