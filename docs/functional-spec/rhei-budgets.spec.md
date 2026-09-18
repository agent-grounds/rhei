# FS-rhei-budgets: Bounded neural work and budget operations

Every Rhei-started neural invocation passes one admission boundary that
reserves finite ticket travel, lifetime invocation capacity, and a proven
worst-case provider charge before spawn. This specification is the
user-visible realization of §REQ-bounded-neural-work; §AR-neural-admission owns
the shared runtime boundary.

## 1. Terms and invariant

An **allowance** is the Panta-lifetime pair `{invocations, spend}`. Invocation
capacity is an integer. Spend is an integer `amount_micro` in one ISO-4217
currency. Both are finite positive integers; `unlimited`, omission, floating
point values, negative values, and saturation are invalid.

A **travel limit** is the finite positive number of applied transitions a
ticket may make during its identity's lifetime. A **reservation** is the
durable pre-spawn claim on one invocation unit, a complete worst-case charge,
and one travel unit for the scheduled ticket. A **settlement** replaces the
reserved monetary amount with an authoritative final charge. An invocation
unit is consumed once a subprocess may have started and is never settled down.

At every durable boundary, for each allowance unit:

```text
consumed + outstanding <= allowance
```

For money, `consumed` is the sum of settled provider charges plus retained
breach charges. For invocations, it is every confirmed or ambiguous start. For
ticket travel, it is applied transitions. Check-and-reserve is one atomic
operation against the one project ledger; no run-local or per-rhei balance may
answer admission. §REQ-bounded-neural-work.2 §REQ-bounded-neural-work.3

## 2. Authored bounds and identities

### 2.1. Invocation threshold

Every state that can start neural work resolves `budget_threshold` from the
state and then `defaults.budget_threshold`. It is an object:

```yaml
budget_threshold:
  currency: USD
  amount_micro: 500000
```

The value is the contained invocation's live measured threshold `T`, not its
complete reservation. Admission reserves `FWC = T + R`, where qualification
supplies the residual `R` under §6. A threshold must be positive, use the
project allowance currency, and be no larger than the remaining spend
allowance after every other outstanding reservation. Omitting it on a neural
state is a validation error. Setting it on a non-neural program is permitted
but does not classify or qualify that program.

An `all_targets` or `all_models` state resolves one threshold for every arm.
The state-level value is shared unless a qualified target's evidence requires
a smaller maximum, in which case the smaller value is used and shown. No arm
may inherit a larger threshold than its ancestor reservation can contain.

### 2.2. Ticket travel

Every state-machine `profiles.<name>` entry has a required
`transition_limit: <positive-integer>` alongside `initial` and `allowed`.
Every node, including the virtual Panta root, resolves a profile and therefore
a travel limit. Legacy machines without profiles have no implied unlimited
profile: an autonomous command refuses them until the machine is migrated.
The shipped built-in machine declares finite limits for all profiles.

One scheduled task reserves one travel unit even when the state fans out to
several invocation arms. Every applied edge consumes it, including an ordinary
self-loop and callback redirect. A poll self-loop is a wait rather than an
applied transition and releases the reservation. A completion with no edge,
an admission cancelled before any arm may start, and a refused completion
condition also release it. The central transition receipt named in §3.3 is the
only evidence that converts reserved travel to consumed travel.

### 2.3. Persistent identity

`rhei budget init` writes an RFC 4122 lowercase UUID as
`budgetProjectId` in YAML frontmatter: in `index.panta.md` for an explicit
Panta, or in the bare rhei's metadata document for its implicit Panta. The
canonical ledger identity is `panta:<uuid>`.

Each task has a persistent lowercase UUID at
`metadata.tasks.<local-id>.budgetTicketId`; basin tasks use their existing
project-qualified metadata keys. Its canonical identity is
`ticket:<project-uuid>:<ticket-uuid>`. Existing tasks receive identities during
initialization. A later-added task receives one atomically before its first
reservation; creating an identity creates no allowance. Project-qualified
task ids remain display and routing names, not financial identities.

Copying, moving, renaming, snapshotting, resetting, or re-instantiating a
document preserves these identities. Two live documents presenting the same
ticket identity share its travel history; inconsistent content for one
identity is an identity conflict and refuses mutation. Removing an identity,
or presenting a project identity without its ledger, refuses admission and
never causes automatic initialization. §FS-rhei-plan-language.2

## 3. Durable ledger and receipts

### 3.1. Location and ownership

The journal is outside the replaceable product `runtime/` tree:

```text
<panta-root>/.agent-grounds/rhei/budgets/<project-uuid>/
  journal.jsonl
  journal.jsonl.lock
```

For an implicit Panta, `<panta-root>` is the bare rhei's execution root. The
empty lock sidecar has stable identity and is never removed. Every reader or
writer locks it, opens the current journal after acquisition, verifies the
whole chain, performs its decision, durably appends and syncs all receipts,
then unlocks. Platform-specific locking realizes the same serialized contract
on Linux, macOS, and Windows. §REQ-cross-platform.2

### 3.2. Journal envelope

Every line is one UTF-8 JSON object with schema `rhei.budget.receipt.v1` and:

```json
{
  "schema": "rhei.budget.receipt.v1",
  "project_id": "panta:550e8400-e29b-41d4-a716-446655440000",
  "sequence": 7,
  "receipt_id": "receipt:018f...",
  "previous_hash": "sha256:<hex>",
  "written_at": "2026-09-18T12:00:00Z",
  "actor": "rhei-run:<run-id>",
  "kind": "reserve",
  "payload": {}
}
```

`sequence` starts at one and is contiguous. `receipt_id` is a path-independent
UUID/ULID identity. `previous_hash` is null on initialization and otherwise is
the SHA-256 of the exact preceding line bytes excluding its newline. An absent,
truncated, malformed, reordered, duplicated, wrong-project, or hash-invalid
line makes the ledger untrustworthy and refuses every admission. Unknown
receipt kinds are retained but make this build read-only until it understands
them.

### 3.3. Receipt kinds

The closed v1 `kind` vocabulary is:

- `initialize`: allowance, currency, import summary, and operator reason;
- `identity`: a newly discovered ticket identity and current display id;
- `reserve`: reservation id, attempt identity, ticket identity, parent
  reservation, qualification tuple and evidence hash, invocation units,
  `T`, `R`, `FWC`, and travel units;
- `start`: whether spawn is confirmed or ambiguous; either consumes the
  invocation reservation;
- `release`: proof that no provider-capable subprocess started, plus released
  money/travel; it never releases a confirmed or ambiguous invocation unit;
- `settle`: authoritative evidence identity/hash, final provider charge, and
  unused money released from that same reservation;
- `transition`: transition receipt id, reservation id, ticket identity,
  from/to, and the central transition-ledger receipt it matches;
- `adjust`, `reconcile`, and `recover`: the audited operator operations of §8;
- `breach`: actual charge, reserved charge, affected qualification, and retained
  exposure.

The attempt identity is §FS-rhei-cost-accounting.3.7's existing identity. A
transition receipt is `transition:<uuid>` and is stored both in this journal
and with the central transition entry. Replaying a receipt id with identical
content is idempotent; replaying it with different content is corruption.
Settlement and earned-edge recovery key on these identities, never timestamps,
file names, or display task ids.

## 4. Shared admission

Immediately before every possible neural spawn, one boundary performs this
ordered transaction:

1. resolve the project, ticket, attempt, ancestor reservation, and every inner
   timeout/attempt/visit/poll bound;
2. resolve the exact executable, configuration, OS, provider, model, billing
   contract, price contract, nested/fallback reachability, and qualification;
3. compute `FWC = T + R` without floating point or overflow;
4. lock and verify the Panta journal, recover completed idempotent receipts,
   and recompute current exposure;
5. verify attempt, visit or poll headroom, one ticket travel unit, one
   invocation unit, full `FWC`, and every ancestor envelope;
6. append and durably sync one reservation containing all units; and only then
7. create the subprocess and append a confirmed or ambiguous start receipt.

No spawn path bypasses this transaction: retries, poll attempts, recovery
re-spawns, supervisor wake-ups, fanout arms, appended descendants, concurrent
passes, resumed and narrowed runs, embedded callers, model-capable programs or
callbacks, and nested Rhei runs all enter here. Error-continuation flags,
skip/no-callback switches, and accounting-only price overrides do not waive it.
`rhei validate` and `rhei run --dry-run` execute steps 1–5 read-only and debit
nothing. §FS-rhei-run.4 §FS-rhei-validate.4

An externally launched worker-authority session remains outside this boundary.
If it launches `rhei run`, the nested run is Rhei-started neural work and must
reach the parent's ledger or refuse its own admissions. §FS-rhei-agents.3.1

## 5. Parallel, fanout, retry, poll, and ancestry rules

Parallel schedulers and separate OS processes serialize on the same ledger.
No two reservations can observe or claim the same remaining unit. Fanout is
one atomic transaction: all arms reserve one invocation unit and their full
FWC apiece, plus one travel unit for the scheduled ticket, or no arm starts.
A partial reservation is recovered to none before any spawn.

Every neural retry and poll attempt consumes a distinct invocation unit and
provider exposure. A per-visit attempt refund only lets the inner attempt
counter be reused; it cannot refund a start, provider charge, or Panta
invocation unit. A poll wait releases its travel reservation but retains the
invocation and provider accounting. An ordinary self-loop consumes travel.

Direct and transitive work inherits `project_id`, ledger location, reservation
ancestry, and remaining envelope through a non-secret budget context. A child
reservation's `FWC` must fit within both current project capacity and its
ancestor's reserved envelope; it does not add the ancestor reservation a
second time. A nested runtime that cannot authenticate and lock the inherited
ledger refuses. Credentials and provider egress are not inherited unless the
qualified broker mediates them under §6.4.

## 6. Provider-spend qualification

### 6.1. Grades and tuple

The only grades are:

- `enforced`: the provider/transport contract prevents overshoot for the whole
  resolved invocation, including accepted and background work;
- `contained`: Rhei stops at live threshold `T` and has proved and reserved a
  finite residual `R` for every billable exposure not yet durably observed;
- `unqualified`: admission is refused.

There is no `declared` grade and no opt-in that accepts an unproved configured
maximum. Qualification binds the exact executable hash and semantic version,
arguments and environment policy, Rhei version, OS/architecture, agent,
provider, model, API-billing mode, credential/egress policy, billing contract,
price-book identity and effective interval, and evidence bundle hash. A change
to any bound field produces a different, unqualified tuple.

### 6.2. Full reservation

For `contained`, `T` is the resolved `budget_threshold`. `R` is:

```text
(maximum requests committed during capture latency
 + maximum requests in flight when containment fires)
* verified maximum billable dimensions per request
* maximum applicable qualified prices
+ complete post-kill, nested, and fallback exposure not already included
```

All arithmetic rounds in the provider's costly direction and refuses on
overflow. The full `T + R` is reserved before spawn. A usage stream or local
kill establishes neither `R` nor qualification by itself.

### 6.3. Four mandatory obligations

A qualification bundle is rejected unless it proves all four obligations:

1. **Capture:** a validated maximum capture latency or a durable barrier that
   prevents the next billable request until prior usage is durably captured;
   missing recognized usage within its watchdog interval triggers containment,
   and extractor failure fails containment closed.
2. **Committed/in-flight work:** a verified maximum count of queued,
   committed, and concurrent billable requests, multiplied by verified maxima
   for every billable request dimension.
3. **Post-kill work:** a validated provider cancellation/no-background-work
   guarantee, or reservation of the complete charge of every accepted request
   regardless of process death.
4. **Nested/fallback work:** every extension, tool, MCP route, fallback model,
   child agent, and nested run is disabled or has validated finite fanout,
   depth, model selection, request dimensions, and prices included in `R`.

Failure or absence of any obligation makes the tuple `unqualified` before
spawn. Deterministic test providers may carry fixture contracts to exercise
these branches, but their evidence never qualifies an external transport.

### 6.4. Request broker and confinement

The initial contained clients use a Rhei request broker that permits at most
one unsettled provider request per admitted invocation. The broker durably
captures and prices one response before releasing the next request, rejects a
request exceeding its verified dimensions, and refuses every provider/model or
fallback outside the tuple. The client receives only broker credentials and
network access to the broker; direct provider credentials, inherited
credentials, extension networking, alternate proxies, and unmediated provider
egress are unavailable.

Confinement must be demonstrated on Linux, macOS, and Windows. Merely removing
an environment variable, asking the client not to call the provider, or
detecting a direct call after it happened is insufficient. If portable
credential and egress confinement cannot be proved for an OS, that OS tuple
remains unqualified.

### 6.5. Initial real qualifications

The first required real tuples are restricted API-billed Codex `0.153.4` and
pi `0.84.1` launches through the broker, initially only their stated
`gpt-5.2-codex` request subset. Each client must independently provide the
binary/configuration, three-OS, model, provider billing/price, capture,
request-bound, post-kill, nested/fallback, credential, and egress evidence of
§6.1–§6.4. Neither tuple is qualified merely because this specification names
it or a fake provider passes.

Claude Code and every other tuple remain unqualified until their own complete
proof exists. Claude Code additionally needs usage before exit or a qualified
stop-at-cap with bounded residual. A refusal-only implementation, or an
implementation qualifying only one of Codex and pi, does not complete this
feature.

## 7. Settlement, crash recovery, and earned edges

Only evidence that is authoritative for provider billing, uniquely matches the
attempt identity, covers every billable dimension and reachable request, uses
the tuple's qualified prices, and is durably recorded may settle money below
`FWC`. Rhei's own complete capture may settle directly. External evidence is a
`reconcile` operation naming actor, time, evidence path/hash, and reason.
Delayed, partial, unsupported, unpriced, extractor-failed, write-failed, or
identity-conflicting evidence releases nothing while that status holds.

A failure before subprocess creation may release every unit when Rhei can prove
no provider-capable process or request started. Once start is confirmed or
ambiguous, the invocation unit is consumed. Ambiguous monetary exposure stays
at `FWC`. Provider refunds are new authoritative evidence only after they are
uniquely matched and final; attempt-counter refunds never constitute provider
evidence.

The travel reservation guarantees that work already admitted can apply its
earned edge even if another bound is exhausted when it exits. Rhei writes the
central transition and budget transition receipt idempotently. A crash between
the two is recovered by matching receipt ids: apply the missing half exactly
once without rerunning callbacks, fabricating a task outcome, or consuming a
second travel unit. A crash before spawn leaves a durable reservation; recovery
releases it only with proof no process could have started, otherwise it becomes
an ambiguous start.

Missing, unreadable, corrupt, forked, or identity-conflicting journals refuse
new admission Panta-wide. Completed invocations retain their actual results and
transitions; existing accounting write-failure behavior remains a warning with
partial coverage. Recovery never zeros unknown exposure. §FS-rhei-cost-accounting.11

## 8. Operator commands

All commands resolve the full Panta and its single ledger even when the target
is a member rhei or `--rhei` narrows display. Mutating commands require
`--reason <TEXT>` and record the authenticated local actor, UTC time, old/new
values, evidence hashes, and exact argv in an audited receipt.

```text
rhei budget init <TARGET> --invocations <N> --spend-micro <N> --currency <ISO> --reason <TEXT> [--history <PATH>]
rhei budget show <TARGET> [--rhei <ID>] [--format text|json]
rhei budget adjust <TARGET> [--invocations <N>] [--spend-micro <N>] --reason <TEXT>
rhei budget reconcile <TARGET> --invocation <ID> --evidence <PATH> --reason <TEXT>
rhei budget recover <TARGET> --journal <PATH> --reason <TEXT>
```

`init` validates bounds, assigns persistent identities, scans every member
accounting root, and imports lifetime history before making a ledger active.
Complete authoritative priced records become settled charges and consumed
invocations. Unknown, partial, unpriced, conflicting, or missing historical
exposure requires authoritative `--history` evidence; without it initialization
refuses and writes no fresh balance. Initialization is idempotent only when the
requested values and history digest exactly match the existing ledger.

`show` is read-only and reports the fields in §10. `adjust` changes allowance
ceilings, never consumption. A decrease below settled plus outstanding
exposure refuses; an increase cannot clear a breach or qualify a tuple.
`reconcile` settles one existing reservation under §7 and cannot raise an
allowance. `recover` accepts only a complete trusted copy whose identity and
hash chain verify and whose exposure is at least every intact reservation and
accounting record Rhei can find; it appends an audited recovery to both copies
and atomically installs the recovered journal. Deletion or a hand-authored
summary is not recovery.

Validation, `show`, and `rhei run --dry-run` never debit. Any command that
cannot verify the journal reports it as untrustworthy and performs no
allowance-affecting mutation.

## 9. Halt and breach behavior

Admission refusal starts no arm, applies no edge, writes no task result, and
leaves the ticket in its current state. `rhei run` continues independent work
whose admissions remain valid, then exits non-zero while any non-terminal
ticket is budget-halted. Every halted ticket names exactly one primary reason:
inner attempt/visit/poll exhaustion, ticket travel exhaustion, Panta invocation
exhaustion, Panta spend exhaustion, missing or stale qualification evidence,
ancestor-envelope exhaustion, or untrustworthy persistence. §FS-rhei-run.3

Already admitted work runs to its natural end under its finite timeout and
containment. Its completion condition, callbacks, accounting, result, and
reserved edge are handled normally. A containment stop is an invocation
outcome under its qualification; outer exhaustion is refusal to start another
invocation. Neither is described as the other.

If an authoritative final charge exceeds `FWC`, Rhei records the full actual
charge and a breach, invalidates that qualification tuple, and refuses new work
using it. The completed invocation keeps the outcome it earned. The operator
must install newly validated qualification evidence and recover/reconcile the
ledger; adding spend capacity alone does not clear the breach.

## 10. Visibility and machine surfaces

Text, TUI, JSON, run reports, `rhei cost`, `rhei summary`, and the dashboard
show, for both invocation and money allowances: ceiling, consumed, reserved,
remaining, currency, ledger health, and project identity. Ticket rows show
travel ceiling, applied, reserved, and remaining. Invocation rows show `T`,
`R`, `FWC`, settled charge, qualification grade/tuple/evidence hash, ancestor
reservation, and containment state. Inner time, attempt, visit, and poll bounds
remain visible alongside them.

JSON run events add `budget_snapshot`, `budget_reserved`, `budget_settled`,
`budget_released`, `budget_halt`, and `budget_breach` records. Each carries the
project, ticket, attempt and receipt identities that apply, exact amounts, and
a stable reason code. Human text names the same specific bound and a recovery
command; it never uses `failed`, `completed`, or `cancelled` for budget
availability. Live surfaces update on every receipt, not just exhaustion.

The durable run report records the starting and ending snapshot, every
reservation and settlement relevant to the run, every containment action,
qualification evidence, exact halt reason, and links to the journal and
accounting evidence. It distinguishes run spend, workspace accounting totals,
and Panta-lifetime allowance consumption.

## 11. Accounting contract

Accounting normalization, complete usage capture, and verified prices are
inputs to admission qualification and settlement. Post-exit accounting never
changes a completed invocation's result or selected transition. Pre-start
evidence determines whether the invocation exists at all. An extractor or
accounting write failure after admission preserves normal completion while
retaining the full reservation and refusing dependent future admissions that
can no longer be bounded. §FS-rhei-cost-accounting.4

`--prices` chooses an accounting price book only. It does not qualify a billing
contract, override the tuple's maximum prices, or make an unpriced route safe.
Every qualification price must be no smaller than the greatest provider price
that can apply over the reservation's validity interval; uncertainty refuses.

## 12. Migration, compatibility, and delivery proof

Old autonomous plans without a project allowance, thresholds, finite profile
travel, or a qualified tuple validate with actionable errors and refuse neural
launch. Migration uses `rhei budget init` and preserves imported lifetime
history. Read-only commands and externally launched worker-authority sessions
remain available. No compatibility path supplies unlimited bounds, resets
history, or admits a declared maximum.

The portable regression suite uses finite guards and synthetic usage to prove
permitted work and independent invocation, travel, and money halts; atomic
competing reservations and all-or-none fanout; retry/poll/refund distinctions;
nested ancestry; resume/reset/narrowed-run persistence; unknown history and
usage; corruption and ambiguous starts; idempotent settlement and earned-edge
recovery; audited adjustment; surface reasons; and each §6.3 failure path.
Fake providers prove engine behavior only.

Rollout additionally requires pinned real Codex `0.153.4` and pi `0.84.1`
provider evidence for every §6 obligation on Linux, macOS, and Windows. The
gate fails if either client, any OS, billing prices, confinement, or residual
bound is missing. This evidence must not purchase unbounded work: its provider
account and request fixture have their own finite external cap.

Delivery also updates the README quick start; language, authoring, usage,
budget/agent, accounting, run, and reset references; supported qualification
table; migration/recovery guide; and the Unreleased changelog with its PR
number. Documentation must say that non-neural infrastructure/shell costs and
subscriptions are outside the monetary promise and must list every currently
qualified tuple without implying others are safe.
