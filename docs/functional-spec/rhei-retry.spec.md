# FS-rhei-retry: `rhei retry`

Grant an exhausted current visit one more configured attempt budget without
moving the ticket. Recovery must preserve the held subtree and unread
checkpoints of a supervising visit (§FS-rhei-supervision.3.5).

## 1. Usage

```bash
rhei retry <TICKET_ID>
rhei retry <RHEI_PLAN> --task <TASK_ID>
```

Use the shared ticket-or-plan target rules (§FS-rhei-usage.2), qualified ticket
IDs in output, and the active state machine. The command applies to every
non-poll, non-terminal state, including ordinary agent and program states.

## 2. Grant and Refusals

Grant exactly one configured `attempts:` budget (§FS-rhei-agents.3.2.3) to an
exhausted current visit. Persist `grants: [{at, os_user, attempts}]` in its
current spawn record (§FS-rhei-agents.8.4); older records default to no grants.
The effective allowance is configured attempts plus all grants for that visit.
Keep the same move-count visit key, monotonic attempt/log numbering, state,
move ledger, `stateVisits`, supervision phase and pending checkpoints.

For a fan-out state, resolve the active invocations and grant each exhausted
invocation of the current visit its own configured-size grant. An invocation
with attempts remaining receives no grant; it must not prevent recovery of an
exhausted sibling. Invocations whose work the next run can reuse, removed
targets and older visits receive no grant. Refuse without writes when none of
the active invocations has an exhausted current record: report remaining
attempts when any still has capacity, otherwise the missing current record.

Write the record through a temporary file and rename under the same lock used
by attempt charging. Re-read under that lock; an interleaved charge and grant
must both survive in a complete readable record. A concurrent reader sees
either no grant or the whole grant, never half a record or a lost grant.

Refuse non-zero, without writing anything, when:

- Attempts remain under the effective budget: grants cannot be stockpiled.
- The state is polling or terminal.
- There is no spawn record for this current visit (an older visit is not one).
- `RHEI_TASK_ID` is set: a spawned worker cannot refill its own bound.

The grant takes effect on the next `rhei run`, because a live run already set
the halted ticket aside. It creates no travel, visit or invocation headroom;
each admitted granted neural attempt still costs an invocation and may be
refused by that account (§FS-rhei-budgets.4.2). Provider-limited and withheld
attempts retain their independent non-charge rules. `visits:` and
`transition_limit` are not consulted: nothing moves.

## 3. Output and Halt Advice

Success names the ticket/state, configured attempts granted, effective total,
next attempt number and the next-run effect. For a supervising visit it also
reports that the subtree remains held and the pending checkpoint count:

```text
Task plan.1 'supervising': granted 2 more attempts to this visit (4 in total); the next `rhei run` spawns it as attempt 3. Subtree still held, 1 checkpoint pending.
```

Fan-out success names each invocation actually granted, with its own effective
total and next attempt number; it does not claim to refill or spawn a sibling
that still has capacity or whose finished work is reusable.

Retain the halt ending `The ticket stays in '{state}'.` exactly. Append these
clauses, using the configured budget and singular/plural checkpoint spelling:

- Ordinary: `To give this visit {budget} more attempts, run: rhei retry {id}.`
- Supervising: `Its subtree is still held with {N} unread checkpoint(s). To give this visit {budget} more attempts and keep the hold, run: rhei retry {id} (its self-loop would release the subtree instead).`
- Poll: unchanged; the ordinary visit-budget advice does not apply.

## 4. Related Recovery Commands

`rhei run` still halts on an exhausted visit until a grant exists. `retry`
applies no transition and requires no self-loop. `rhei transition` self-loops
release supervisors and require `--release-subtree` when checkpoints are
pending (§FS-rhei-transition-cmd.2). `rhei reset` removes spawn records and
their grants, restoring the plain allowance but refunding no capacity.
