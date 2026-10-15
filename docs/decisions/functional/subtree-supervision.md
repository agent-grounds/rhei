# DF-subtree-supervision: A supervisor is a barrier over its subtree, woken after the fact

## Status

accepted

## Context

The non-leaf task model made a parent a task in its own right, worked once
after every descendant is terminal and never concurrently with one of them
([§FS-rhei-plan-language.3](../../functional-spec/rhei-plan-language.spec.md#3-semantic-constraints)). That gives a parent exactly one chance to look at
its subtree: at the end, with nothing left to steer. A review/fix chain
authored as four children runs unattended, and the parent's hard-won context —
why the work was decomposed this way, what "good" looks like — is not in the
room while the children run.

The ask was a parent that acts as a *supervisor*: woken after each subtask,
its own session continued, able to judge what just happened and steer what
comes next. Four shapes were weighed. [§FS-rhei-supervision](../../functional-spec/rhei-supervision.spec.md#fs-rhei-supervision-subtree-supervision-specification)

1. **Incremental decomposition, no engine change.** The parent holds the
   chain as a queue in its body and appends one child per visit; today's rule
   already schedules it between children it dispatches itself. Works now, but
   the plan does not show the chain as tasks until they are dispatched, so
   `rhei list` and the dashboard cannot see what is coming.
2. **Artifact gates on the children.** Pre-authored children whose states
   declare an `inputs:` brief the supervisor writes, plus a readiness rule
   that wakes the parent when its subtree is "quiescent". Works for
   after-every-task, but after-every-*state* needs a brief per child-state
   declared on every state of the child's machine, which couples a reusable
   machine to being supervised.
3. **A transition callback that resumes the parent's session.** No engine
   change, but the supervisor runs off the books — no cost accounting, no
   logs, outside the supervised process group ([§DA-supervised-process-groups](../architectural/supervised-process-groups.md#da-supervised-process-groups-subprocesses-are-supervised-process-groups-with-one-termination-path))
   — and can only approve or reject the one transition it fires on.
4. **A pre-transition veto**, where the supervisor decides the child's edge.
   Duplicates what `on_leave` callbacks already are
   ([§FS-rhei-transitions.3.2](../../functional-spec/rhei-transitions.spec.md#32-callback-trigger-triggeredby-callback)) and would make the supervisor a second
   transition authority beside `rhei run`.

## Decision

Supervision is an engine-owned **hold/release barrier** declared on a state
with `execute_on: <scope>-<event>` ([§FS-rhei-supervision.1](../../functional-spec/rhei-supervision.spec.md#1-declaring-a-supervisor)) — one of
`child-terminal`, `child-transition`, `descendant-terminal`, or
`descendant-transition`. The scope picks whose moves the supervisor hears (its
direct children, or its whole subtree) and the event picks which of them (a
task finishing, or every transition it applies); together they pick the
checkpoint set. The hold moves into the engine so the children's states stay
plain and reusable unsupervised.

- A supervisor and its descendants are never worked at the same time. Entry
  holds the subtree; the supervisor's self-loop releases it; a checkpoint
  holds it again and the supervisor is ready once nothing beneath it is in
  flight ([§FS-rhei-supervision.3.1](../../functional-spec/rhei-supervision.spec.md#31-the-rule)). This is the non-leaf model's existing
  guarantee, extended to a parent that runs many times.
- Checkpoints are **post-transition** and are delivered to the **nearest
  in-scope** supervising ancestor only ([§FS-rhei-supervision.2](../../functional-spec/rhei-supervision.spec.md#2-checkpoints)): a `child-*`
  supervisor declines a grandchild's move and the event climbs to the next
  ancestor whose scope reaches that deep, or to nobody. The supervisor judges
  what happened; vetoing stays with callbacks.
- The supervisor steers with the levers that already exist: a *brief* the
  next step reads, plan edits that append children, `rhei transition` that
  cancels held ones, and its own terminal edge selected by the new
  `openDescendants` operand ([§FS-rhei-supervision.4.1](../../functional-spec/rhei-supervision.spec.md#41-the-opendescendants-operand),
  [§FS-rhei-supervision.5](../../functional-spec/rhei-supervision.spec.md#5-prompt-composition)). The descendants-first guard still decides whether
  that edge may be taken ([§FS-rhei-transition-cmd.3.1](../../functional-spec/rhei-transition-cmd.spec.md#31-descendants-first-on-terminal-entry)).
- The phase and the pending checkpoints live in plan frontmatter beside
  `stateVisits` ([§FS-rhei-supervision.3.3](../../functional-spec/rhei-supervision.spec.md#33-supervision-metadata)), written on the shared transition
  path, so a stopped run resumes where it was and a manual worker sees the
  same state the orchestrator would.
- A supervisor that changes nothing changes nothing: its self-loop releases
  the subtree, and it is not ready again until a descendant produces a
  checkpoint. Supervision never spins.

## Consequences

A spent supervising visit must recover without releasing its hold or losing
the checkpoints it has yet to read. `rhei retry` grants an audited attempt
budget to that same visit ([§FS-rhei-retry.2](../../functional-spec/rhei-retry.spec.md#2-grant-and-refusals)), and the halt names it. A manual
self-loop with unread checkpoints requires the explicit `--release-subtree`
request ([§FS-rhei-transition-cmd.2](../../functional-spec/rhei-transition-cmd.spec.md#2-options)), because release deliberately discards
those events. The engine's release remains its own decision. This raises only
the attended level-2 bound, preserving the composed limits and useful recovery
guidance of [§REQ-bounded-neural-work.1](../../requirements/bounded-neural-work.spec.md#1-the-five-levels), [§REQ-bounded-neural-work.4](../../requirements/bounded-neural-work.spec.md#4-nothing-creates-capacity) and
[§REQ-bounded-neural-work.5](../../requirements/bounded-neural-work.spec.md#5-exhaustion-is-a-halt-where-the-ticket-stands).

- All four triggers fall out of one rule; the `*-transition` values are the
  expensive setting — they serialize the subtree at every hop and spend one
  supervisor invocation per hop — and the spec says so. Scope narrows what
  *wakes* a supervisor, never what it holds: a `child-*` supervisor is still
  the barrier over its whole subtree, so supervision can be layered one level
  of decomposition at a time without a task having two owners.
- Pre-authored chains stay visible as tasks from the start, so every surface
  that explains readiness needs a "held by supervisor" reason row
  ([§FS-rhei-supervision.3.4](../../functional-spec/rhei-supervision.spec.md#34-manual-workers)) or a held subtree reads as a stall.
- Self-loops become a general loop-back re-entry on agent states
  ([§FS-rhei-supervision.4.2](../../functional-spec/rhei-supervision.spec.md#42-self-loops-on-agent-states)) rather than a polling-only construct, and
  `condition:` expressions can see the subtree through `openDescendants`; the
  sentence in [§FS-rhei-transition-cmd.3.1](../../functional-spec/rhei-transition-cmd.spec.md#31-descendants-first-on-terminal-entry) that said machines cannot see
  children is revised to say they can select but never permit.
- Real context continuity depends on the agent's snapshot session support;
  with the built-in `claude-code` profile still unsupported
  ([§FS-rhei-snapshots.9.3](../../functional-spec/rhei-snapshots.spec.md#93-per-agent-runtime-behavior)) a supervisor runs each visit cold, carried by
  checkpoints and briefs. That adapter is tracked on the roadmap
  independently ([§RM-rhei-roadmap](../../functional-spec/roadmap.md#rm-rhei-roadmap-roadmap)).
