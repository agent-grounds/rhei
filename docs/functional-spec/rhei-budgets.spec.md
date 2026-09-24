# FS-rhei-budgets: Bounded ticket travel and project invocations

Every ticket may make a finite number of moves, and every project may be
admitted a finite number of neural starts. Both bounds are in force on every
plan and every machine without anyone declaring anything, both are visible with
their value and their source before the first agent starts, and both stop the
work they bound where it stands rather than inventing an outcome for it. This
is the user-visible realization of [§REQ-bounded-neural-work](../requirements/bounded-neural-work.spec.md#req-bounded-neural-work-every-unit-of-neural-work-is-bounded-before-it-starts); [§AR-neural-admission](../architecture/neural-admission.spec.md#ar-neural-admission-one-serialized-account-beneath-every-neural-start)
owns the runtime boundary beneath it.

Provider-billed spend is not bounded here. That is a separate obligation on
`agent-grounds/rhei#107`, and nothing in this specification qualifies a
transport, brokers a request, confines a process, or settles money.

## 1. The two counts

**Travel** is the number of applied transitions one ticket may make over the
lifetime of its identity. It is a property of the ticket, it is persisted with
the ticket, and it survives `rhei reset`, a copy, a move, and re-instantiation.

**Invocations** are the neural starts a project may be admitted. They are a
property of the **project** — one durable account shared by every rhei of the
project, every member added later, every concurrent `rhei run`, and every nested
runtime. A bare rhei is the single rhei of its implicit project ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)).

Both are integer counts. Neither is a duration and neither is an amount of
money. Neither is `visits:`, `attempts:`, or a poll counter: those bound a state
entry, and they may refresh or increment without touching either count
([§REQ-bounded-neural-work.1](../requirements/bounded-neural-work.spec.md#1-the-four-levels)).

At every durable boundary, for each count:

```text
consumed + outstanding <= effective bound
```

## 2. Where a bound comes from

A requested value resolves **most specific first**, exactly as every other
setting resolves ([§FS-rhei-agents.1.1.1](rhei-agents.spec.md#111-defaults)):

```text
plan or profile  >  project settings  >  machine-global settings  >  built-in
```

The machine's value is then applied as a **ceiling**:

```text
effective bound = min(resolved value, machine value)
```

Where machine-global settings configure no value for a dimension, the built-in
default **is** the machine value and therefore the ceiling. There is no
unconfigured state in which a count dimension is unbounded
([§REQ-bounded-neural-work.2](../requirements/bounded-neural-work.spec.md#2-bounded-by-default-refusing-nothing-that-runs-today)).

Requesting more than the machine allows is **never a validation refusal**. The
plan is valid, the effective bound is the machine's, and every surface that
reports the bound says so. Refusing it would make a template invalid on the
machine that did not write it; honoring it would let a template raise the cap of
the machine that pays for it.

The clamp is enumerated to exactly these two count dimensions. `agent_timeout`,
`attempts:`, `visits:`, `poll.max_attempts`, and every other setting keep their
existing resolution and are not clamped by anything here.

### 2.1. The settings keys

Three keys in the `defaults` block of global or project settings
([§FS-rhei-agents.1.1.1](rhei-agents.spec.md#111-defaults)):

| Key | Type | Built-in | Bounds |
|---|---|---|---|
| `transition_limit` | positive integer | `80` | applied transitions per ticket identity |
| `invocations_per_day` | positive integer | `200` | admitted neural starts per project per UTC day, in the window contract |
| `invocation_lifetime_max` | positive integer | `6000` | the ceiling on an explicit lifetime allowance |

The three built-in values are measured rather than chosen, and the measurement —
the definition of a healthy history, the observed maxima, the multiplier, and
what the evidence could not see — is recorded in [§REQ-bounded-neural-work.7](../requirements/bounded-neural-work.spec.md#7-the-defaults-are-measured-not-chosen) so
that a re-measurement can supersede them honestly.

`invocation_lifetime_max` is a ceiling, not an allowance: nothing consumes it
and no project receives it. It clamps `rhei budget init` and every `adjust`.

A value that is zero, negative, fractional, or the word `unlimited` is a
settings error naming the key.

### 2.2. `transition_limit` on a profile

A `profiles.<name>` entry may declare `transition_limit` alongside `initial` and
`allowed` ([§FS-rhei-states.8](rhei-states.spec.md#8-profiles)):

```yaml
profiles:
  reviewed:
    initial: draft
    allowed: [draft, agent-review, completed, cancelled]
    transition_limit: 40
```

The field is **optional**. A profile that declares none resolves the settings
chain above, so a machine that has never been configured still bounds every node
of every profile. A declared value is an inner value like any other: honored
when it is at or below the machine's ceiling, clamped and reported when it is
above.

Every node resolves a profile and therefore a travel bound, including the
virtual project root.

### 2.3. Provenance is two-valued

A reported bound carries two facts, because they answer different questions: the
**value source** is whoever set the requested value, and the **limiting source**
is the machine, named only when the machine clamped it.

```text
transition_limit: 80 (built_in)
transition_limit: 40 (plan)
transition_limit: 100 (requested 500 by the plan, limited by machine settings)
```

The value sources are `built_in`, `machine`, `project`, and `plan` — `plan`
covering anything the plan's own state machine declares, such as a
`profiles.<name>.transition_limit`, because from the operator's side the plan is
what asked. `machine` is both a value source and, where it clamps, the limiting
source; a report never collapses the two, because "the machine set this" and
"the machine lowered this" send a reader to different files.

The clamped form names the requester and the limiter in one line, and is the
same sentence on every surface that reports a bound:

```text
<key>: <effective> (requested <N> by the <value source>, limited by machine settings)
```

## 3. The invocation contracts

A project's account holds exactly one contract at a time. They bound different
quantities and no surface describes one as the other.

### 3.1. The window contract

The default, and what a project has until it is explicitly initialized. It
bounds the invocations admitted **during the current window**, which is one UTC
calendar day beginning at `00:00:00Z`. It makes no claim about the project's
lifetime total.

The bound is `defaults.invocations_per_day`, resolved and clamped by [§FS-rhei-budgets.2](rhei-budgets.spec.md#2-where-a-bound-comes-from).

### 3.2. The lifetime contract

What `rhei budget init` establishes. It bounds the invocations **ever admitted**
by the project, is persistent, and is never renewed by the passage of time. Its
capacity changes only through an audited `adjust` ([§FS-rhei-budgets.10](rhei-budgets.spec.md#10-rhei-budget)).

An explicit allowance is clamped by `defaults.invocation_lifetime_max` at `init`
and at every `adjust`.

The two contracts are mutually exclusive, and an explicitly initialized account
does not revert to the window contract. Reversion is **not offered**: the two
bound different quantities, so any mapping from a lifetime consumption to a
window consumption either strands the history or charges today for work done
months ago. If it is ever wanted its shape is fixed now — an audited `revert`
receipt that keeps the whole lifetime history and opens the current day already
fully consumed, so that reversion costs the rest of the day and can never mint.

### 3.3. The window is a key, not a refill

A day's capacity is never written, granted, or reset. Every reservation is
stamped with the UTC calendar day it was admitted in, and the remainder is
derived by replay:

```text
remaining = limit - consumed(today) - outstanding(today)
```

There is no renewal event, so there is nothing to forge, replay twice, or lose.
A reservation stamped `2026-09-24` stays stamped: midnight neither enlarges it,
revives it, nor moves its deadline. The new day simply has its own key.

The boundary is `00:00:00Z` and not local midnight, because daylight saving
makes a local day 23 or 25 hours long and the answer would differ per machine,
which [§REQ-cross-platform.2](../requirements/cross-platform.md#2-parity) forbids. It is a calendar day and not a rolling
24-hour window, because a rolling window would have to retain every timestamp
forever and could not answer "when does this renew?" in a halt message.

The clock is the system clock, **guarded by the highest day key the journal has
ever recorded**. A clock moved backwards keeps drawing on the recorded day and
mints nothing. A clock moved forwards does open a new day; that is the honest
residual, and an operator who can set the machine's clock can already set the
machine's settings key.

#### 3.3.1. Reading the clock from the environment

`RHEI_BUDGET_NOW`, when set, supplies the instant admission uses in place of the
system clock. Its value is an RFC 3339 timestamp with an explicit offset; a
value that does not parse is an error naming the variable, never a silent
fallback to the system clock.

It exists because the window is otherwise not testable on a portable fixture
without waiting for midnight ([§REQ-cross-platform.4](../requirements/cross-platform.md#4-portable-fixtures)), and it is given **exactly
the authority the system clock has and no more**: the highest-day-key guard of
[§FS-rhei-budgets.3.3](rhei-budgets.spec.md#33-the-window-is-a-key-not-a-refill) applies to it unchanged, so it can open a new day and can never un-spend a
recorded one.

## 4. What spends a count

### 4.1. Travel

One unit per **applied edge** of the ticket, whoever selected it: an agent
outcome, a program's declared route, a callback redirect, an ordinary self-loop,
or a person running `rhei transition`. The transition ledger is authority-
agnostic ([§FS-rhei-transition-cmd.3](rhei-transition-cmd.spec.md#3-behavior)), and so is this charge — a manual path that
moved for free would be the bypass.

A fanout consumes **one** travel unit for the ticket's one applied edge,
however many arms it starts.

These spend no travel: a ticket's initial placement, a poll self-loop that is a
wait ([§FS-rhei-states.2.2](rhei-states.spec.md#22-semantics)), a completion that selects no edge, a stall, and a
refused admission.

### 4.2. Invocations

One unit per **admitted neural start**: every first spawn, every retry, every
neural poll attempt, and every arm of a fanout.

**Residual: the run loop admits a fanout's arms individually.** The account
supports the grouped, all-or-none form — either every arm reserves its unit and
the ticket reserves its travel unit, or no arm starts — and that is what an
account-level reservation of several arms does. The run loop does not yet use
it: it admits one resolved invocation at a time, so a fanout that cannot afford
every arm **starts the arms it can afford and refuses the rest**. The ticket
then waits on its own state for the arms that did not start. Nothing is leaked
and the visit is resumable: the first arm reserves the ticket's one travel unit,
later arms take an invocation unit alone, an edge that was never applied
releases the travel unit, and the next run that has capacity starts only the
missing arms before the ticket advances.

A retry costs **zero travel and one invocation**. A fresh visit refreshes
`attempts:` and never the project account.

The four non-spend exceptions of [§FS-rhei-agents.3.2.3](rhei-agents.spec.md#323-attempt-budget) — run interruption, a
withheld supervisor release edge, the poll-state exemption, and a recognized
provider-limited invocation — are exceptions to the **inner** attempt counter
and do not propagate outward. An attempt given back is an attempt, never an
invocation.

### 4.3. What spends neither

A program or a callback that Rhei starts is not a neural start and is not
charged an invocation; its applied edge still spends the ticket's travel unit
like any other. A program that calls a provider directly is the residual gap of
[§REQ-bounded-neural-work.6](../requirements/bounded-neural-work.spec.md#6-the-residual-gap).

`rhei run --no-agent` and `--no-program` start nothing and charge no invocation;
edges taken by callbacks still spend travel.

## 5. The project account

### 5.1. Where it lives

```text
<project-root>/.agent-grounds/rhei/budgets/<uuid>/
  journal.jsonl
  journal.jsonl.lock
```

For a bare rhei, `<project-root>` is its execution root. The directory is
deliberately **outside `runtime/`**, so that [§FS-rhei-reset.2](rhei-reset.spec.md#2-behavior)'s wholesale
deletion of `runtime/` cannot reach it. It is machine state rather than authored
content, and `rhei init` seeds `budgets/` into the project's own `.gitignore`
([§FS-rhei-init.3](rhei-init.spec.md#3-ignore-rules)).

### 5.2. The journal

Every line is one UTF-8 JSON object with schema `rhei.budget.receipt.v1`,
carrying the project identity, a contiguous `sequence` starting at one, a
path-independent `receipt_id`, the SHA-256 of the exact preceding line's bytes,
the UTC instant, the actor, the receipt `kind`, the `window` day key where one
applies, and the kind's payload.

The `kind` vocabulary is closed: `initialize`, `identity`, `reserve`, `start`,
`release`, `transition`, and `adjust`. Replaying a receipt id with identical
content is idempotent; replaying it with different content is corruption. An
unknown kind is retained and makes this build read-only until it understands it.

A ticket's identity is bound by **source path and content hash**, so several
live sources are tolerated only while their bytes agree. That is what makes a
copied or moved ticket keep its travel, and what makes two documents claiming
one identity with different content a conflict rather than a fork.

The binding is also what a ticket carrying no identity is resolved *from*. A
ticket whose document names no identity is not therefore a new ticket: the
ledger is asked for a binding on this source path and display id first, and one
is minted only where there is none. That closes the one door left open by the
fact that a receipt and the document naming it are two writes rather than one —
a document whose write was lost after its receipts were durable would otherwise
be handed a second identity, and with it a second travel bound. Deleting
`budgetTicketId` by hand is the same case and gets the same answer: the history
comes back. The display id is part of the key, because one document holds every
ticket of a rhei and a binding matched on the path alone would hand one ticket's
travel to its sibling.

### 5.3. The witness

A byte-identical copy of the committed receipts lives at
`$XDG_STATE_HOME/rhei/budget-authority/<uuid>/history.jsonl`, falling back to
`$HOME/.local/state` (or `%USERPROFILE%\.local\state`). It is keyed by the
project uuid and records every canonical root it has seen, so an absent project
directory is still recognized.

Where that directory is, is the operator's. The one place the witness may **not**
be is inside the account it witnesses — a chain verifying against itself
verifies nothing — and that is what Rhei refuses. It does not require the state
directory to be outside the project.

The witness is not a second spendable balance. It exists so that an accidentally
lost tail is distinguishable from a fresh project: deleting the journal cannot
recreate capacity. Deleting the journal **and** the witness is the stated
residual of this design. Where an operator's state directory happens to sit
under a project root, that residual narrows to one key: both copies are in the
one tree, so removing the tree removes both.

### 5.4. Absent, damaged, adopted

Three states, and the specification distinguishes them by name because they are
answered differently:

**Absent** — no `budgets/` directory and no witness claiming this canonical
root. This is **lawful and silent**. The first admission mints the uuid, creates
the directory and the witness, and writes the `initialize` receipt recording the
resolved defaults, all inside that same atomic admission. No command is
required, no prompt is shown, and establishment mints nothing.

**Damaged** — a witness exists for this root but the journal is absent,
truncated, or its hash chain does not verify. New work is **refused**, naming
both paths and saying that the journal is restored by copying the witness back.
No repair command is added here.

**Adopted** — the journal is present and internally valid but this machine has
no witness: a project cloned from git, or the same project on a new machine. The
witness is written from the journal's own bytes and the run proceeds. This mints
nothing, because the consumed counts travelled with the journal. It is a
deliberate loosening: refusing every fresh clone would refuse work that runs
today, which [§REQ-bounded-neural-work.2](../requirements/bounded-neural-work.spec.md#2-bounded-by-default-refusing-nothing-that-runs-today) forbids. **The witness catches an
accidentally lost tail, not a hand-edited ledger**, and that is the whole of
what it claims.

## 6. Admission

### 6.1. The transaction

Immediately before every neural spawn, one boundary performs one ordered,
serialized transaction:

1. resolve the project, the ancestor reservation where there is one, and every
   bound in force with its provenance ([§FS-rhei-budgets.2](rhei-budgets.spec.md#2-where-a-bound-comes-from)), and **read** the ticket's
   identity from its document — reporting its absence rather than inventing one,
   because nothing may be minted before the ledger has been asked;
2. lock and verify the account, replay it, derive the current consumed and
   outstanding amounts for the active contract ([§FS-rhei-budgets.3](rhei-budgets.spec.md#3-the-invocation-contracts)), and
   **settle** the ticket identity against what was replayed: the document's own,
   else the binding the ledger already holds for it ([§FS-rhei-budgets.5.2](rhei-budgets.spec.md#52-the-journal)), else a
   fresh one;
3. check one travel unit for the ticket and one invocation unit per arm against
   the effective bounds and against every ancestor envelope;
4. append and durably sync one reservation carrying every unit, or refuse and
   append nothing;
5. only then create the subprocess.

One account held exclusively is what serializes competing processes, parallel
arms, and nested runtimes. Two racing `rhei run` processes cannot both take the
last unit.

A settled identity is written into the document only by step 4 succeeding, under
the document lock step 1 took and has held since. So the identity follows
**spending**: an admission that refused leaves the authored document
byte-identical, and a lost write is a recoverable state rather than a fresh
history, because step 2 will find the binding again.

### 6.2. Reserve, then settle

A reservation becomes **consumed** once a start is recorded, confirmed or
ambiguous, and is never settled downward. `record_start` is appended immediately
before the spawn call and refined after it returns.

There is exactly **one lawful release** of a reserved invocation unit: engine-
side proof that no process could have started. That proof is the **absence of a
start record** for a reservation whose owning run is gone — established by
acquiring that run's execution-root lock. A reservation found outstanding with
no start record and a free execution-root lock is released by the next admission
on the same account, with `proof: "no_start_record"`. A reservation with a start
record is settled consumed, ambiguous or not, forever.

Recovery therefore needs no command and no new lock: it is one step inside an
admission that already holds the account exclusively.

No event downstream of an admitted start, and no accounting or observational
record, can authorize, refund, or reverse a consumed invocation unit or an
applied travel unit ([§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)).

### 6.3. What never debits

`rhei validate` and `rhei run --dry-run` ([§FS-rhei-run.4](rhei-run.spec.md#4-dry-run)) resolve and report
every bound and preview what a pass would cost. They take no lock that mutates,
append no receipt, and debit nothing.

## 7. Nested runs

A nested `rhei run` inherits the project and its one account through an
authenticated ancestry path, and **opens no balance of its own**. Every
descendant admission charges the shared account and must additionally fit its
ancestor's reservation envelope and deadline.

An absent, finished, envelope-less, or exhausted ancestor is a typed refusal
before spawn, naming which of the four it is. Each nested neural start is
charged exactly once. A window renewal makes capacity available to new
admissions only: it never enlarges, revives, or extends the deadline of a
reservation an ancestor already holds.

Ancestry is the one door through which anything a program does is counted
([§REQ-bounded-neural-work.6](../requirements/bounded-neural-work.spec.md#6-the-residual-gap)).

### 7.1. How the descriptor reaches a nested run

`RHEI_BUDGET_PARENT_RESERVATION` is set in the environment of every agent Rhei
starts, naming the reservation that invocation was admitted on. A `rhei run`
started inside that agent reads it and places its own admissions under that
ancestor.

It carries no credential, because there is nothing to credential: a descendant
charges the same account through the same locks, and the ledger is what decides
whether the name it was given buys anything ([§AR-neural-admission.6](../architecture/neural-admission.spec.md#6-ancestry)). A value
naming a reservation this project's journal does not hold, or one that is not
outstanding, is `ancestor_unavailable` before any spawn — which is why forging
it buys nothing rather than buying a fresh balance.

## 8. Exhaustion

A refused admission starts no arm, applies no edge, writes no task result, and
leaves the ticket in the state it is in with its artifacts intact. This is the
existing stall contract of [§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop) and not a new one: `rhei run`
continues with the other claimable tickets, and the run exits non-zero once a
pass makes no progress, naming every halted ticket.

The halt names, in one place:

- the **dimension** — ticket travel, or project invocations;
- the **effective bound** and its **value source**, plus the machine as the
  **limiting source** when the value was clamped ([§FS-rhei-budgets.2.3](rhei-budgets.spec.md#23-provenance-is-two-valued));
- **consumed**, **outstanding**, and **remaining**;
- the **accounting mode** — window, with its day key, or lifetime;
- exactly **one remedy**, the one that raises the limiter that actually stopped
  the work: the machine-global settings key when the ceiling limits, the audited
  `rhei budget adjust` when an explicit lifetime allowance limits, and the
  **renewal instant** when the window limits.

It never offers an inner value the ceiling would clamp: telling an operator to
raise a plan field that cannot take effect sends them to the wrong file.

The labels are fixed, so that one halt reads the same on every surface:

```text
error: ticket 'plan.1' has spent its travel bound
       dimension:   ticket travel
       bound:       4 (machine)
       consumed:    4  outstanding: 0  remaining: 0
       mode:        per ticket identity
       to raise it: set `defaults.transition_limit` in the machine settings file
```

A clamped bound carries the requester as well, in the one line of
§FS-rhei-budgets.2.3, and the remedy still names the limiter rather than the
requester:

```text
       bound:       100 (requested 500 by the plan, limited by machine settings)
       to raise it: set `defaults.transition_limit` in the machine settings file
```

A window-limited halt replaces the remedy with the instant the window renews,
because nothing an operator does is needed and saying otherwise would send them
to a settings file for a wait:

```text
error: project 'panta' has spent today's invocation capacity
       dimension:   project invocations
       bound:       200 (built_in)
       consumed:    200  outstanding: 0  remaining: 0
       mode:        window (2026-09-24Z)
       renews at:   2026-09-25T00:00:00Z
```

## 9. Visibility

Every bound in force is visible with its value and its source **before** capacity
is spent, on every surface that already shows autonomous work:

- **`rhei validate`** reports each bound in force with its value and provenance,
  on the warning channel and in the report order of [§FS-rhei-validate.4](rhei-validate.spec.md#4-behavior). It
  never fails a plan for asking above the ceiling.
- **`rhei run`** text and TUI show each bound with consumed, outstanding, and
  remaining amounts, updating as receipts are written rather than only at
  exhaustion ([§FS-rhei-run-tui.1.1](rhei-run-tui.spec.md#11-event-surface)).
- **`rhei run --json`** carries the same facts as records with stable reason
  codes ([§FS-rhei-run-json.2.1](rhei-run-json.spec.md#21-records)).
- **the run report** records the starting and ending snapshot of both counts,
  every reservation the run made, and the exact halt reason
  ([§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout)).

Where cost accounting also exists for a run, the two are reported side by side
and neither is described as the other: accounting is observational, and where
its extraction fails the counts are unaffected.

## 10. `rhei budget`

```text
rhei budget init <TARGET> --invocations <N> --reason <TEXT>
rhei budget show <TARGET> [--rhei <ID>] [--format text|json]
rhei budget adjust <TARGET> --invocations <N> --reason <TEXT>
```

All three resolve the whole project and its single account even when the target
is one member rhei, and `--rhei` narrows display only.

`init` moves the project from the window contract to the lifetime contract with
the given allowance, clamped by `defaults.invocation_lifetime_max`. It is
optional: a project that never runs it is bounded by the window contract and is
never refused for not having run it.

`show` is read-only and reports, for both counts, the effective bound, its value
source and limiting source, consumed, outstanding, remaining, the contract in
force, the window day key where one applies, the account's health, and the
project identity.

`adjust` changes the allowance and never consumption. Every adjustment is
audited: it records the actor, the UTC instant, the old and new values, the
reason, and the exact argv. A request **above** the ceiling is clamped and
reported, never refused for being high. A request **below** `consumed +
outstanding` is refused, because it would put a project in deficit for work
already done.

Lowering the machine's ceiling is always lawful and is not an `adjust`: it
erases no recorded consumption and rewrites no receipt. It removes headroom
until the effective bound again exceeds consumption — by a raise, or, in window
mode, by the next window under the new ceiling.

A command that cannot verify the account reports it as damaged ([§FS-rhei-budgets.5.4](rhei-budgets.spec.md#54-absent-damaged-adopted)) and
performs no mutation.

## 11. What does not change

| Seam | What happens |
|---|---|
| a plan with no budget fields | validates and runs exactly as today, under the three built-in defaults, which `rhei validate` and the run print with source `built_in` |
| `agent_timeout`, `attempts:`, `visits:`, `poll.max_attempts` | untouched, and not clamped |
| `rhei reset` | still deletes `runtime/` and restores authored state; the account is not under `runtime/` and survives, and so does the ticket's budget identity ([§FS-rhei-reset.2](rhei-reset.spec.md#2-behavior)) |
| `rhei reset --rhei` | removes what is keyed by an in-scope ticket id and **no receipts**: a narrowed reset that dropped a ticket's travel would make `--rhei` the faucet ([§FS-rhei-reset.2.1](rhei-reset.spec.md#21-narrowed-reset---rhei)) |
| `rhei run --rhei` | narrows candidates only. One account, one balance, whatever the selection ([§FS-rhei-run.2.5](rhei-run.spec.md#25-project-scope---rhei)) |
| `rhei run --dry-run` | reports the bounds and what the pass would cost, and spends nothing |
| detached and concurrent runs | the same account, serialized by the same lock ([§FS-rhei-run-headless](rhei-run-headless.spec.md#fs-rhei-run-headless-detached-runs)) |
| snapshots, appended work, live member admission | a new member joins the run and the one account; joining creates no capacity |
| cost accounting | untouched and still observational ([§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)) |
| git | the account is machine state and is gitignored; a project that commits it anyway is *adopted* on the next machine ([§FS-rhei-budgets.5.4](rhei-budgets.spec.md#54-absent-damaged-adopted)) |

Four things do change for someone, and they are the point rather than a side
effect:

1. A project that starts more than 200 agent processes in a UTC day now stops.
   In this workspace's whole history the busiest day was 58.
2. A ticket that makes more than 80 moves now stops. The largest ever seen is 20.
3. `rhei transition` by hand now spends one travel unit. Silence becomes a count.
4. A **damaged** account is an error where there was previously no account at
   all. A **missing** one is not: absence is lawful and silent.

And one thing stops being true: `rhei reset` no longer returns a ticket to a
state from which it can consume without limit.

## Related Specifications

- [Requirements — Bounded Neural Work](../requirements/bounded-neural-work.spec.md) — why the bounds exist and where the defaults came from
- [Neural Admission Architecture](../architecture/neural-admission.spec.md) — the boundary, the lock order, and the receipt chain
- [Run Specification](rhei-run.spec.md) — the execution loop the admission checkpoint sits in
- [Agents Specification](rhei-agents.spec.md) — the inner timeout and attempt budgets
