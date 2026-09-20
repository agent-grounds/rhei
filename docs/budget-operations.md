# Budget operations on the development branch

The bounded-work implementation is **not ready for rollout**. The persistent
ledger, inspection, adjustment, reservation primitives, and provider-request
barrier are present. The qualification registry is empty. Agent, program,
shell-callback, and snapshot-continuation launches refuse until a verified
confined launch path is available. The feature's complete contract remains
[§FS-rhei-budgets](functional-spec/rhei-budgets.spec.md).

Signed-bundle inspection is available as the `rhei-plan` example
`inspect_qualification`. It verifies exact signed bytes and artifact hashes
without registering any tuple or launching a provider. The required complete
qualification harness and real-client evidence are still absent. Budget
refusals now use the shared run-event stream and leave retained reports;
receipt event producers are available in the core ledger, but their positive
execution integration depends on the unfinished confined runtime.

## Authored bounds

Each neural state supplies a positive measured threshold, with
`defaults.budget_threshold` as its fallback:

```yaml
budget_threshold:
  currency: USD
  amount_micro: 500000
```

This is `T`. A qualified transport must additionally establish residual `R`;
admission reserves all of `T + R` before process creation. A user price file
cannot establish `R`. See
[§FS-rhei-budgets.2.1](functional-spec/rhei-budgets.spec.md#21-invocation-threshold)
and [§FS-rhei-budgets.6](functional-spec/rhei-budgets.spec.md#6-provider-spend-qualification).

Each state-machine profile needs a positive `transition_limit`. The built-in
profile uses 100 applied edges for a ticket's lifetime. Restart and reset do
not refill travel. State-level `visits`, `attempts`, polling limits, and finite
timeouts still apply. See
[§FS-rhei-budgets.2.2](functional-spec/rhei-budgets.spec.md#22-ticket-travel).

## Initialize, inspect, and adjust

For a new project with no execution history:

```sh
rhei budget init ./panta --invocations 50 --spend-micro 20000000 --currency USD --reason "Initial project allowance"
rhei budget show ./panta
rhei budget show ./panta --format json
rhei budget adjust ./panta --spend-micro 30000000 --reason "Approved additional work"
```

Amounts are integer micro-units: `20000000` micro-USD is USD 20. Initialization
assigns persistent project and task UUIDs. The journal resides under
`.agent-grounds/rhei/budgets/<project-uuid>/`, outside `runtime/`.
Its committed-history witness lives outside the project at
`$XDG_STATE_HOME/rhei/budget-authority/<project-uuid>/history.jsonl` (with
`$HOME/.local/state`, or `%USERPROFILE%/.local/state`, as the fallback).
Keep that authority intact independently of project backups. A state directory
inside the project is refused. Both copies must agree before any balance is
read or changed; missing authority cannot be reconstructed from project files.
Inspection exposes consumed, reserved, remaining, and the audit trail.
Adjustment cannot reduce a ceiling below settled charges plus outstanding
exposure, change currency, erase consumption, or clear a qualification breach.
See [§FS-rhei-budgets.8](functional-spec/rhei-budgets.spec.md#8-operator-commands).

Selecting a member rhei resolves its full Panta allowance. `show --rhei` never
creates a member-specific balance. Keep financial identities and the journal
together when moving a project. Removing an identity beside existing budget
history, or keeping an identity but deleting its journal, refuses
initialization. Do not delete financial state to obtain a fresh balance.
Initialization validates metadata before activating an allowance and keeps a
durable intent until installation finishes. Retry an interrupted initialization
with the original bounds; it reuses the chosen identities and refuses changed
documents. `rhei cost` and `rhei summary` show lifetime allowance balances
separately from their accounting selection.

## Migration and recovery status

This implementation only initializes demonstrably empty local history.
Existing accounting, spawn, log, or transition artifacts require an
authoritative history import. The `--history` and `reconcile`
command syntax is accepted, but their provider-evidence verification paths
remain incomplete and refuse mutation. A supplied JSON
summary is never treated as authoritative billing evidence.

The eventual command forms are specified in
[§FS-rhei-budgets.8](functional-spec/rhei-budgets.spec.md#8-operator-commands):

```sh
rhei budget init ./panta --invocations 50 --spend-micro 20000000 --currency USD --reason "Import history" --history evidence.json
rhei budget reconcile ./panta --invocation reservation:UUID --evidence provider-evidence.json --reason "Final charge"
rhei budget recover ./panta --journal trusted-journal.jsonl --reason "Restore complete receipts"
```

`recover` accepts a complete journal matching the external history witness,
checks intact receipts in the damaged target, appends the recovery audit to the
witness and candidate, then atomically installs the candidate. `budget show`
prints the authority path; that witness can itself supply `--journal`. A stale
backup or a self-consistent chain without matching authority is refused.
Copies serialize on the same authority; after one advances, another must
recover its stale project journal. Moving a project within that authority
preserves its balance; another host without the authority refuses.

Preserve stable lock sidecars. Unknown usage, ambiguous starts, or an unreadable
chain do not become zero exposure. A crash between witness and project writes
requires recovery from the complete witness. These changes await independent
behavioral verification and do not qualify any real launch. Read-only plan
inspection remains available.

## Qualification status

| Client | Required launch | Linux | macOS | Windows |
| --- | --- | --- | --- | --- |
| Codex 0.153.4 | Brokered API billing, approved `gpt-5.2-codex` subset | Unqualified | Unqualified | Unqualified |
| pi 0.84.1 | Brokered API billing, approved `gpt-5.2-codex` subset | Unqualified | Unqualified | Unqualified |
| Claude Code and other/custom clients | Independently verified exact tuple | Unqualified | Unqualified | Unqualified |

No restricted real launch is permitted yet. The implementation has a durable
request barrier, but no HTTP provider adapter, confined process capability, or
complete transport evidence bundle. Both initial clients and every required
OS remain delivery blockers; a refusal-only release does not satisfy
[§FS-rhei-budgets.12](functional-spec/rhei-budgets.spec.md#12-migration-compatibility-and-delivery-proof).

Environment declarations, custom command names, client streaming usage, and
test fixture JSON cannot qualify production work. The eventual restricted
launches require credential isolation, broker-only egress, and disabled or
fully bounded extensions, nested work, and fallback paths. The promise covers
neural provider charges; infrastructure, shell costs, human workers, and
subscriptions remain outside it. Externally started worker sessions are outside
Rhei-owned subprocess admission. See
[§REQ-bounded-neural-work.1](requirements/bounded-neural-work.spec.md#1-scope).
