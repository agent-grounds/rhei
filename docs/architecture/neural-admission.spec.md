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

The global order is: external authority witness sidecar, Panta budget sidecar, sorted rhei metadata sidecars,
sorted task sidecars, then sorted transition-ledger sidecars. Admission usually
needs only the first lock; assigning a missing ticket identity additionally
takes its metadata lock while the budget lock remains held. Edge commitment
takes the already owned budget lock before plan and transition locks, never in
the reverse order.

Identity replay retains the union of source bindings from legacy single-source
receipts. New identity receipts carry the full live-source set after checking
each previous source. The identity transaction retains all source locks through
the allowance mutation; it does not validate, unlock, and reopen the ledger.

The first arm's ambiguous-start receipt uses that same owned journal. A lease
may reopen the journal only after the admission transaction has ended; it never
waits for a second acquisition of its own sidecar. No lease-cache mutex is held
while acquiring a journal lock. Admission failures publish a typed ticket halt
and the verified ending snapshot before the scheduler marks the ticket stalled.

After a receipt and its witness are durable, the ledger publishes a typed
budget event followed by its verified snapshot to an optional shared sink.
Sink delivery is observational: replay never reissues mutations and missing
UI delivery never refunds exposure. Run frontends encode the same event type
for live JSON and retained event logs; admission refusals retain a report even
when no worker was started. A display-only travel limit is resolved from the
current profile without appending a receipt.

One fanout group is one ledger transaction. It validates every arm, reserves
all invocation and FWC units plus one ticket-travel unit, syncs one group of
receipts, and only then yields launch capabilities. If any arm refuses, no
reservation is appended. Competing processes use the same stable sidecar and
therefore cannot double-spend the last unit. §AR-agent-orchestrator-workflow.3.3.1

Financial reservation and breach mutation additionally acquire sorted tuple
invalidation locks after the budget journal, before appending. No tuple-lock
owner opens another journal. These external locks serialize admissions across
projects with write-ahead invalidations; dry-run only inspects the same records.

The journal lives under `.agent-grounds/rhei/budgets/`, outside `runtime/` and
outside reset cleanup. Its stable lock follows the same identity rule as the
transition ledger. Atomic replacement is allowed only for verified recovery;
the sidecar remains held and is never replaced.

The authority witness described in §FS-rhei-budgets.3.1 is a write-ahead copy
of committed receipt bytes, keyed by project UUID outside the movable project.
Both locks remain held through witness sync and journal sync. An interrupted
append poisons the open writer; a new opener requires byte-for-byte equality.
Recovery validates the entire witnessed chain and accepts only those exact
bytes as a trusted copy, appends the audited recovery to the witness and the
candidate, then atomically replaces the project journal. Initialization holds
the bootstrap lock through a durable metadata intent and activation, so input
errors cannot publish financial state and interrupted writes retain their
original identity and allowance.

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

The Linux backend obtains all three from the kernel rather than from the
client's cooperation. The launch runs under an unprivileged user namespace with
its own network, pid, ipc and uts namespaces, all capabilities dropped and a
cleared environment: egress denial is that the child's network namespace holds
one loopback device of its own, so no host address, proxy or provider endpoint
is routable from it; credential confinement is that an unbound path is not in
the child's mount namespace at all, rather than merely unset; and the process
tree is complete because the launch is that pid namespace's init, so every
descendant — detached, re-parented or daemonized — ends with it. One bound Unix
socket is the only channel across the boundary, and a launch given none has no
provider reach whatsoever. A host without those namespaces has no confinement,
so admission refuses on it rather than running beside it. macOS and Windows
backends are separate obligations under §REQ-cross-platform.3; neither tuple is
qualified until its own platform proof exists.

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

A fanout reservation allocates its transition identity and travel owner before
any arm starts. Admitted central log entries retain the legacy `task from@to`
prefix followed by the shared `transition:<uuid> reservation:<uuid>` identities;
readers consume the movement token independently of these optional receipt
fields. Each arm records its end once, and only the group travel owner may
consume or release travel. Finishing an arm without an edge cannot release the
whole group's travel while another arm is outstanding.


## 7. Nested and embedded execution

Admission passes a non-secret ancestry descriptor and an authenticated ledger
handle to children. The descriptor names project, parent reservation, maximum
descendant envelope, and broker endpoint; it contains no provider credential.
The embedded runtime receives the same handle through its API rather than
opening an independent balance. A nested process that cannot authenticate the
handle or reach the ledger returns a typed `ancestor_unavailable` refusal.

Descendant reservations are ledger entries in their own right for invocation
count and visibility, while their FWC must fit within the parent's descendant
envelope. Settlement cannot make a child outlive or enlarge an ancestor. The
envelope is carried on the ancestor's own reservation, having come from its
qualification's validated delegation graph; admission authenticates the token
against this project's journal and refuses a child under an absent, finished,
envelope-less or already exhausted ancestor. §FS-rhei-budgets.5

## 8. Evidence and rollout

Qualification bundles are immutable content-addressed artifacts. The build's
registry names allowed tuple hashes; an operator cannot author or opt into a
new grade. Deterministic fixture contracts are compiled only into the test
harness and cannot appear in a release registry. The fixture driver has its own
Cargo workspace and target directory; ordinary workspace dependency resolution
cannot enable fixture features.
The e2e target may depend on the ordinary CLI library for parser assertions,
with fixture features disabled. Retry fixture migration moves each declared
initial state to its profile and removes the state-level initial flags; it
preserves transitions, inner limits and all behavioral assertions.
Release-refusal tests execute the ordinary binary with the former environment
switch as well as fixture documents. Real bundles contain raw
client/provider fixtures, capture timing bounds, request concurrency probes,
kill/cancellation results, nested/fallback enumeration, price/billing terms,
credential and egress confinement results, and OS/architecture provenance.

The registry pins the SHA-256 of the exact bundle bytes and the Ed25519 review
key and detached signature at build time. Verification checks that signature,
every referenced artifact's bytes and role, the complete launch tuple, and the
validity interval before constructing a capability. Parsing a bundle or passing
an inspection command never registers it. The initial contained format covers
one outstanding text Responses request, a durable capture barrier, a finite
watchdog, full accepted-request billing after kill, and disabled nested/fallback
routes. Its residual is derived from maximum request dimensions and prices;
it is not a supplied monetary claim. Supporting a different billing dimension
requires extending the reviewed format and its adapter together.

Codex `0.153.4` and pi `0.84.1`, restricted to brokered API billing and the
approved `gpt-5.2-codex` subset, each need complete Linux, macOS, and Windows
bundles before rollout. Missing evidence keeps the corresponding tuple absent
from the registry. CI verifies the content hashes and reruns portable engine
fixtures; provider-contract renewal is a release input, not a unit-test
assumption. §REQ-cross-platform.3
