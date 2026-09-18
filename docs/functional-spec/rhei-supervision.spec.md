# FS-rhei-supervision: Subtree Supervision Specification

This document specifies **subtree supervision**: how a non-leaf task node
looks after the tasks beneath it *while* they run, instead of only
integrating them once they are all finished. A state that declares
`execute_on:` turns the task holding it into a *supervisor*. The value names
the *scope* the supervisor watches — its direct children, or its whole subtree
— and the *event* that wakes it — a task finishing, or every transition a task
applies. The orchestrator wakes the supervisor at *checkpoints*: after every
finished child, every child transition, every finished descendant, or every
descendant transition — with
the same agent session continued from its previous visit *where the agent
supports one* (today that is `pi`; with the built-in `claude-code` profile the
supervisor runs each visit cold, carried by its checkpoints and its briefs),
and holds the rest
of the subtree while the supervisor decides how to steer it. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

Supervision builds on the non-leaf task model ([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)): a
parent is a task in its own right, and a parent and one of its descendants are
never worked at the same time. Supervision does not relax that. It adds the
one thing the model lacked — a parent that is scheduled *between* its children
rather than only after them.

This spec depends on:

- [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification) for the task hierarchy and the non-leaf eligibility rule
- [§FS-rhei-run](rhei-run.spec.md#fs-rhei-run-rhei-run) for the orchestrator loop and the ready set
- [§FS-rhei-next](rhei-next.spec.md#fs-rhei-next-rhei-next) for manual claimability
- [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification) and [§FS-rhei-transitions](rhei-transitions.spec.md#fs-rhei-transitions-rhei-transitions-specification) for the state-machine schema,
  counted loops, and conditions
- [§FS-rhei-agents](rhei-agents.spec.md#fs-rhei-agents-rhei-agents-specification) for prompt composition
- [§FS-rhei-snapshots](rhei-snapshots.spec.md#fs-rhei-snapshots-rhei-session-snapshots-specification) for the session continuity a supervisor relies on

The decision behind this shape — a barrier woken after the fact, steering
through the levers that already exist — is recorded in
[§DF-subtree-supervision](../decisions/functional/subtree-supervision.md#df-subtree-supervision-a-supervisor-is-a-barrier-over-its-subtree-woken-after-the-fact).

## Overview

Without supervision a parent task runs exactly once, after every descendant
is terminal, and sees only what its children left on disk. A review/fix chain
authored as four children runs unattended until the end. With supervision the
parent is a standing participant:

```
supervising (visit 1) → release → 1.1 review → checkpoint → supervising (visit 2) → release → 1.2 fix → …
```

Each visit continues the supervisor's own transcript
([§FS-rhei-snapshots.4.3](rhei-snapshots.spec.md#43-lineage-resolution)), reads what reached the checkpoint, and steers the
next step — by writing a *brief* the next child reads, by appending or
cancelling children, or by finishing the parent once the subtree is terminal.
The supervisor is a **barrier over its subtree**: while it is owed a visit,
nothing beneath it is dispatched; while it is running, nothing beneath it
runs.

Supervision deliberately keeps the existing machinery as the levers — plan
edits, artifacts, transitions. It adds one state field, one hold/release rule,
one condition operand, one metadata block, and two prompt sections.

A note on the word: a **supervisor** here is a *task* — a plan node in a
supervising state. The "supervisor" of [§FS-rhei-run](rhei-run.spec.md#fs-rhei-run-rhei-run) is the `rhei run` process
that owns spawned subprocesses ([§DA-supervised-process-groups](../decisions/architectural/supervised-process-groups.md#da-supervised-process-groups-subprocesses-are-supervised-process-groups-with-one-termination-path)); the two are
unrelated and never appear in the same rule.

## 1. Declaring a Supervisor

### 1.1. The `execute_on` Field

```yaml
states:
  supervising:
    execute_on: descendant-terminal   # <scope>-<event>
    target: pi:anthropic:claude-sonnet-4-5
    visits: 20
    snapshot:
      emit:    { name: supervisor, on: always }
      inherit: { name: supervisor, from: self }
    instructions: |
      You supervise Task {task_id} ...
```

The `snapshot:` block is what makes each visit continue the last one, and it is
not universally available. The built-in `pi` and `codex` profiles declare
native session layouts; the resolved target must still supply the identity
their snapshot rules require. Other profiles without a supported layout must
omit the block or provide a custom session-capable profile; explicit snapshot
use otherwise fails with `unsupported-snapshot-session`. A supervisor without
snapshot support still supervises: it runs each visit cold, carried by its
checkpoints and briefs (§6).

The value is a *scope* and an *event*, in that order, and exactly four are
legal:

| Value | A checkpoint is produced when… | The supervisor runs… |
|-------|--------------------------------|----------------------|
| `child-terminal` | a direct child enters a terminal state | after every finished child |
| `child-transition` | a direct child fires any transition, terminal ones included | after every hop of a child's own machine |
| `descendant-terminal` | any descendant, at any depth, enters a terminal state | after every finished descendant |
| `descendant-transition` | any descendant fires any transition, terminal ones included | after every hop of every descendant's own machine |

The **scope** says whose moves the supervisor hears about — `child`: its direct
children only; `descendant`: everything beneath it, at any depth. The **event**
says which of their moves those are — `terminal`: the applied transition's
effective target is `final: true`; `transition`: any applied transition,
terminal ones included.

A non-leaf child is terminal only once its own subtree is
([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)), so `child-terminal` wakes the supervisor exactly
once per finished child *subtree*: it is supervision at one level of
decomposition, with each child's internal steps left to the child — or to a
supervisor of its own, nested beneath this one. `child-transition` watches a
child's own hops — its `review ↔ fix` loop — without hearing the grandchildren
that child dispatches. Narrowing the scope needs no new way to say "the subtree
is done": because a non-leaf child cannot be terminal while anything under it
is open, `openDescendants` (§4.1) is zero exactly when no child is open, so
`openDescendants < 1` stays the edge every supervisor finishes on, whatever its
scope.

`execute_on` is a property of the *state*, not of the task: a task supervises
while it is in a supervising state and stops when it leaves one. A leaf task
in a supervising state behaves as an ordinary agent state — it has no
descendants, so it is woken once and finishes on its `openDescendants < 1`
edge (§4.1).

Omitting `execute_on` leaves the state with today's behavior: a non-leaf task
in it is worked once, after its whole subtree is terminal
([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)).

### 1.2. Validation Rules

- `execute_on`, when present, must be one of `child-terminal`,
  `child-transition`, `descendant-terminal`, or `descendant-transition`; the
  error names all four.
- A supervising state must be agent-bearing: it declares `agent`, `target`,
  `model`, or a legacy agent/model selection ([§FS-rhei-states.1.2](rhei-states.spec.md#12-per-state-fields)).
  `execute_on` on a `final: true`, `gating: true`, `program:`, or `poll:` state
  is a validation error. On a `poll:` state the error says why: a state has one
  trigger — `poll:` (time) or `execute_on:` (its subtree).
- `execute_on` combined with `all_targets` or `all_models` is a validation
  error in v1: a supervisor is one continued session, not a fanout.
- A supervising state must declare a self-loop transition
  (`from: <state>, to: <state>`). The self-loop is the *release* edge (§3.1);
  without it the supervisor would run once and never wait for its subtree.
  This mirrors the self-loop rule for polling states ([§FS-rhei-states.1.3](rhei-states.spec.md#13-validation-rules)).
- `rhei validate` warns when no transition from a supervising state that uses
  `openDescendants` (§4.1) leads to a final state the supervisor can finish in:
  the supervisor would have no way to finish. *Leads to* is a path, not a
  single edge. The walk starts at the target of the `openDescendants` edge and
  follows the edges that count as a way out of a state
  ([§FS-rhei-transitions.4.6](rhei-transitions.spec.md#46-wildcard-semantics)),
  so an edge onto a human gate that itself reaches `completed` is a way to
  finish and is not warned about, while an edge into a pocket that reaches no
  final state still is. A supervising state that declares no `openDescendants`
  transition, or only one back to itself, has no walk to start and is warned
  about as it is today.

  A final state the supervisor can finish in is any `final: true` state except
  a terminal with cancellation role
  ([§FS-rhei-states.1.4](rhei-states.spec.md#14-reserved-state-names)). This is
  the one place the question parts company with the one asked by
  [§FS-rhei-transitions.4.6](rhei-transitions.spec.md#46-wildcard-semantics),
  which is deliberately written against terminality rather than against the
  cancellation role. "Can this state be left" asks whether the engine has anywhere
  to take the task, and abandonment is somewhere; "can this supervisor finish"
  asks whether the work it supervises can ever be declared done, and
  abandonment is not. So an `openDescendants` edge whose every path ends only
  in `cancelled`, the direct edge included, is warned about, where today it is
  silent.

  `rhei run` prints that warning at start
  ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)), and if the run reaches
  the state the warning describes, the halt names the repair: an
  `openDescendants` edge that reaches a final state, whether it points at one
  directly or at a state that leads to one. Running the whole subtree and then
  reporting "stalled in non-terminal state" is still the one outcome this
  machine must not produce.
- `visits` on a supervising state is allowed and budgets the number of
  supervisor visits; the usual exhaustion rules apply
  ([§FS-rhei-transitions.4.3](rhei-transitions.spec.md#43-counted-loops)). `rhei validate` warns when a supervising state
  declares neither `visits` nor an exhaustion edge.
- A supervising state that should continue its own conversation between visits
  uses `session: continue` ([§FS-rhei-snapshots.4.7](rhei-snapshots.spec.md#47-state-local-continuation)). It remains valid when the resolved profile cannot preload;
  those visits run cold with a reasoned diagnostic. The field's ordinary
  state-shape and target-resolution validation still applies.

## 2. Checkpoints

### 2.1. Checkpoint Events

A *checkpoint event* is produced on the shared transition path
([§FS-rhei-transition-cmd.3.1](rhei-transition-cmd.spec.md#31-descendants-first-on-terminal-entry)) — by `rhei run`, `rhei transition`,
`rhei complete`, or a callback redirect alike — when a transition is applied
to a task that has a supervising ancestor:

- under a `*-terminal` value, when the applied transition's effective target is
  `final: true`;
- under a `*-transition` value, on every applied transition, terminal ones
  included.

The event decides *which* moves are news; the scope decides *whose* (§2.2).

A polling state's self-loop attempt ([§FS-rhei-states.2](rhei-states.spec.md#2-polling-states)) is a retry, not
progress, and never produces a checkpoint. A fanout state's per-invocation
exits are not transitions; the one transition selected once every invocation
has landed is ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)).

A transition applied to a descendant while its nearest supervisor is itself
in flight — a cancel the supervisor issues during its own visit (§5.1) — is
not a checkpoint: the supervisor already knows. The shared path recognizes
that visit from the supervisor's `**Assignee:**` claim or from execution
context scoped to an explicit operation on the outer plan — which on the
command line is `rhei transition --supervisor <TASK_ID>`, naming the supervisor
the move is issued by ([§FS-rhei-transition-cmd.2](rhei-transition-cmd.spec.md#2-options)).
That context names
the active supervisor only while its command is applying a transition to the
same plan; it is not an ambient task-id variable inherited by the agent or its
children ([§FS-rhei-agents.4](rhei-agents.spec.md#4-environment-variables)). A nested independent Rhei therefore has no authority over the outer
supervision tree, while a supervisor can still move or cancel a held
descendant without checkpointing itself. A descendant's own exit remains a
checkpoint as usual.

### 2.2. Nearest In-Scope Supervising Ancestor

A checkpoint event is delivered to exactly one task: the **nearest** ancestor
of the transitioning task that is currently in a supervising state **whose
scope includes that task**. A `child-*` supervisor's scope includes only its
own children, so it declines a grandchild's move; the event then climbs to the
next ancestor up whose scope includes the task — a `descendant-*` supervisor,
however far up — or to nobody, in which case the transition is ordinary and
nothing is held. Ancestors above the one that takes it see nothing of it; what
they see is that supervisor's own transitions, per their own `execute_on`.

Scope is the only filter that climbs. An in-scope supervisor whose *event* does
not match — a `child-terminal` supervisor over a child that merely hopped — is
still the one this move belongs to: it simply produces no checkpoint, and the
move is not offered to anyone above.

A supervisor's **self-loop** exit is never a checkpoint for its ancestors — it
is the supervisor waiting, not the subtree progressing. Every other transition
of a supervisor is an ordinary transition of an ordinary descendant: its
terminal exit is a `*-terminal` event for the next supervisor whose scope
reaches it, and its exit into any other state is a `*-transition` one.

### 2.3. Timing

Checkpoints are **post-transition**. The descendant has already moved to its
new state and is held there (§3); the supervisor judges what happened and
steers what comes next. Vetoing or redirecting a transition before it is
applied remains the job of `on_leave` callbacks ([§FS-rhei-transitions.3.2](rhei-transitions.spec.md#32-callback-trigger-triggeredby-callback));
supervision does not add a second approval path.

## 3. Hold and Release

### 3.1. The Rule

A task `P` in a supervising state is a barrier over its subtree. Its
*supervision phase* is either **held** or **released** (§3.3):

1. **Entry holds.** When `P` enters a supervising state — by any verb, or by
   being authored in it — the phase is `held`: no descendant of `P` is
   dispatched by `rhei run` or claimable by `rhei next`. `P` itself is ready.
2. **The self-loop releases.** When `P` exits the supervising state by its
   self-loop, the phase becomes `released`: descendants of `P` are eligible
   under the ordinary rules — `**Prior:**`, `inputs:`, gating, polling,
   `concurrent`, `--parallel` — exactly as if `P` had no supervising state.
   The visit is over, so the self-loop also drops `P`'s `**Assignee:**`: the
   worker that took the visit no longer holds the ticket, and `P` is claimed
   afresh by whoever answers its next checkpoint (§3.4).
3. **A checkpoint holds again.** When a checkpoint event is delivered to `P`
   (§2), the phase becomes `held` and the event is recorded (§3.3). Nothing
   new beneath `P` is dispatched. Descendants already in flight run to their
   exit, and their exits may deliver further checkpoints, which accumulate.
   Once no descendant of `P` is in flight, `P` is ready.
4. **Leaving releases, except into a human gate; finishing is guarded.** When
   `P` exits the supervising state by any edge other than its self-loop, the
   phase is cleared and its descendants follow the ordinary rules — *unless*
   the target state is `gating: true`, in which case the block is **kept** with
   phase `held`. The `supervision` block, not the state, is the hold (§3.2):
   the one edge a supervisor takes without deciding anything is its exhaustion
   edge, and letting that silently un-supervise the subtree is the opposite of
   what a budget is for. A supervisor parked at a gate therefore keeps its
   subtree held until a human moves it — back into a supervising state, where
   entry holds as usual, or anywhere else, which releases. `rhei run` says so
   at that transition, and the run report gives the parked ticket a row of its
   own naming the subtree it still holds ([§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout)). An exit
   into a `final: true` state is subject to the descendants-first guard like
   any other terminal entry ([§FS-rhei-transition-cmd.3.1](rhei-transition-cmd.spec.md#31-descendants-first-on-terminal-entry)); a machine expresses
   "finish once the subtree is done" with `openDescendants < 1` (§4.1).

Two invariants follow, and they are the point:

- **A supervisor and its descendants are never worked at the same time.** `P`
  is ready only while nothing beneath it is in flight, and nothing beneath it
  is dispatched while `P` is owed a visit or running. This is the property the
  non-leaf model already guarantees for an unsupervised parent
  ([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)), extended to a parent that runs many times.
- **A supervisor that changes nothing changes nothing.** Its self-loop
  releases the subtree and the subtree proceeds — *when there is a subtree that
  can proceed*. A visit that neither moved the subtree nor left it able to move
  releases nothing, and its self-loop is withheld (§3.6). Supervision never
  spins: `P` is not ready again until a descendant produces a checkpoint, and a
  withheld self-loop re-spawns `P` only within the attempt budget of the visit
  it did not spend.

Under `--parallel`, rule 3 is a drain: siblings already running finish, no new
ones start, and the supervisor sees every checkpoint they produced in one
visit ([§FS-rhei-run.5](rhei-run.spec.md#5-parallel-execution)). A subtree that shares a supervisor therefore
serializes at each checkpoint; a `*-transition` value serializes it at every hop
and costs one supervisor invocation per hop, which is the trade an author makes
by choosing it.

Scope narrows what *wakes* a supervisor, never what it is responsible for. A
`child-*` supervisor is the barrier over its whole subtree exactly as a
`descendant-*` one is: it is never worked concurrently with any descendant, and
a checkpoint drains and holds everything beneath it, grandchildren included.
Between its visits the descendants it does not hear about run freely — that is
the whole difference.

### 3.2. Readiness

The ready set of `rhei run` ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)) and the claimability rule of
`rhei next` ([§FS-rhei-next.3](rhei-next.spec.md#3-default-behavior-claim-mode)) gain one rule each, replacing the "every
descendant is terminal" requirement for the tasks it concerns:

- A task in a supervising state is ready when its phase is `held` and no
  descendant of it is in flight. The "every descendant is terminal" condition
  does not apply to it.
- A task with one or more supervising ancestors is ready only when **every**
  supervising ancestor's phase is `released`, in addition to the ordinary
  rules. A supervising ancestor that is `held`, or in flight, holds the whole
  subtree beneath it, nested supervisors included. *Ancestor* here means any
  ancestor carrying a `supervision` block with phase `held` — the block is the
  hold, whatever state the task is now in (§3.1 rule 4) — plus a task in a
  supervising state with no block at all, the authored-initial case.

A task is *in flight* when a run has spawned it and it has not exited, or when
it carries `**Assignee:**` — the manual worker's claim. Every other task keeps
today's rule unchanged.

### 3.3. Supervision Metadata

The phase and the pending checkpoints are runtime-maintained task metadata in
plan frontmatter, beside `stateVisits` ([§FS-rhei-transitions.2.2](rhei-transitions.spec.md#22-metadata-storage-example)):

```yaml
metadata:
  tasks:
    1:
      stateVisits:
        supervising: 3
      supervision:
        phase: held                 # held | released
        checkpoints:
          - task: "1.2"
            from: review
            to: fix
            visit: 2
```

- `supervision.phase` is written on the shared transition path: `held` on
  entry into a supervising state, on every delivered checkpoint, and on an exit
  into a `gating: true` state (§3.1 rule 4); `released` on the self-loop exit.
  A task in a supervising state with no
  `supervision` block is `held` — the authored-initial-state case.
- `supervision.checkpoints` accumulates delivered events in delivery order.
  `task` is the rhei-local id of the transitioning descendant, `from` and `to`
  its bare state names, `visit` the `to` state's visit number. The list is
  cleared on the self-loop exit, after the visit that consumed it.
- The block is removed when the task leaves the supervising state by any
  other edge, when a task that is *not* in a supervising state but still
  carries a block moves at all — the human moving a gate-parked supervisor on —
  and by `rhei reset` together with `stateVisits`, which also drops
  a `metadata.tasks.<id>` entry left empty by the two ([§FS-rhei-reset](rhei-reset.spec.md#fs-rhei-reset-rhei-reset)).

Nothing here is authored by hand in normal workflows. The block exists so
that a run stopped between a checkpoint and the supervisor's visit resumes
exactly where it was, and so that a manual worker (§3.4) sees the same state
`rhei run` would.

### 3.4. Manual Workers

Supervision is defined on the shared path, so the manual-worker loop sees it
without special casing:

- `rhei next` claims a held supervisor under §3.2 and renders its prompt with
  the checkpoints (§5.1). Claim mode and `--peek` render the same two
  supervision sections `rhei run` composes, from the same renderers and in the
  same order relative to the instructions: `## Checkpoints` for a ticket in a
  supervising state that is owed any, and `## Supervisor Brief` for a ticket a
  supervising ancestor wrote one for (§5.2). A ticket in a supervising state
  also gets `## Supervising This Subtree`, which carries what `rhei run` puts in
  `## Rhei Commands` and `## Result` — sections `rhei next` does not render: the
  brief paths (§5.1), the barrier sentence, the qualified `## Result` rule, and
  the one command that ends the visit: the state's own self-loop, spelled out
  with this invocation's plan path and machine, because that edge is what
  releases the subtree and drops the claim. Under `--json` each is a field,
  present only when the section is. A plan with no supervising state produces
  the output it always did. It never claims a descendant of a held supervisor;
  such descendants are reported as `Task <id> held by supervisor Task <P>
  (<state>)`, a reason row of its own beside the prerequisite row
  ([§FS-rhei-next.3.4](rhei-next.spec.md#34-claiming-a-non-leaf-ticket-with---task)). That row ends in the next step, because a held ticket is
  not a stall but someone else's turn: it names the supervisor as the ticket to
  work and gives the command that claims it, or — when a worker already holds
  that visit — names the holder and the `rhei release` that hands it back, or —
  when the supervisor is parked at a human gate (§3.1 rule 4) — names the
  transition a human takes to release the subtree. The same three answers reach
  `rhei next --task <descendant>`, whose refusal is the same fact asked about
  one ticket. The
  supervisor is in no other category the diagnosis reports: its own subtree is
  open, so the "workable" set that feeds them excludes it, and a row that
  stopped at "everything is held" would leave the worker with nowhere to go.
- The worker releases the subtree with the self-loop,
  `rhei transition <P> --from <state> --to <state>`, and finishes it with the
  terminal edge once `openDescendants` is `0`. That self-loop ends the visit
  and with it the worker's claim: `rhei transition` drops `P`'s
  `**Assignee:**` on this one edge, the way a terminal entry does
  ([§FS-rhei-transition-cmd.3](rhei-transition-cmd.spec.md#3-behavior)). A claim that outlived the visit would be read
  as "the supervisor is working right now" — every later descendant exit
  would be taken for the supervisor's own doing and deliver no checkpoint
  (§2.1), and `P` itself would never be scheduled again.
- `rhei run --dry-run` names the barrier per pass — `N ticket(s) held by
  supervisor Task <P>`, one line per supervisor — and renders the release
  self-loop as `<state> -> <state> (release)`. A dry run is what an author reads
  to learn what a machine will do, and unannotated the barrier is invisible in
  it: most of the plan simply never appears as ready, and the one edge that
  decides whether it ever does reads as a no-op.
- `rhei list --ready` excludes a held descendant, by the same rule the ready
  set applies (§3.2), so the listing never offers a ticket `rhei run` would
  refuse to schedule; and the run report names the reason on the ticket it
  halted on, under a **Waiting** group rather than Attention — a held ticket is
  someone else's turn, not a human's, and it is counted in no halt tally
  ([§FS-rhei-list](rhei-list.spec.md#fs-rhei-list-rhei-list), [§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout)). The plain `rhei list` listing carries no
  held reason: it has no readiness-reason column for anything today, and adding
  one is a follow-up alongside the same reason in the TUI and the Flow
  dashboard ([§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization)).

### 3.5. Failed Visits

A visit's subprocess exiting non-zero fires no transition unless the state
declares a matching `exit_code` rule (§FS-rhei-programs.3.2), and a
supervising state never does — `exit_code` requires `program:`
(§FS-rhei-programs.3.1), and a supervisor is an agent state. So a failed
visit is not progress: `--continue-on-error` governs it
(§FS-rhei-agents.5.2.1 step 12) and the release self-loop does not fire.
`supervision.phase` stays `held`, `supervision.checkpoints` keeps every entry
delivered before the failure, and `stateVisits.<state>` is not incremented —
the visit is not spent. The task is still ready under §3.2, so a later pass,
or a rerun of `rhei run`, spawns the same visit again with the same pending
checkpoints (§5.1).

### 3.6. Empty Visits

A visit that exits `0` and meets its completion condition has still to have
*done* something before its self-loop may fire. The self-loop is the release
edge (§3.1 rule 2), and the release is the one thing a run cannot take back:
`P` is `released` until a descendant delivers a checkpoint, and a subtree that
cannot move delivers none. A visit that neither moved the subtree nor left it
able to move would therefore spend the only edge that could ever wake `P`
again — and the run would not be stalled but beyond the reach of a rerun, with
`rhei reset` the only remedy.

So the self-loop fires only for a visit that **released** something. A visit
released something when at least one of these holds, judged against the plan
as re-read after the subprocess exits (§4.1):

1. **There is no subtree to release.** `P` has no non-terminal descendant.
   Nothing is being held, and finishing `P` is the machine's own business —
   its `openDescendants` edge (§4.1), or the halt that names the missing one
   (§1.2).
2. **The visit moved the subtree.** Some descendant's state differs from what
   it was when the visit was spawned, or the visit appended or removed one. A
   cancel, a hand `rhei transition`, an appended-and-moved step: the supervisor
   steered, and the engine does not second-guess how.
3. **The subtree can still move.** Some non-terminal descendant can, once the
   barrier lifts, be scheduled without `P`. A `gating: true` descendant
   answers this on its own, because a human owns its next move and makes it
   with `rhei transition`, which reads no `inputs:` at all
   ([§FS-rhei-transition-cmd.3](rhei-transition-cmd.spec.md#3-behavior)).
   Every other descendant answers it when **both** hold: its `**Prior:**`s do
   not block it — an unsatisfied prior naming a non-terminal task **outside**
   `P`'s subtree is other work's to finish and does not block — **and** its
   state's declared `inputs:` exist (§3.2, reading `P` as released).

   A `poll:` state is no clause of its own. Time and other work decide *when*
   a descendant is scheduled and neither puts a file on disk, so a poll that
   keeps firing and a prior that eventually lands both leave a descendant
   waiting on a brief only `P` writes exactly where it was. The clock says
   when; the same priors-and-inputs test says whether.

When none of them holds the visit is **empty**, and it is treated exactly as a
failed visit (§3.5): **no transition fires**, and the plan file is not
rewritten. `supervision.phase` stays `held`, `supervision.checkpoints` keeps
every entry delivered before the visit, `stateVisits.<state>` is not
incremented — the visit is not spent — and no other edit the release self-loop
makes happens either. Not its `**Assignee:**` drop (§3.4), though: a claimed
ticket anywhere in `P`'s subtree stops the visit being scheduled at all (§3.2),
so no visit judged by this rule has one to drop.
The task is still ready under
§3.2, so a later pass, or a rerun of `rhei run`, spawns the same visit again.
The engine warns where it withheld, naming `P`, its state, every descendant it
left with nowhere to go **with the reason that one is stuck** — the file it is
waiting for, or the `**Prior:**` that has not landed — and the one action that
answers it. It records the ticket as
stalled for the run report exactly as an unmet completion condition does
([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop) step 5).

An empty visit does **not** spend the visit's attempt budget
([§FS-rhei-agents.3.2.3](rhei-agents.spec.md#323-attempt-budget)). The budget bounds a worker that could not answer for
itself, and this worker did: it exited `0` and met its completion condition,
and the engine took the edge away. So the attempt it was charged is given
back when the edge is withheld, and every later `rhei run` visits `P` again —
the §3.5 rerun shape, with no ceiling that a run can leave behind it.

That ceiling would be the strand in another form. A held `P` still bars its
whole subtree (§3.2), so a budget that ran out would leave `P` unspawnable and
its descendants barred: writing the very file the warning named would start
nothing, and `rhei reset` would again be the only remedy. A halt has to be a
thing an operator can answer, and the answer here has to keep working.

What bounds the re-spawning instead is the rule that bounds every stalled
ticket: the empty visit puts `P` out of this run's running
([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop) step 5), and the run picks it up again only after something
else advanced. A subtree that cannot move produces no such advance, so a run
that has nothing else to do visits `P` exactly once and halts. The invariant of
§3.1 holds: within one run, `P` is re-visited only when the world it supervises
changed.

A supervisor that is *already* `released` over a subtree where nothing can
move — a workspace stranded before this rule existed, or one whose supervisor
steered its subtree into a corner under rule 2 — gets a halt row of its own:
it released its subtree on a visit that changed nothing and nothing beneath it
can move, and the next action is to unblock one of the descendants the row
names, or `rhei reset`. It is not "not scheduled … rerun to pick it up": a
released supervisor is woken by a descendant checkpoint and by nothing else,
so a rerun is the one remedy that provably does nothing
([§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout)).

The rule guards the release edge, so it is asked wherever `rhei run` fires that
edge in agent mode. That is the sequential and parallel completion paths after
a visit — and also the advance that spawns nothing at all, because the state's
declared `outputs:` are already on disk and there is no invocation left to run
([§FS-rhei-agents.3.2](rhei-agents.spec.md#32-completion-condition)). A held
visit is exactly what leaves those outputs behind, so a rule that skipped that
advance would hand the strand back on the very next command. Rules 1 and 3
answer there as they always do; rule 2 is not asked, because no subprocess ran
and nothing could have moved while it did — and nothing was charged, so there
is no attempt to give back. A dry run asks the same question at that advance
and reports the edge as withheld, because a run that will not take an edge must
not say it would ([§FS-rhei-run.4](rhei-run.spec.md#4-dry-run)).

What is exempt is what has no visit to judge, not what carries a flag: with
`--no-agent` the run intends no visit at any state and applies §3.1 unchanged,
and a supervising state is never a `program:` state (§1.2). A conditioned exit
— `visitCount >= visits`, `openDescendants < 1` — is untouched: the rule
reaches only the edge that releases, which is the self-loop.

## 4. Transition Support

### 4.1. The `openDescendants` Operand

Transition `condition` expressions gain the integer operand
`openDescendants`: the number of descendants of the transitioning task —
child, grandchild, or deeper — whose state is not terminal. A leaf's
`openDescendants` is `0`.

It is evaluated against the plan as **re-read after the subprocess exits**, so
children a supervisor appended during its visit count, and children it
cancelled do not. The operand is available on transitions from any state, not
only supervising ones; it is the operand that lets a machine *select* a
terminal edge for a parent. The descendants-first guard still decides whether
that edge may be taken ([§FS-rhei-transition-cmd.3.1](rhei-transition-cmd.spec.md#31-descendants-first-on-terminal-entry)) — `openDescendants` is
how a machine agrees with the guard, never a way around it.

The canonical supervisor edges are:

```yaml
transitions:
  - from: supervising
    to: human-review
    description: Supervisor budget exhausted; a human decides
    condition: visitCount >= visits
  - from: supervising
    to: completed
    description: Every child is terminal and the supervisor wrote its result
    condition: openDescendants < 1
  - from: supervising
    to: supervising
    description: Released the subtree; wait for the next checkpoint
```

Transitions are tried in declaration order ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)), so the
exhaustion edge comes first, the terminal edge second, and the unconditional
self-loop last.

A supervisor may also finish *through* a gate. Here the `openDescendants` edge
selects a `gating: true` state and a human takes the last hop with
`rhei transition`:

```yaml
transitions:
  - from: moderating
    to: human-judgment
    description: Every child is terminal; a human rules on the verdict
    condition: openDescendants < 1
  - from: moderating
    to: moderating
    description: Released the subtree; wait for the next checkpoint
  - from: human-judgment
    to: completed
    description: The ruling is recorded
```

This shape finishes, so §1.2 does not warn about it: the walk from
`human-judgment`, the target of the `openDescendants` edge, reaches
`completed`. The three shapes above are one-hop only because they are the
simplest, not because a supervisor's finishing edge must point at a final
state.

### 4.2. Self-Loops on Agent States

A transition whose `from` equals its `to`, on a state that does not declare
`poll:`, is a **loop-back re-entry**: it increments `stateVisits.<state>` like
any other re-entry ([§FS-rhei-transitions.4.3](rhei-transitions.spec.md#43-counted-loops)), is bounded by `visits` when
declared, emits and inherits snapshots per visit ([§FS-rhei-snapshots.10.3](rhei-snapshots.spec.md#103-counted-loops-fanout-and-polling)),
and re-evaluates the state's `inputs:` on re-entry. The engine selects it on
the same rules as any other edge. On a supervising state it is the release
edge (§3.1), withheld from a visit that released nothing — which is the one
case where a selected self-loop does not fire and `stateVisits.<state>` does
not move (§3.6); on any other agent state it simply means "run this state
again".

The counter is therefore kept for **every** non-poll state the machine
declares a self-loop from, whether or not that state caps itself with
`visits:` — a task that merely enters such a state gets `stateVisits.<state>:
1` — because `visitCount` is what the loop's own exit condition reads. Without
the counter a `condition: visitCount >= N` exit compares against `0` forever
and the loop never ends. Counting does not change how the state is spelled:
`**State:**` takes its `-<n>` suffix only when `visits:` is declared
([§FS-rhei-transitions.2.3](rhei-transitions.spec.md#23-counted-loop-metadata)), so a machine that adds no budget sees no change in
its plan files.

`rhei validate` warns when a non-poll state with a self-loop declares neither
`visits:` nor a transition bounded by `visitCount`: nothing terminates that
loop ([§FS-rhei-states.1.3](rhei-states.spec.md#13-validation-rules)).

This generalizes the self-loop, previously specified only for polling states
([§FS-rhei-states.2](rhei-states.spec.md#2-polling-states)). The poll interpretation — release the slot, retry after
`interval` — is unchanged and applies only when the state declares `poll:`.

## 5. Prompt Composition

### 5.1. The Supervisor's Prompt

An invocation of a supervising state renders two sections after
`## Task Content` ([§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition)):

```
## Child Tasks

- Task plan.1.1: Review parser [completed]
- Task plan.1.2: Fix findings [fix]
- Task plan.1.3: Re-review [review]

## Checkpoints

These are the descendants that moved since your last visit, in order. Each
carries what that step left behind.

### Task plan.1.2: Fix findings — review → fix (visit 2)

    ```markdown
    {for a terminal `to`: the descendant's result, `runtime/results/<id>.md`;
     otherwise: every declared, existing, non-empty `outputs:` artifact of the
     `from` state, each under its artifact name}
    ```
```

Every pasted body is **fenced**. A result file opens with `## Result`, a heading
that outranks the `### Task …` heading it is pasted under, so unfenced it would
turn everything after it into a new top-level section of the prompt. The fence
is as long as it needs to be: a body that already contains a run of backticks
gets a longer one. `## Child Task Results` fences its bodies for the same
reason, and so do `## Prior Task Results`, `## Consumed Exports`, and the
handoff sections ([§FS-rhei-memory.4.5](rhei-memory.spec.md#45-fencing-and-rendering)).

`## Child Tasks` is the map and is rendered on every visit. `## Checkpoints`
renders the recorded `supervision.checkpoints` (§3.3) and is omitted on a
visit with none — the first visit, where the supervisor's job is to brief the
first step.

Both sections spell a descendant the one way the supervisor can act on it: the
**qualified** id, the same one `rhei transition` accepts. `supervision.checkpoints`
stores the rhei-local id (§3.3), and the renderer resolves it to exactly one
descendant — the recorded id under the supervisor's own qualification — never
to a deeper node whose id happens to end the same way.

`## Result` ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)) is **qualified** on a supervising state. The
unqualified sentence is true of the last visit and misleading on every earlier
one, and a supervisor without session continuity reads it cold each time: it
says instead that a transition from this state can finish the task *once its
subtree is closed*, and that the result is written only on the visit where every
descendant is terminal and the supervisor intends to finish — otherwise the
supervisor returns without one and is woken at the next checkpoint.

A non-leaf task in a state that is *not* supervising renders `## Child Tasks`
as today and, new, `## Child Task Results` — the result of every terminal
child, in plan order — so an unsupervised parent integrating its subtree also
sees what the subtree produced.

The `## Rhei Commands` section of a supervisor's prompt names the lever the
supervisor steers with before the ones it destroys with: one sentence giving
both brief paths (§5.2) with the execution root resolved to an absolute path,
because the supervisor's working directory is not something the prompt can
promise. It says in one clause which moves bring this supervisor back — the
state's `execute_on` in words: *woken after every finished child*, *after every
transition one of your children makes*, *after every finished descendant*, or
*after every transition any descendant makes* (§1.1) — because an agent that
does not know what wakes it cannot tell waiting from being finished with a
step, and it does not read the machine. It also states the barrier in one
sentence — while this invocation
runs nothing beneath it runs, and when the invocation ends the subtree is
released (§3.1) — because everything a supervisor is tempted to do wrong
follows from not knowing it: waiting for a child that cannot start, or treating
this visit as the last one. It additionally states
that the agent **may** run `rhei transition` against *held descendants* — to
cancel a step the checkpoint made unnecessary, typically, passing
`--result "<why>"` because a cancelled ticket still has to say why (§6) and
`--supervisor <this task's id>` because the move is this supervisor's own — and
may append descendants under its own task in its task file, as any agent
editing its own file may. The prompt says what that flag is for rather than
only that it is passed: it does not permit the move, which the
dispatch-and-claim hold already allows (§3.1), it keeps the move from returning
as a checkpoint (§2.1) and waking the supervisor with news it already has. A
supervisor told only to type the flag reads it as the permission, and a
supervisor is the reader most likely to write that reading down somewhere
else. It still must not transition its own task: the orchestrator owns
that edge. A transition the supervisor applies to a descendant is an external
plan change the orchestrator respects on re-read ([§FS-rhei-agents.5.2](rhei-agents.spec.md#52-execution-loop)).

### 5.2. The Brief

A supervisor steers a descendant by writing a **brief**, a Markdown file at
one of two reserved paths under the execution root:

- `runtime/supervise/<task-id>.md` — read by every state of that descendant;
- `runtime/supervise/<task-id>/<state>.md` — read by one state only.

When the descendant is invoked, each brief that exists and is non-empty is
rendered, task-level first, under:

```
## Supervisor Brief

These are directions from the supervising Task {supervisor-id}. Follow them
within this state's instructions and artifact contract: a brief may narrow or
direct the work, but it cannot waive a required output or choose the
transition.
```

The brief is how a `review → fix` chain carries the review's verdict into the
fix and the supervisor's judgement into both. It is an ordinary artifact: a
machine may additionally declare it as an `inputs:` entry, optional or
required ([§FS-rhei-states.3.1](rhei-states.spec.md#31-optional-inputs)), to gate a state on its presence, but no
declaration is needed for it to be rendered. Briefs are not cleared by the
engine; a supervisor that wants a fresh one overwrites it.

## 6. Interaction With Other Features

- **Snapshots.** A supervising state should use `session: continue` so each
  visit preloads its immediately preceding auto snapshot when supported and
  otherwise runs cold ([§FS-rhei-snapshots.4.7](rhei-snapshots.spec.md#47-state-local-continuation)). It should not duplicate that transcript with a named
  self-emit/inherit pair. A descendant may still branch from a separately
  named supervisor transcript with `from: ancestor` ([§FS-rhei-snapshots.6](rhei-snapshots.spec.md#6-sub-task-inheritance)) when a step should
  start inside the supervisor's context. Agents without session support run
  each visit cold ([§FS-rhei-snapshots.9.3](rhei-snapshots.spec.md#93-per-agent-runtime-behavior)); supervision still works, carried
  by the checkpoints and the briefs. `from: prior` and task `**Inherits:**`
  are explicit capabilities, not lifecycle defaults: review and short iterative
  green work remain cold unless an author deliberately opts a task in, and no
  template promises savings without measuring total cache-read and write cost.
- **Counted loops.** Each supervisor visit is a visit of the supervising
  state. `visits` budgets them; the exhaustion edge is the safety valve for a
  subtree that never converges.
- **Gating descendants.** A descendant at a human gate is not in flight, so it
  does not keep the subtree from being quiescent. Under a `*-transition` value
  its entry into the gate is a checkpoint — the supervisor can cancel the step
  or leave it to the human; under a `*-terminal` one it is not, and the subtree
  waits on the human as it would unsupervised. A gate below a `child-*`
  supervisor's own children is invisible to it either way.
- **Polling descendants.** Poll self-loops are not checkpoints (§2.1); a
  polling descendant between attempts is not in flight.
- **Appending and cancelling.** The supervisor appends descendants by editing
  its task file and cancels them with `rhei transition`, both during its
  visit; the orchestrator re-reads the plan when the visit ends, so
  `openDescendants` (§4.1) and the released subtree reflect the edits. A cancel
  does not have to satisfy the cancelled step's own `outputs:` — cancellation
  abandons the work, so that contract is moot ([§FS-rhei-transitions.4.5](rhei-transitions.spec.md#45-artifact-enforcement)) — but
  it does have to say why: the terminal-result obligation stands, so every
  cancel carries `--result "<why>"`. The waiver keys on the effective cancellation role
  ([§FS-rhei-states.1.4](rhei-states.spec.md#14-reserved-state-names)): an abandonment terminal with another name declares `role: cancellation`;
  legacy `cancelled`/`canceled` spellings keep the same waiver.
- **Reset.** `rhei reset` on a supervisor clears its `supervision` block;
  resetting a descendant does not touch the supervisor's phase.
- **Resumed runs.** A checkpoint reports a transition, not that work was done.
  A run resumed after an interruption can advance a ticket on artifacts its
  killed worker had already written — the completion condition is satisfied, so
  the step is not redone — and the supervisor is then checkpointed on a step
  nobody finished. That is generic resume behaviour ([§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership)) which
  supervision makes visible rather than causes; it is a roadmap item, and until
  it is settled a supervisor should read a checkpoint as "this ticket moved",
  not as "this work happened".
- **Fanout.** Not supported on a supervising state in v1 (§1.2).
  Descendants may fan out freely; their merged transition is the checkpoint.

## 7. Example

A pre-authored review/fix chain, supervised after every finished descendant:

```markdown
### Task 1: Harden the parser
**State:** supervising

Goal and acceptance criteria for the whole change.

#### Task 1.1: Review parser
**State:** review
#### Task 1.2: Fix findings
**State:** fix
**Prior:** Task 1.1
#### Task 1.3: Re-review
**State:** review
**Prior:** Task 1.2
#### Task 1.4: Fix remaining
**State:** fix
**Prior:** Task 1.3
```

```yaml
name: harden-the-parser
version: 1

states:
  supervising:
    initial: true
    execute_on: descendant-terminal
    target: pi:anthropic:claude-sonnet-4-5
    visits: 12
    snapshot:
      emit:    { name: supervisor, on: always }
      inherit: { name: supervisor, from: self }
    instructions: |
      You supervise Task {task_id}. Judge every checkpoint below. Brief the
      next step at runtime/supervise/<child-id>.md, append a child if the plan
      needs one, cancel a child the results made unnecessary. When every child
      is terminal, write your result and finish.
  review:
    agent: claude-code
    outputs:
      - name: findings
        path: runtime/review/{task_id}.md
    instructions: Review as briefed; write findings to {output.findings.path}.
  fix:
    agent: claude-code
    instructions: Apply exactly the fixes the brief asks for.
  human-review:
    gating: true
  completed:
    final: true
  cancelled:
    final: true

transitions:
  - { from: supervising, to: human-review, description: Budget exhausted, condition: visitCount >= visits }
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released; wait for the next checkpoint }
  - { from: review, to: completed, description: Findings written }
  - { from: fix, to: completed, description: Fixes applied }
  - { from: "*", to: cancelled, description: Dropped }
```

The run then proceeds:

```
pass 1  Task 1 held, nothing in flight  → supervising v1: briefs 1.1 → self-loop (released)
pass 2  1.1 ready → review runs → completed: checkpoint → Task 1 held
pass 3  supervising v2 (inherits v1): reads 1.1's result, briefs 1.2 → released
pass 4  1.2 fix runs → completed: checkpoint
pass 5  supervising v3: if 1.1 found nothing, cancels 1.3 and 1.4 → released
        …
last    openDescendants = 0 → supervising vN writes its result → completed
```

Change `execute_on: descendant-terminal` to `descendant-transition` and a child
that runs its own `review → fix` loop hands control back at every hop as well.
Change it to `child-terminal` and the supervisor hears only its four children
finishing — a child that decomposes further runs its own subtree unwatched, or
supervises it itself.

## Related Specifications

- [Plan Language — Semantic Constraints](rhei-plan-language.spec.md#3-semantic-constraints)
- [Run — Execution Loop](rhei-run.spec.md#3-execution-loop)
- [Next — Default Behavior](rhei-next.spec.md#3-default-behavior-claim-mode)
- [States — Per-state fields](rhei-states.spec.md#12-per-state-fields)
- [Transitions — Counted Loops](rhei-transitions.spec.md#43-counted-loops)
- [Agents — Prompt Composition](rhei-agents.spec.md#3-prompt-composition)
- [Snapshots — Lineage Resolution](rhei-snapshots.spec.md#43-lineage-resolution)
