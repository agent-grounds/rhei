# AR-neural-admission: One serialized boundary for neural work

The scheduler cannot construct a neural subprocess directly. It submits a
resolved launch to the neural-admission component, which either returns an
owned reservation and confined launch capability or a typed halt. This single
boundary realizes §FS-rhei-budgets and §REQ-bounded-neural-work beneath every
scheduler, embedded runtime, callback/program bridge, retry, poll, fanout, and
nested run.

## 1. Boundary

The admission API takes the project/ticket/attempt identities, resolved launch
tuple, timeout and inner-budget snapshot, threshold, fanout group, ancestry
token, and intended edge reservation. It returns one of:

```text
Admitted { reservation, confined_launch }
Refused  { reason_code, budget_snapshot, evidence_gap }
```

Only `confined_launch` can obtain broker credentials or spawn a provider-capable
process. Raw scheduler, agent-profile, program, callback, and embedded-runtime
paths have no such capability. A source-level dependency rule keeps process
creation and provider credentials below this module, so adding a new spawn path
cannot accidentally omit admission. §AR-agent-orchestrator-workflow.3.3

## 2. Components

The boundary composes five owned parts:

1. **Classifier/resolver** determines whether any direct or reachable path is
   neural and produces the exact qualification tuple. Uncertainty is neural.
2. **Qualification registry** verifies signed, versioned evidence bundles and
   computes `T`, residual `R`, and `FWC`; it has no declared-max fallback.
3. **Ledger** verifies and serializes the persistent hash-chained receipt
   journal, reserves all units, and emits live budget events.
4. **Broker/confinement launcher** supplies one-request-at-a-time provider
   access, captures usage durably, enforces the watchdog, and owns teardown.
5. **Settler/recovery coordinator** matches accounting, spawn, and transition
   receipts idempotently and retains every ambiguous exposure.

The public command layer calls the same ledger for init/show/adjust/reconcile
and recover. Validation and dry-run use a read-only transaction that shares
resolution and qualification code but cannot append.

## 3. Locking and transaction order

The global order is: Panta budget sidecar, sorted rhei metadata sidecars,
sorted task sidecars, then sorted transition-ledger sidecars. Admission usually
needs only the first lock; assigning a missing ticket identity additionally
takes its metadata lock while the budget lock remains held. Edge commitment
takes the already owned budget lock before plan and transition locks, never in
the reverse order.

One fanout group is one ledger transaction. It validates every arm, reserves
all invocation and FWC units plus one ticket-travel unit, syncs one group of
receipts, and only then yields launch capabilities. If any arm refuses, no
reservation is appended. Competing processes use the same stable sidecar and
therefore cannot double-spend the last unit. §AR-agent-orchestrator-workflow.3.3.1

The journal lives under `.agent-grounds/rhei/budgets/`, outside `runtime/` and
outside reset cleanup. Its stable lock follows the same identity rule as the
transition ledger. Atomic replacement is allowed only for verified recovery;
the sidecar remains held and is never replaced.

## 4. Spawn and ambiguity boundary

The durable reservation is written before process creation. Immediately after
the OS spawn primitive returns, admission appends a confirmed-start receipt.
If the process-creation call, prompt delivery, broker handshake, or receipt
write leaves it uncertain whether provider-capable work began, it appends or
recovers an ambiguous start and retains the invocation unit and full FWC.

A release is possible only when the launcher proves it never created a process
with provider capability and no broker request was accepted. Teardown, timeout,
interrupt, failed prompt delivery, and local process exit do not constitute
that proof after capability transfer. §FS-rhei-run.3.2

## 5. Broker and containment

Each admitted invocation gets an unforgeable reservation handle and
short-lived broker credential scoped to its tuple and remaining envelope. The
broker admits at most one unsettled request. It checks verified per-request
dimensions, persists usage and price evidence behind a durable barrier, updates
the ledger, then permits another request. When measured charge reaches `T`, a
usage watchdog fails, or any bound is uncertain, it stops accepting requests
and triggers the common process-group teardown.

The launcher supplies no direct provider credential. OS-specific confinement
must prevent alternate egress, inherited credentials, extensions, MCP servers,
fallbacks, and nested clients unless the qualification includes them. The
qualification harness proves this by attempting every prohibited path before a
tuple can enter the release registry. A local firewall best effort or
post-request detector is not a proof.

## 6. Settlement and edge recovery

The settler consumes normalized accounting evidence but applies the stricter
completeness and price predicates of §FS-rhei-budgets.7. It appends settlement
against the original reservation id; duplicate identical evidence is a no-op
and conflicting evidence stops admission. A final charge above FWC appends a
breach before invalidating the tuple.

Travel reservation and transition commitment use a shared transition receipt
id. Recovery considers four states: neither side (release or retain according
to spawn evidence), budget only (apply the already-earned edge if its outcome
receipt exists), transition only (append the matching budget transition), or
both (no-op). It never reruns callbacks or infers an outcome from an absent
receipt. §AR-agent-orchestrator-workflow.3.4

## 7. Nested and embedded execution

Admission passes a non-secret ancestry descriptor and an authenticated ledger
handle to children. The descriptor names project, parent reservation, maximum
descendant envelope, and broker endpoint; it contains no provider credential.
The embedded runtime receives the same handle through its API rather than
opening an independent balance. A nested process that cannot authenticate the
handle or reach the ledger returns a typed `ancestor_unavailable` refusal.

Descendant reservations are ledger entries in their own right for invocation
count and visibility, while their FWC must fit within the parent's descendant
envelope. Settlement cannot make a child outlive or enlarge an ancestor.

## 8. Evidence and rollout

Qualification bundles are immutable content-addressed artifacts. The build's
registry names allowed tuple hashes; an operator cannot author or opt into a
new grade. Deterministic fixture contracts are compiled only into the test
harness and cannot appear in a release registry. Real bundles contain raw
client/provider fixtures, capture timing bounds, request concurrency probes,
kill/cancellation results, nested/fallback enumeration, price/billing terms,
credential and egress confinement results, and OS/architecture provenance.

Codex `0.153.4` and pi `0.84.1`, restricted to brokered API billing and the
approved `gpt-5.2-codex` subset, each need complete Linux, macOS, and Windows
bundles before rollout. Missing evidence keeps the corresponding tuple absent
from the registry. CI verifies the content hashes and reruns portable engine
fixtures; provider-contract renewal is a release input, not a unit-test
assumption. §REQ-cross-platform.3
