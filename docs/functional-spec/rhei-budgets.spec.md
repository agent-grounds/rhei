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

Identity assignment records the chosen UUID and live source binding before its
metadata write, then records installation. Interrupted assignment may reuse
that pending UUID; removal of an already installed UUID refuses. Binding a
second live source takes its source lock as well as the new document locks and
compares content. Conflicting live content refuses; disappearance of the old
source permits a move with the same history. Adopting an identical copy retains
every earlier live binding. Before mutation or admission, all registered live
sources are locked and compared, including sources outside the selected root;
matching the most recently adopted source is not sufficient. A binding is
retired only after its source is confirmed absent, never on an unreadable-source
error. The locks remain held through the mutation or admission transaction.
Identity receipts spend no units.

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

The project copy is checked against a lifetime history witness outside the
project: `$XDG_STATE_HOME/rhei/budget-authority/<project-uuid>/history.jsonl`,
falling back to `$HOME/.local/state` (or `%USERPROFILE%/.local/state`). This
witness contains the exact committed receipts, not a second spendable balance.
Its stable lock is acquired before the project sidecar. The state directory
must be absolute and outside the project; it is trusted operator storage and
must be preserved independently of project copies and resets. Moving between
hosts requires that same authority, not initialization from a copied journal.
Missing authority refuses; a project journal never bootstraps a lost witness.

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

Before accepting replay, the journal must exactly match its authority witness.
A complete but shorter prefix is lost history, not a valid balance. Append
syncs the witness first and the project copy second; a crash or write failure
between them refuses further operations until audited recovery installs the
complete witnessed history. Copies on one authority serialize on its lock;
after either copy advances, the stale copy must recover before mutation.
This detects project rollback, not an administrator rolling back the trusted
authority itself. Restoring that authority requires independent complete
history evidence; copying project files cannot establish it.

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

The descendant envelope is a property of the ancestor's qualification, not of
the child's request: it is whatever the bundle's validated delegation graph
proved under §6.3, and zero wherever nested and fallback routes are disabled —
which is what the initial contained format does. The ledger authenticates the
ancestry token by finding that reservation in this project's own journal: it
must exist, have started, not yet be released or settled, carry a descendant
envelope above zero, and carry a finite deadline the descendant does not
exceed. A token naming anything else is `ancestor_unavailable`, and a
descendant whose `FWC` would take the ancestor's descendants past that envelope
is `ancestor_envelope_exhausted`. Both refuse before spawn, and replay
re-derives every ancestry claim from the chain rather than trusting a cached
balance.

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
overflow. Unless qualified billing terms prove a shared rounding boundary,
each billable dimension is rounded up separately before their maxima are added.
The full `T + R` is reserved before spawn. A usage stream or local
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

The restricted HTTP adapter accepts authenticated `POST /v1/responses` with
bounded JSON bodies, fixed provider routing, and one outstanding request. It
forbids client-selected upstream URLs, redirects, proxy overrides and credential
headers. The broker commits request ownership before forwarding and durably
captures the complete JSON/SSE response before acknowledging completion. An
independent watchdog closes admission and requests teardown on deadline or
capture expiry even if the HTTP worker is blocked. An HTTP adapter is not an OS
confinement proof and cannot alone authorize a provider-capable spawn.

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

A fanout group allocates one `transition:<uuid>` identity and one travel owner
at reservation. Its central `task from@to` log entry appends that identity and
`reservation:<uuid>` owner as separate tokens, while older entries remain
readable. All readers treat only the second token as the movement. Ending one
arm records invocation end without releasing the group's travel prematurely.

Only evidence that is authoritative for provider billing, uniquely matches the
attempt identity, covers every billable dimension and reachable request, uses
the tuple's qualified prices, and is durably recorded may settle money below
`FWC`. Rhei's own complete capture may settle directly. External evidence is a
`reconcile` operation naming actor, time, evidence path/hash, and reason.
Delayed, partial, unsupported, unpriced, extractor-failed, write-failed, or
identity-conflicting evidence releases nothing while that status holds.

At invocation completion and before resumed admission, the driver checks the
ledger's `provider-evidence/<reservation-uuid>.json` inbox and its `.sig` file.
These use the same signed envelope and mutation-time checks as `reconcile`.
A missing or rejected export retains full exposure; a valid export publishes
the ordinary settlement or breach receipts without changing the earned outcome.

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
refuses and writes no fresh balance. That refusal may retain only the durable
initialization intent and report its chosen project identity and UTC import
cutoff, so an authority can produce evidence for that exact scope; rerunning
the identical command with `--history` verifies and attaches the evidence
before any identity document or ledger is installed. Initialization is
idempotent only when the requested values and history digest exactly match the
existing ledger.

Fresh initialization validates every owning document and existing identity
under sorted metadata locks before writing an allowance. It durably saves an
initialization intent with the original and intended document bytes, chosen
identities, bounds, and audit information, then installs those documents and
activates the initial receipt. Retrying an interrupted initialization resumes
that same intent only when each document matches its original or intended
bytes. It cannot choose a new identity or allowance. Changed input refuses
without activating financial state; ordinary invalid UUID input leaves no
intent or active allowance. An existing identity without a journal remains a
refusal unless this exact unfinished intent accounts for it.

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

### 8.1. Authoritative evidence envelope

History import and reconciliation accept canonical UTF-8 JSON signed by a
provider/account authority pinned by the qualified billing contract. The
closed `rhei.provider-evidence.v1` envelope names its `project_id`, provider,
account, currency, billing and price contracts, complete `qualification` tuple,
`qualification_evidence_hash`, coverage start/end, and a
`complete` finality flag. Its `records` array names one invocation identity,
provider request identity, reservation identity when reconciliation is being
performed, accepted-at and finalized-at times, and final integer
`charge_micro`. A detached Ed25519 signature covers the exact envelope bytes;
the evidence hash recorded by the ledger covers those same bytes.
For `--evidence <PATH>` and `--history <PATH>`, the detached signature is the
64-byte lowercase-hex file at `<PATH>.sig`; no key is accepted from the
command line or project.

The verifier rejects a wrong project, provider/account, currency, contract,
coverage interval, signature, duplicate identity, non-final record, missing
reservation match, or an envelope whose `complete` flag is false. History
initialization additionally requires coverage from the declared beginning of
the account/project association through the import cutoff. Reconciliation
requires exactly one final record per durably committed broker request, all
matching the named reservation and attempt. Authority over one tuple or interval confers no
authority over another. §REQ-bounded-neural-work.3

The opaque verification result retains the authenticated project, complete
qualification tuple and evidence hash, account, currency, contracts, signing
key fingerprint, and coverage interval through the financial mutation. Import
rechecks the destination project and currency before creating persistence;
reconciliation rechecks the reserved qualification and durable broker request
set while holding the journal lock. Copying an expected request identifier from
the supplied evidence is not a match. Every committed request must have exactly
one final charge, including requests whose local response capture was lost.
Multiple requests in one invocation are summed once; their shared invocation
identity is not a duplicate invocation. Timestamps use RFC 3339 with an explicit
offset: coverage start <= acceptance <= finality <= coverage end, and coverage
end cannot be in the future. Test signing keys exist only in test compilation.

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

An external authority directory retains a tuple invalidation shared across
projects. Admission holds sorted tuple locks through the reservation append;
settlement holds the same tuple lock while publishing a write-ahead invalidation
and the local breach. The invalidation includes actual charge and evidence hash
so an interrupted local write cannot let another project reuse the tuple.
Repeating identical evidence is idempotent. Neither project relocation nor an
allowance increase removes that invalidation.

## 10. Visibility and machine surfaces

Text, TUI, JSON, run reports, `rhei cost`, `rhei summary`, and the dashboard
show, for both invocation and money allowances: ceiling, consumed, reserved,
remaining, currency, ledger health, and project identity. Ticket rows show
travel ceiling, applied, reserved, and remaining. Invocation rows show `T`,
`R`, `FWC`, settled charge, qualification grade/tuple/evidence hash, ancestor
reservation, and containment state. Inner time, attempt, visit, and poll bounds
remain visible alongside them.

JSON run events add `budget_snapshot`, `budget_reserved`, `budget_started`,
`budget_contained`, `budget_settled`, `budget_released`, `budget_halt`, and
`budget_breach` records. Each carries the
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

Qualification acquisition uses a separately verified `rhei.qualification-probe.v1`
grant from a build-pinned provider/account cap authority. It binds the complete
candidate tuple, account, currency, external cap identity and amount, maximum
launches, per-case deadline, authorization expiry, finality deadline, capture-size
limit, and hashed prerequisite artifacts. A reviewed transport qualification is
not required to acquire its own evidence, but an independently enforced finite
external cap is. The finite runner enumerates allowed/forbidden routes,
concurrency and missing-usage cases, request-phase kills and escaped descendants;
it never retries automatically. The authority consumes the complete grant once
in an external durable receipt before the first case. Each case receives an
opaque, single-use launch permit; changing the output directory cannot replenish
the grant. Native backends own isolation and idempotent
complete teardown. Raw artifacts and per-case receipts remain evidence for an
independent reviewer, never automatic registry admission. Missing cap authority
or native backends refuses before any real-client launch.

The Linux acquisition backend uses a hash-verified bubblewrap executable and
a complete, signed inventory of its read-only root image. It requires private
user, mount, PID, IPC, network and UTS namespaces, disables additional user
namespaces, clears inherited environment and capabilities, and runs its
supervisor as PID 1. Only the dedicated broker Unix socket crosses into that
image; host homes, workspaces, credential stores and network namespaces do not.
The supervisor relays private loopback HTTP to that socket and exits on client
completion or deadline, destroying the PID namespace including detached
descendants. The native runner captures bounded output and tears down on every
exit path. Signed per-case argv and configuration must independently demonstrate
the pinned client's request compatibility; this backend alone qualifies neither
client and supplies no macOS or Windows proof.

Acquisition preserves the grant signature, prerequisite bytes and native result
hashes. A separately signed account-finality artifact binds the grant, exact
tuple, full result hash, account, currency, complete accepted request list and
final charges through the authorized finality deadline. An independent review
signature binds that artifact and the raw run results. Missing, oversized,
expired or mismatched artifacts leave acquisition incomplete; collection neither
invents a final bill nor registers a release tuple.

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
