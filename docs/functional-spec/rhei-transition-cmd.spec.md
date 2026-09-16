# FS-rhei-transition-cmd: `rhei transition`

Atomically advance a task's state using compare-and-swap semantics. `rhei transition` is the coordination primitive for manual workers and concurrent agents: only the caller whose expected `--from` matches the task's actual current state wins the race, and every transition is validated against the active state machine before any write.

## 1. Usage

```bash
rhei transition <TICKET_ID> --from <STATE> --to <STATE>
rhei transition <TICKET_ID> --from <STATE> --to <STATE> --result <MESSAGE>
rhei transition <RHEI_PLAN> --task <TASK_ID> --from <STATE> --to <STATE>
```

The positional slot is a *ticket or plan*, on the shared rule every
single-ticket command follows ([§FS-rhei-usage.2](rhei-usage.spec.md#2-coordination-through-the-state-machine)): an argument naming an
existing path is the plan, an id-shaped argument naming no path is the ticket.
The ticket must be named one way or the other.

## 2. Options

| Flag             | Required | Default | Description                                                                 |
|------------------|----------|---------|-----------------------------------------------------------------------------|
| `--task <ID>`    | Unless named positionally | | Ticket identifier: project-qualified (`auth.1`) or rhei-local (`1`). See §2.1. |
| `--from <STATE>` | Yes      |         | Expected current state (compare-and-swap guard)                             |
| `--to <STATE>`   | Yes      |         | Target state                                                                |
| `--result <MSG>` | Only when `--to` is a `final: true` state and the ticket has no result yet | | Result message appended to `runtime/results/<task-id>.md`. See §3.2. |
| `--supervisor <TASK_ID>` | No |  | The move's nearest in-scope supervising ancestor. Suppresses the checkpoint the move would otherwise deliver to it. |
| `--no-callbacks` | No       | false   | Skip execution of `on_leave` / `on_enter` callbacks registered on the edge  |

`--result` is accepted on any transition, not only terminal ones: a message
passed on a non-terminal hop is appended to the same result file and creates it
if absent, which is one of the two ways the terminal-result obligation can
already be satisfied by the time the ticket reaches a `final: true` state. A
`--result` whose message is empty or whitespace-only is rejected — an empty
result is the exact thing §3.2 refuses, and accepting the flag while ignoring
its value would hide that.

`--supervisor` grants no authority. The hold a supervisor places on its subtree
is a dispatch-and-claim hold
([§FS-rhei-supervision.3.1](rhei-supervision.spec.md#31-the-rule)), so the move
lands on a held descendant with or without the flag; its one effect is that the
checkpoint the move would otherwise deliver to the named supervisor is not
recorded ([§FS-rhei-supervision.2.1](rhei-supervision.spec.md#21-checkpoint-events)),
so a supervisor acting on its own held descendant is not woken by its own doing.
The value is an ordinary ticket target (§2.1), so a value naming no task is
rejected before anything is applied. Every value that resolves to a real task
must equal the transitioning task's nearest in-scope supervising ancestor under
the scope-first ownership rule
([§FS-rhei-supervision.2.2](rhei-supervision.spec.md#22-nearest-in-scope-supervising-ancestor)).
The owner is selected before its event filter is considered: a nearer
`child-*` ancestor outside the moving task's scope is skipped, while an
in-scope ancestor remains the owner even when this particular move would not
emit a checkpoint.

A different real task is refused non-zero. The diagnostic identifies the
moving task, the supplied supervisor, and the expected ancestor, and tells the
caller to pass the expected id or omit the flag when the move is not that
supervisor's own. When the moving task has no in-scope supervising ancestor,
any resolved explicit value is refused; the diagnostic identifies the moving
task and supplied value, explains that there is no such ancestor, and tells the
caller to omit the flag. Neither refusal changes the plan or creates a result,
ledger entry, checkpoint, or other transition effect. Correct explicit values,
including rhei-local and project-qualified spellings of them, retain the
checkpoint suppression above; an omitted value retains ordinary checkpoint
delivery. The flag confers no authority and does not alter existing
dispatch-and-claim holds or in-flight suppression.

State values passed to `--from` and `--to` follow the state-value rendering rules in the [main spec](rhei-plan-language.spec.md#32-state-validity): bare for names that match `IDENTIFIER`, backtick-wrapped otherwise.

### 2.1. Ticket Targets

The ticket target — positional or `--task` — accepts either the
project-qualified ticket id (`auth.1`, numbers or names in either segment) or a
rhei-local shorthand (`1`). A shorthand resolves
only when exactly one rhei in the project contains that ticket; when more than
one does, the error names the qualified candidates. Output, the result file,
and the ledger entry always use the qualified id regardless of how the target
was written ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)).

`rhei transition` takes no `--rhei` flag: the explicit ticket target already
names the scope. The rewrite is routed to the file of the rhei that owns the
ticket, under that rhei's own rhei-local heading ([§FS-rhei-panta.6.1](rhei-panta.spec.md#61-readiness-and-rhei-next)).

## 3. Behavior

1. Load the state machine and plan (single-file or directory workspace). Validate.
2. Locate the task by id. Fail if it does not exist.
3. Acquire the canonical sibling sidecar for the plan file (single-file plan)
   or task file that contains the task (directory workspace), following the
   shared writer protocol in
   [§AR-agent-orchestrator-workflow.3.3.1](../architecture/agent-orchestrator-workflow.spec.md#331-stable-writer-exclusion).
   The sidecar is the sole writer lock; the replaceable plan destination stays
   unlocked. Hold the sidecar through callbacks, replacement, ledger and result
   writes, terminal finalization, success, or restoration.
4. Re-read the task's current state from the current destination pathname under
   the sidecar. If it does not equal `--from`, fail with a compare-and-swap
   conflict error and print the actual current state.
5. If `--supervisor` was supplied and resolved, validate under the lock that it
   equals the re-read task's nearest in-scope supervising ancestor as defined
   in §2. Refuse a mismatch or the absence of such an ancestor with the §2
   diagnostic. A stale `--from` therefore retains precedence over this check.
   Once compare-and-swap succeeds, supervisor validation precedes target-profile,
   edge and condition evaluation, callbacks and redirects, artifact checks,
   transition-metadata preparation, and every state, result, ledger, or
   checkpoint effect. `--no-callbacks` does not bypass it.
6. Validate that a declared transition exists from `--from` to `--to` in the active state machine. Reject if the edge is unlisted. Then evaluate the edge's `condition:`, if it declares one, and reject when it is unmet, naming which transitions from `--from` *are* currently applicable.
7. Apply the descendants-first guard (§3.1). Reject before any callback runs
   when `--to` is a `final: true` state and the task still has a non-terminal
   descendant. The guard runs after the edge is confirmed declared and
   currently applicable, so a move the machine never offered — unlisted or
   condition-blocked — is reported as such rather than as an open subtree: a
   user is not sent to finish descendants for a move that was never available.
8. After all preceding guards accept the attempt, allocate its transient
   callback firing identity immediately before executing `on_leave`. Execute
   `on_leave` on the source state, if any, unless `--no-callbacks` is set. All
   callbacks for the attempt share the identity and see its central-ledger
   status as pending ([§FS-rhei-transitions.1.2](rhei-transitions.spec.md#12-firing-identity-and-callback-time-visibility)).
9. Verify that every required `outputs:` artifact declared on the source state
   exists (see [Plan Language Specification — State Artifact
   Contracts](rhei-plan-language.spec.md#310-state-artifact-contracts)). Missing
   outputs abort the transition before the state write. This check is skipped
   when the effective target is the `cancelled` state: cancellation abandons the
   work, so the source state's artifact contract is moot. Nothing else on the
   path changes — step 7's descendants-first guard, step 10's target inputs, step
   11's terminal-result obligation, and the callbacks all still apply, so a
   cancel into `cancelled` still needs `--result` or a result on disk.
10. Resolve the target state's `inputs:` artifacts. Missing required inputs abort the transition before the state write; optional inputs are resolved but do not block entry.
11. Apply the terminal-result obligation (§3.2) against the same effective
    target, before the state write: when the target is `final: true`, either
    `runtime/results/<task-id>.md` already has content or `--result` carried a
    message. Neither, and the transition is refused with the plan untouched.
12. Rewrite the task's `**State:**` line to the new state value (with counted-visit suffix when applicable) and write the file atomically (temp file + rename).
13. Execute the `on_enter` callback on the target state, if any, unless `--no-callbacks` is set. The write comes first so the callback observes the plan already in the state it is entering; the callback still sees the attempt's central-ledger status as pending. A callback that fails rolls the write back to the file's previous contents, and the transition fails. When the rollback itself fails, the error says so — the plan file may then be inconsistent.
14. Append one state-transition entry to `runtime/state-transitions.log` as
    `<task-id> <from>@<to>`, creating the `runtime/` directory if needed. The
    file is the central, deterministic audit trail for all task state changes.
    Append `--result`, when given, to `runtime/results/<task-id>.md`; when the
    effective target is `final: true`, also perform the terminal finalization
    of [§FS-rhei-complete.3](rhei-complete.spec.md#3-result-file) — ensure the result file, drop `**Assignee:**`, and
    link the result from the task body.
15. Release the sidecar lock.

Steps 11 and 14 are the same code on every verb that can move a task, so a
`rhei transition --result` into a terminal state leaves a ledger line, a result
file, a `> **Result:**` link, and an absent `**Assignee:**` indistinguishable
from the ones `rhei complete` and `rhei run` leave for the same edge.

Outside a terminal entry, `rhei transition` does not add, remove, or modify
the `**Assignee:**` line, with one exception: the self-loop of a supervising
state ([§FS-rhei-supervision.3.1](rhei-supervision.spec.md#31-the-rule)). That edge ends a supervisor's visit, so it
ends the claim on that visit too, and the line is dropped exactly as a
terminal entry drops it. Assignment is otherwise owned by `rhei next`;
unassignment is part of the shared terminal finalization above, which
`rhei complete` also runs.

`rhei transition` deliberately does **not** check `**Prior:**` dependencies.
It is the explicit human-initiated primitive, so it is the escape hatch for
the moves the scheduling commands refuse: leaving a gating state
([§FS-rhei-complete.4](rhei-complete.spec.md#4-behavior)), and advancing a ticket ahead of an unsatisfied prior.
`rhei next`, `rhei run`, and `rhei complete` all enforce readiness; a caller
that reaches for `transition` is stating the out-of-order move is intended.
Because the resulting plan then contradicts its own declared dependencies,
`rhei validate` reports it as a warning ([§FS-rhei-validate.4](rhei-validate.spec.md#4-behavior)) rather than
letting it pass unremarked.

Counted-visit accounting: if the target state declares a `visits` budget and `--to` is a loop-back re-entry, the runtime increments `metadata.tasks.<id>.stateVisits.<target>` and renders the new visit number in `**State:**` using the `-<n>` suffix. See [Transitions Specification — Counted Loops](rhei-transitions.spec.md#43-counted-loops).

All plan-writing verbs that use this path share that identity, acquisition
order, authoritative-read rule, and lifetime. Metadata is acquired before a
distinct task sidecar, the ledger last, and release is in reverse order;
canonical identity deduplication makes a single-file plan one acquisition.
Configured recovery first releases this stack and then runs as a separate
ordinary transition. `next --peek` and `release --dry-run` remain read-only and
establish no sidecar. Operators upgrading a shared directory must stop older
writers, upgrade every writer, and then resume; live mixed-version writing is
unsupported, and existing sidecars are retained and reused.

### 3.1. Descendants-First on Terminal Entry

A transition into a `final: true` state is rejected while the task has any
non-terminal descendant — child, grandchild, or deeper. The error names the
target state and every open descendant as `Task <id> (<state>)` — the same
shape `rhei next` ([§FS-rhei-next.3.4](rhei-next.spec.md#34-claiming-a-non-leaf-ticket-with---task)) and the run report ([§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout))
print, so a user moving between the three verbs reads one format. Its guidance
names the commands that reveal and claim the open work rather than only
restating the rule ([§FS-rhei-errors.2](rhei-errors.spec.md#2-copy-paste-safety)).

This guard lives on the **shared transition path**, beside compare-and-swap,
`outputs:`/`inputs:` enforcement, and callbacks. It therefore applies
identically to every verb that can move a task into a terminal state:
`rhei transition`, `rhei complete` ([§FS-rhei-complete.4](rhei-complete.spec.md#4-behavior)), `rhei run`'s
orchestrator-owned auto-advance ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)), and a callback that redirects
an edge with `nextState` ([§FS-rhei-transitions.3.2](rhei-transitions.spec.md#32-callback-trigger-triggeredby-callback)) — a redirect is re-checked
against the effective target, so it cannot smuggle a terminal entry past the
guard. No command holds a private copy of the rule, and no state machine can
opt out of it: a transition `condition:` can *select* a parent's terminal edge
on its subtree with `openDescendants` ([§FS-rhei-supervision.4.1](rhei-supervision.spec.md#41-the-opendescendants-operand)), but
selection is not permission — a machine author has no way to take a terminal
edge past an open descendant. The engine must guard it.

The guard is deliberately **not** symmetric with `**Prior:**` readiness, which
`rhei transition` skips as the human escape hatch (§3). The line between the
two is the one `rhei validate` already draws: a terminal parent with an open
descendant is an **error**, an out-of-order prior is a **warning**
([§FS-rhei-validate.4](rhei-validate.spec.md#4-behavior)). `rhei transition` may deliberately produce a warning; it
must never be able to produce an error. A parent that genuinely must finish
ahead of its subtree is finished by finishing or cancelling the subtree first —
`cancelled` is terminal, so an abandoned child satisfies the guard.

`rhei next` is unaffected by this guard because claiming does not advance state
([§FS-rhei-next.3](rhei-next.spec.md#3-default-behavior-claim-mode)); it applies the eligibility rule instead
([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)).

### 3.2. Terminal Result on Entry

A transition into a `final: true` state is refused unless the ticket has a
non-empty `runtime/results/<task-id>.md` or the caller carried a message on the
move. The obligation belongs to the state, not to the command: it is specified
once in [§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result) and enforced here, on the same shared path as
compare-and-swap, the descendants-first guard (§3.1), and `inputs:` /
`outputs:` resolution.

Consequently `rhei complete`, `rhei transition --result`, `rhei run`'s
orchestrator-owned auto-advance, `rhei run`'s engine-owned failure routes, and
a callback `nextState` redirect all enforce it identically, and the check runs
against the effective target so a redirect cannot smuggle a terminal entry past
it. No command holds a private copy of the rule.

The refusal names the path that was checked and the flag that carries the
message ([§FS-rhei-errors.2](rhei-errors.spec.md#2-copy-paste-safety)):

```text
Error: Task auth.1 cannot enter terminal state 'completed' without a result.
       Expected a non-empty result file at: runtime/results/auth.1.md
  help: a final state records why the ticket ended there. Pass it on the move:
        rhei transition auth.1 --from review --to completed --result "<what happened>"
        (rhei complete auth.1 --result "<what happened>" for the everyday finish),
        or write runtime/results/auth.1.md before the move.
```

## 4. Compare-and-Swap Conflicts

Two agents that race on the same task both specify the same `--from`. The first call to acquire the lock rewrites the state. The second call re-reads under the lock, sees the actual state no longer matches `--from`, and fails non-zero with:

```text
Error: Task <ID> is in state '<actual>', not '<from>'.
       Another transition may have preceded this call.
```

Losers are expected to re-read the plan and either re-select with `rhei next` or retry against the new state.

## 5. Output

On success:

```text
Task <ID> transitioned: '<from>' -> '<to>'
```

With `--no-callbacks`:

```text
Task <ID> transitioned: '<from>' -> '<to>' (callbacks skipped)
```

## Relationship to Other Commands

| Command            | What it does                                                                    |
|--------------------|---------------------------------------------------------------------------------|
| `rhei next`        | Claims the next ready task (assigns without transitioning), prints instructions |
| `rhei next --peek` | Read-only: prints the next claimable task without claiming it                   |
| `rhei transition`  | Atomically changes a task's state; `--result` appends to the result file, and carries the message a terminal entry requires (§3.2) |
| `rhei complete`    | Infers the one-hop terminal target and runs the same transition with `--result` |
| `rhei reset`       | Returns each task to the state it was authored in ([§FS-rhei-reset.2.2](rhei-reset.spec.md#22-authored-state)), removes `runtime/`; narrowed with `--rhei <id>` it removes only the in-scope tickets' keyed output ([§FS-rhei-reset.2.1](rhei-reset.spec.md#21-narrowed-reset---rhei)) |

The typical agent loop is: `next` (claim) → work → `transition` (advance as needed) → `complete` (finish, record result, release).

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — state-value grammar and validation rules
- [States Specification](rhei-states.spec.md) — state machine format
- [Transitions Specification](rhei-transitions.spec.md) — transition YAML schema, callbacks, and counted-loop accounting
- [Callbacks Specification](rhei-callbacks.spec.md) — `on_leave` / `on_enter` callback examples
- [Next Command](rhei-next.spec.md) — `rhei next` behavioral contract
- [Complete Command](rhei-complete.spec.md) — `rhei complete` behavioral contract
