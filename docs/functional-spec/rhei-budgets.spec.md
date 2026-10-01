# FS-rhei-budgets: Bounded ticket travel, project invocations, and a day's spend

Every ticket may make a finite number of moves, every project may be admitted a
finite number of neural starts, and every project may be charged a finite
amount of measured spend in a day. All three bounds are in force on every plan
and every machine without anyone declaring anything, all three are visible with
their value and their source before the first agent starts, and all three stop
the work they bound where it stands rather than inventing an outcome for it.
This is the user-visible realization of [§REQ-bounded-neural-work](../requirements/bounded-neural-work.spec.md#req-bounded-neural-work-every-unit-of-neural-work-is-bounded-before-it-starts); [§AR-neural-admission](../architecture/neural-admission.spec.md#ar-neural-admission-one-serialized-account-beneath-every-neural-start)
owns the runtime boundary beneath it.

The spend bounded here is **measured** spend, read from the cost accounting
record a completed invocation already produces §FS-rhei-cost-accounting.
Nothing in this specification qualifies a transport, brokers a request,
confines a process, or settles money: spend as a provider bills it is the
stricter grade and remains an obligation on `agent-grounds/rhei#107`. What is
added here is a ceiling over what this engine's own records say was spent
§REQ-bounded-neural-work.6.

## 1. The two counts and the day's spend

**Travel** is the number of applied transitions one ticket may make over the
lifetime of its identity. It is a property of the ticket, it is persisted with
the ticket, and it survives `rhei reset`, a copy of its document, and a
relocation of its document. A ticket's identity follows its **id**, not its
bytes: the account binds the pair (full rhei-qualified display id, metadata
file) to a ticket uuid, so a ticket placed or re-parented under a pair the
account has never bound is a genuinely new ticket identity with a travel bound
of its own ([§REQ-bounded-neural-work.4](../requirements/bounded-neural-work.spec.md#4-nothing-creates-capacity)), and a pair the account has already
bound keeps its history whatever placed it, including a ticket deleted and
re-placed at the same id ([§FS-rhei-budgets.5.2](rhei-budgets.spec.md#52-the-journal)). A ticket's document never
asserts an identity the account has not settled; `rhei instantiate` refuses a
template that declares one ([§FS-rhei-library.5](rhei-library.spec.md#5-placement-and-ticket-identity)).

**Invocations** are the neural starts a project may be admitted. They are a
property of the **project** — one durable account shared by every rhei of the
project, every member added later, every concurrent `rhei run`, and every nested
runtime. A bare rhei is the single rhei of its implicit project ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)).

**Spend** is the measured cost a project may be charged during one UTC day.
Like invocations it is a property of the **project**, and the same one durable
account holds it. Unlike either count it is an amount of money: admission
tallies no unit of it, and its value comes from the accounting record the
invocation produced §FS-rhei-cost-accounting.

Travel and invocations are integer counts. Neither is a duration and neither is
an amount of money. Neither is `visits:`, `attempts:`, or a poll counter: those
bound a state entry, and they may refresh or increment without touching either
count ([§REQ-bounded-neural-work.1](../requirements/bounded-neural-work.spec.md#1-the-five-levels)). Spend is the amount, held in integer
micro-units of the account's currency so that the arithmetic below stays exact
and no rounding accumulates §FS-rhei-cost-accounting.5.

At every durable boundary, for each of the three:

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
unconfigured state in which any of the three dimensions is unbounded
([§REQ-bounded-neural-work.2](../requirements/bounded-neural-work.spec.md#2-bounded-by-default-refusing-nothing-that-runs-today)).

Requesting more than the machine allows is **never a validation refusal**. The
plan is valid, the effective bound is the machine's, and every surface that
reports the bound says so. Refusing it would make a template invalid on the
machine that did not write it; honoring it would let a template raise the cap of
the machine that pays for it.

The clamp is enumerated to exactly the two count dimensions and the spend
dimension. `agent_timeout`, `attempts:`, `visits:`, `poll.max_attempts`, and
every other setting keep their existing resolution and are not clamped by
anything here.

The first term of the chain above is not open to every dimension:
`spend_per_day` resolves on the **machine and project tiers only**
§FS-rhei-budgets.2.1.

### 2.1. The settings keys

Four keys in the `defaults` block of global or project settings
([§FS-rhei-agents.1.1.1](rhei-agents.spec.md#111-defaults)):

| Key | Type | Built-in | Bounds |
|---|---|---|---|
| `transition_limit` | positive integer | `80` | applied transitions per ticket identity |
| `invocations_per_day` | positive integer | `200` | admitted neural starts per project per UTC day, in the window contract |
| `invocation_lifetime_max` | positive integer | `6000` | the ceiling on an explicit lifetime allowance |
| `spend_per_day` | positive number | `400.00` | measured spend per project per UTC day |

The four built-in values are measured rather than chosen, and the measurement —
the definition of a healthy history, the observed maxima, the multiplier, and
what the evidence could not see — is recorded in [§REQ-bounded-neural-work.7](../requirements/bounded-neural-work.spec.md#7-the-defaults-are-measured-not-chosen) so
that a re-measurement can supersede them honestly. The spend number was
measured over a different corpus under a restated definition of healthy, which
§REQ-bounded-neural-work.7.5 states separately for exactly that reason.

`invocation_lifetime_max` is a ceiling, not an allowance: nothing consumes it
and no project receives it. It clamps `rhei budget init` and every `adjust`.

`spend_per_day` is a **bare number** carrying no symbol and no currency code:
`400`, `25.00` and `9.5` are all accepted, to at most six decimal places, which
is the micro-unit the amount is held in. The currency is the account's own and
is fixed by §FS-rhei-budgets.5.5, not by this key, because a machine setting
that named a currency could disagree with the records it is charged from and
nothing would say which of the two was wrong.

`spend_per_day` is the one key of the four a plan or a profile may **not**
declare; it resolves on the machine and project tiers only. A plan that could
raise a day's spend would be raising the cap of whichever machine paid for it,
which is the same reason `invocations_per_day` is not plan-declarable.

For the three counts, a value that is zero, negative, fractional, or the word
`unlimited` is a settings error naming the key. For `spend_per_day` a
fractional value is the point of the key, so the error is the narrower one:
zero, a negative, the word `unlimited`, or more than six decimal places.

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

### 3.4. The spend window

Spend is bounded per **UTC calendar day**, always. It is not a third invocation
contract and it does not follow the one the account holds: an account moved to
the lifetime contract by `rhei budget init` still has its spend bounded by the
day, because §FS-rhei-budgets.10 grants no spend allowance for a lifetime
contract to bound.

The day key is the one [§FS-rhei-budgets.3.3](rhei-budgets.spec.md#33-the-window-is-a-key-not-a-refill) already defines, read through the one
clock seam of [§FS-rhei-budgets.3.3.1](rhei-budgets.spec.md#331-reading-the-clock-from-the-environment), and it is a key rather than a refill for
the same reasons. A day's spend capacity is never written, granted, or reset;
every amount is stamped with the UTC day it was charged in, and the remainder
is derived by replay:

```text
remaining = spend_per_day - consumed(today) - outstanding(today)
```

The highest-day-key guard of [§FS-rhei-budgets.3.3](rhei-budgets.spec.md#33-the-window-is-a-key-not-a-refill) applies unchanged, so a clock
moved backwards keeps charging the day already recorded and mints no money.

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
`release`, `transition`, `adjust`, and `spend`. Replaying a receipt id with
identical content is idempotent; replaying it with different content is
corruption. An unknown kind is retained and makes this build read-only until it
understands it.

A `reserve` receipt for a neural start additionally carries the worst case
reserved against the day, `spend_reserve_micro`, and the account's
`spend_currency`. They ride the existing kind because one transaction decides
them at one instant, and a build that predates them ignores payload keys it
does not know, so an account that has only ever reserved still reads on an
older build.

A `spend` receipt is the settle, and it has to be a kind of its own because
every other kind is appended *before* a measured cost can exist. It names the
`reservation_id` it settles, the `amount_micro` charged, the `currency`, and a
`basis` of `measured`, `unpriced`, or `unmeasurable` §FS-rhei-budgets.6.2. At
most one `spend` receipt may name a reservation: a second naming the same
reservation with different content is corruption, and one naming a reservation
the chain does not hold is corruption. A build that does not know the kind
reads the account **read-only** rather than refusing it, which is what the
closed-vocabulary rule above already promises and what a downgrade across this
version is owed.

A ticket's identity is bound by **source path and content hash**, so several
live sources are tolerated only while their bytes agree. That is what makes a
*moved* ticket keep its travel, and only a moved one: agreeing bytes are what
distinguishes a relocated document from a conflicting one, and say nothing about
whether one identity has two live claimants, so a *copy* does not take the
travel of the ticket it was copied from §FS-rhei-budgets.5.2.1. Two documents
claiming one identity with different content are a conflict rather than a fork.

The binding is also what a ticket carrying no identity is resolved *from*. A
ticket whose document names no identity is not therefore a new ticket: the
ledger is asked for a binding on this source path and display id first, and one
is minted only where there is none. That closes the one door left open by the
fact that a receipt and the document naming it are two writes rather than one —
a document whose write was lost after its receipts were durable would otherwise
be handed a second identity, and with it a second travel bound. Deleting
`budgetTicketId` by hand is the same case and gets the same answer: the history
comes back. The key is rhei's own and registered as one
([§FS-rhei-transitions.2.5](rhei-transitions.spec.md#25-keys-rhei-writes)), which
is also what keeps it out of the author metadata a query publishes. The display id is part of the key, because one document holds every
ticket of a rhei and a binding matched on the path alone would hand one ticket's
travel to its sibling — and the key may only move to a display id the document
no longer claims the identity under §FS-rhei-budgets.5.2.1.

#### 5.2.1. One identity, one live ticket

A binding counts one uuid's travel against one display id, and that display id
may move: a renumbered task and a renamed plan file both carry their travel with
them. What it may not do is move while the display id it is counted against is
still claiming the uuid. A bound uuid is **refused** admission under a second
display id while more than one live ticket claims it.

One travel bound covers one ticket, so two tickets drawing on one binding is one
bound covering two — capacity created out of a copied task definition, for which
[§REQ-bounded-neural-work.4](../requirements/bounded-neural-work.spec.md#4-nothing-creates-capacity) allows no source: fresh travel comes from a
genuinely new ticket identity and from nothing else. The ticket that earned the
history is also the one that would lose it, because the display id the account
counts against would name the copy instead.

The edit itself cannot be read. One metadata file holds every ticket of a rhei,
so a definition copied onto a sibling and a definition renumbered in place are
the same bytes at the same path. What tells them apart is what the live sources
still claim, and that is the one fact this rule turns on — how many live tickets
name the uuid.

**More than one — refused.** The admission is refused under
§FS-rhei-budgets.8's contract rather than as a halt: no arm is started, no edge
is applied, no task result is written, and the ticket stays in the state it is
in. No bound is what stopped it, so it carries the plain `error:` shape rather
than the six-row report of an exhausted bound:

```text
error: ticket 'plan.2' claims a budget identity the account holds for 'plan.1'
       identity:    7f3c1a90-5e21-4d8b-9a6c-bc5de10f12e7
       claimed by:  plan.1 and plan.2, both live in plan.rhei.md
       why:         one travel bound covers one ticket, and these two would
                    draw on one
       to fix it:   give the copy a fresh `budgetTicketId` in that file; the
                    account holds this history for 'plan.1'
```

It names both display ids, the uuid, and the file they are both live in, because
those are what a person needs in order to find the key they duplicated. Its
**one** remedy — §FS-rhei-budgets.8 requires exactly one — is a fresh
`budgetTicketId` for the copy. Deleting the copy's key is not offered and is not
a way out: in an account whose binding has already moved, a keyless copy
resolves to the binding that moved and adopts it again.

**Exactly one, or none — permitted, and reported.** The display id the uuid was
counted against is gone from every live source, which is a move. The key follows
the ticket as it does today and the travel comes with it. What changes is that
the move is no longer silent: it is reported on the warning channel, beside the
edge that caused it.

```text
warning: travel for 7f3c1a90-…-bc5de10f12e7 now counts against 'plan.3'; it was
         counted against 'plan.1', which this plan no longer has
```

A move is the one case the refusal cannot reach — delete the original's key and
then write it onto a second ticket, and the document says exactly one ticket owns
the identity — so without that line, travel following a heading edit in silence
is what would be left standing.

A ticket carrying no identity never reaches this rule. It resolves through the
binding on its own display id and source path, so the display id it then binds
is the one already recorded and the binding does not move: the lost-write case
of §FS-rhei-budgets.5.2 keeps its answer.

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
recreate capacity **while the journal that is left verifies**. Deleting the
journal **and** the witness is the stated residual of this design. Where an
operator's state directory happens to sit under a project root, that residual
narrows to one key: both copies are in the one tree, so removing the tree
removes both.

The condition is the whole of what `rhei budget forget` is
([§FS-rhei-budgets.10](rhei-budgets.spec.md#10-rhei-budget)). It puts an audited
door where that residual already is rather than opening a second one: it
**refuses** every account whose journal is still there, verifying or damaged, so
no working balance is ever reset by it and no journal that is this project's own
is given up instead of restored; it keeps the receipts rather than unlinking
them; it records who
retired the root, when, why and with what argv; and it retracts the root's
entry from the witness index, which the bare `rm -rf` leaves behind. Measured
against the residual it replaces, the bound is better guarded after the command
exists than before.

Retiring a root is not repairing an account and does not recreate the old one's
capacity: the receipts stay readable under a retired name, the path stops
resolving to any account, and the next admission establishes a new one at zero
consumed under the lawful **absent** path
([§FS-rhei-budgets.5.4](rhei-budgets.spec.md#54-absent-damaged-adopted)). Nothing
carries over, in either direction.

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
both paths. A damaged account is never repaired in place; what a refusal offers
depends on which of three sub-cases it is, because they are not the same
accident:

| sub-case | what is true | what the refusal offers |
|---|---|---|
| `journal_absent` | the journal file is not there at all | both readings, and one runnable command for each |
| `journal_truncated` | a journal exists but holds fewer receipts than the witness | restore it by copying the witness back |
| `chain_broken` | a journal exists and its identity, sequence or hash chain does not verify | restore it by copying the witness back |

The names are the vocabulary a machine reader gets
([§FS-rhei-budgets.10](rhei-budgets.spec.md#10-rhei-budget)) and the reason the
split exists. Where a journal is present, it **is** this project's journal and
its tail is what was lost, so copying the witness over it restores this
project's own history and is the remedy.

Where the journal is **wholly absent**, that inference does not hold, because
two different things produce byte-identical state: this project's journal was
lost, or the path was previously held by a different project and this one is new
at it. Rhei cannot tell them apart — the distinguishing fact is the operator's
intent, and nothing on disk carries it — so it **does not choose**. The refusal
states both readings and names the command that answers each: copying the
witness back where the journal was this project's, and `rhei budget forget`
where the path was reused. It may **not** instruct the copy as the remedy: on a
reused path that instruction succeeds and silently charges a brand-new project
for the whole spend of the one that used the path before it, with no signal that
anything happened.

The three states stay three. Path reuse is not a fourth, because it is not
distinguishable from a rolled-back journal — which is the case this check exists
to catch — and a state the tool cannot detect is not a state it can answer.

**Adopted** — the journal is present and internally valid but this machine has
no witness: a project cloned from git, or the same project on a new machine. The
witness is written from the journal's own bytes and the run proceeds. This mints
nothing, because the consumed counts travelled with the journal. It is a
deliberate loosening: refusing every fresh clone would refuse work that runs
today, which [§REQ-bounded-neural-work.2](../requirements/bounded-neural-work.spec.md#2-bounded-by-default-refusing-nothing-that-runs-today) forbids. **The witness catches an
accidentally lost tail, not a hand-edited ledger**, and that is the whole of
what it claims.

### 5.5. One currency per account

A project account is denominated in exactly **one** currency, fixed by the
first receipt it holds that carries an amount. That is a `reserve`, so the
currency comes from the composed price book, which is validated before any
agent starts. An account that has never carried an amount has no currency yet
and takes the next one it is shown.

A run whose composed price book names a currency other than the one the account
already holds is **refused before any agent starts**, naming both currencies
and the account. Nothing is converted: an exchange rate is a fact about a day
that this ledger does not hold, and adding two currencies would make every
number above this line wrong without saying so.

This is the same answer §FS-rhei-cost-accounting.5.1 already gives for a second
currency arriving in one accounting root, taken at a different boundary and for
a different reason — that check protects a rollup, this one protects a bound —
which is why the rule is stated here, where the account that holds the currency
is specified, rather than in the accounting specification.

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
3. check one travel unit for the ticket, one invocation unit per arm, and the
   worst case each arm may spend (§FS-rhei-budgets.6.2) against the effective
   bounds and against every ancestor envelope — in that order, so that a spawn
   standing at two bounds at once is refused on the count and reports the text
   it already reported (§FS-rhei-budgets.8);
4. append and durably sync one reservation carrying every unit and every
   reserved amount, or refuse and append nothing;
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

**For the two counts, the settle is monotone.** A reservation becomes
**consumed** once a start is recorded, confirmed or ambiguous, and is never
settled downward. `record_start` is appended immediately before the spawn call
and refined after it returns.

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

**For spend, the settle revises.** Money is not a count
([§REQ-bounded-neural-work.4](../requirements/bounded-neural-work.spec.md#4-nothing-creates-capacity)), and the monotone rule above is scoped to the two
counts in writing because a spend amount is *expected* to change exactly once.
What is reserved before a request is a worst case; what the day carries
afterwards is what the request cost. A settle that lowers it is an estimate
being corrected, not capacity being created, and it is the only write that may
lower an amount.

The worst case reserved for every neural start is a flat **20.00** in the
account's currency. It is not derived per model, because no agent profile
declares a token or a context limit to derive one from; it is the largest
single invocation this workspace has recorded, rounded up, and it carries no
margin of its own §REQ-bounded-neural-work.7.5. Its size bounds the *in-flight*
exposure rather than the day's total, because a settled reservation gives it
back.

A settle has four outcomes, and only the first revises:

| what the invocation produced | what the day carries | mark |
|---|---|---|
| an accounting record priced with an amount | the measured amount | — |
| a record whose pricing is `unpriced` or `not-applicable` | the reserve | `unpriced` |
| no accounting record at all | the reserve | `unmeasurable` |
| a settle that could not be written | the reserve, left outstanding | `unsettled` |

The third row decides where the fallback has to be charged. Usage extraction is
not supported for every agent §FS-rhei-cost-accounting.3.2, so for that class
no record is ever written and a settle never runs at all. The worst case is
therefore charged at the **reserve**, which always runs because admission
always runs. **No invocation is ever charged nothing.** A dimension that read
zero for every transport it cannot price would be silently absent for exactly
the transports no other bound watches.

The fourth row is best-effort preserved and never fails the run. An unsettled
amount is not stored as a mark but *derived*: a reservation carrying a spend
reserve, with no `spend` receipt, whose owning run is gone — the same test the
release step above already performs, on the same account under the same lock.
It stays outstanding, so a wrong day reads high rather than silently low, and
every surface of §FS-rhei-budgets.9 says how many there are.

A `spend` receipt settles money and nothing else. It releases no travel unit
and no invocation unit and changes neither count in either direction, so every
paragraph above this one is as true after a settle as before it.

### 6.3. What never debits

`rhei validate` and `rhei run --dry-run` ([§FS-rhei-run.4](rhei-run.spec.md#4-dry-run)) resolve and report
every bound and preview what a pass would cost. They take no lock that mutates,
append no receipt, and debit nothing.

## 7. Nested runs

A nested `rhei run` **of the same project** inherits that project and its one
account through an authenticated ancestry path, and **opens no balance of its
own**. Every descendant admission charges the shared account and must
additionally fit its ancestor's reservation envelope and deadline.

An absent, finished, envelope-less, or exhausted ancestor is a typed refusal
before spawn, naming which of the four it is. Each nested neural start is
charged exactly once. A window renewal makes capacity available to new
admissions only: it never enlarges, revives, or extends the deadline of a
reservation an ancestor already holds.

Ancestry is the one door through which anything a program does is counted
([§REQ-bounded-neural-work.6](../requirements/bounded-neural-work.spec.md#6-the-residual-gap)).

An envelope bounds **the account that minted it**, and that is what decides the
one nested run which is no descendant at all. A `rhei run` whose own account is
not the one holding the ancestor was never inside that envelope, so opening a
balance in its own ledger is the *absence* of an envelope to escape rather than
an escape from one. Such a run is still counted — on its own project's bounds,
§FS-rhei-budgets.7.2 — and what §REQ-bounded-neural-work.6 forbids is work
counted **nowhere**. Reading that fallback as an escape hatch from the
requirement has it backwards: the hatch would be to charge an account this run
has no claim on.

### 7.1. How the descriptor reaches a nested run

Two variables are set **together** in the environment of every agent Rhei
starts, and they are one descriptor rather than two facts:
`RHEI_BUDGET_PARENT_RESERVATION` names the reservation that invocation was
admitted on, and `RHEI_BUDGET_PARENT_ACCOUNT` names the account that minted it.
A `rhei run` started inside that agent reads both, and asks whose the descriptor
is before asking what the ledger holds — identity first, then the journal
([§AR-neural-admission.6](../architecture/neural-admission.spec.md#6-ancestry)).
The three answers differ in kind:

- **The accounts match.** The reservation is an ancestor of this run, and the
  run places its own admissions under it. The descriptor carries no credential,
  because there is nothing to credential: a descendant charges the same account
  through the same locks, and the ledger is what decides whether the name it was
  given buys anything. A value naming a reservation this project's journal does
  not hold, or one that is not outstanding, is `ancestor_unavailable` before any
  spawn — which is why forging it buys nothing rather than buying a fresh
  balance.
- **The accounts differ.** The descriptor was minted for another project and
  names no ancestor of this run. This is not `ancestor_unavailable`, because
  nothing was unavailable: there was no ancestry to authenticate. The run is
  admitted unparented against its own account and says so once
  (§FS-rhei-budgets.7.2).
- **`RHEI_BUDGET_PARENT_ACCOUNT` is absent.** The reservation is taken as an
  ancestor exactly as it is when the accounts match, refusal included. A parent
  too old to name its account is the only thing that produces this case, and it
  must lose nothing by it: the reservation variable's meaning and format are
  unchanged, so anything that worked before the account variable existed works
  after it.

A refusal on this path names **where the value came from** as well as which
value it was. `no such reservation` alone reads as damaged budget state and
sends a reader to the account directory rather than to the environment, which is
the wrong half of the machine.

### 7.2. A descriptor minted for another project

A run whose account is not the one holding the ancestor is admitted
**unparented**: it draws nothing against the exporter's envelope, it is not held
to the exporter's deadline, and the receipt it writes names no parent. It is not
thereby unbounded. It is held to its own project's three bounds of
§FS-rhei-budgets.1, resolved from that project's settings and clamped by the
machine ceiling ([§FS-rhei-budgets.2](rhei-budgets.spec.md#2-where-a-bound-comes-from)), exactly as an unparented run always has
been.

The run says so **once**, as a note rather than a warning: nothing is wrong, and
a warning invites someone to fix what is working. Once per `rhei run` — not once
per admission and not once per pair of accounts — because the fact is a property
of the run's environment and does not change while the run lasts, so a plan of
thirty tickets repeats it no more than a plan of one. The note names the
reservation, the project the descriptor was minted for, and the account this run
charges instead, each by the directory a reader would recognize where that is
known and by uuid where it is not ([§FS-rhei-budgets.8](rhei-budgets.spec.md#8-exhaustion)).

This is the case that lets Rhei's own suite, and any `rhei run` of a plan
outside the ancestor's project, run from inside an agent at all: each resolves a
state root of its own, so each is a child of an account the descriptor was never
minted for.

## 8. Exhaustion

A refused admission starts no arm, applies no edge, writes no task result, and
leaves the ticket in the state it is in with its artifacts intact. This is the
existing stall contract of [§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop) and not a new one: `rhei run`
continues with the other claimable tickets, and the run exits non-zero once a
pass makes no progress, naming every halted ticket.

The halt names, in one place:

- the **dimension** — ticket travel, project invocations, or project spend;
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

A spend halt has the same six rows and the same labels. Its remedy is the
renewal instant, because spend is bounded by a window §FS-rhei-budgets.3.4 and
the rule above does not change for being about money — which costs the halt the
chance to mention `defaults.spend_per_day`, and is the accepted price of one
halt reading the same on every surface:

```text
error: project 'panta' has spent today's measured budget
       dimension:   project spend
       bound:       $25.00 (machine)
       consumed:    $6.22  outstanding: $20.00  remaining: $0.00
       mode:        window (2026-09-26Z)
       renews at:   2026-09-27T00:00:00Z
```

Two things in that shape are particular to spend. `outstanding:` carries the
worst case of the request that was just refused, added to whatever the day
already held outstanding, because consumed alone never reaches a spend
ceiling — what reaches it is consumed plus the next reserve, and a halt whose
arithmetic the reader cannot do has not said what stopped the work. And every
amount is written in the account's currency §FS-rhei-budgets.5.5: `$` before
the number for USD, the code after it otherwise, as in `25.00 EUR`.

A spend halt carries **one optional row**, present only where some part of the
day was charged at the fallback rather than measured, so a fully measured halt
has exactly the six rows above:

```text
       estimated:   2 unpriced, 1 unmeasurable, 1 unsettled (charged at $20.00 each)
```

It sits between `consumed:` and `mode:`, because it qualifies the numbers above
it. A mark with no members is left out of the list rather than printed as a
zero.

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
- **the run report** records the starting and ending snapshot of all three
  dimensions, every reservation the run made, and the exact halt reason
  ([§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout)).

Every surface that carries the spend dimension carries the account's currency
with it, and says how much of the day was charged at the fallback rather than
measured, by the three marks of §FS-rhei-budgets.6.2.

Where cost accounting also exists for a run, the two are still reported side by
side and neither is described as the other. Accounting only measures, prices
and rolls up: it resolves no ceiling and refuses no spawn, and **where its
extraction fails the counts are unaffected**. That sentence is the whole of the
distinction and it is unchanged. What is new is that one dimension of the
budget is now *charged* from those records rather than only displayed beside
them, and it is the dimension whose unit is money rather than a count. Where
extraction fails, that dimension is charged the worst case rather than nothing
§FS-rhei-budgets.6.2 — which is a fact about the bound, not a change to what
accounting measured.

## 10. `rhei budget`

```text
rhei budget init <TARGET> --invocations <N> --reason <TEXT>
rhei budget show <TARGET> [--rhei <ID>] [--format text|json]
rhei budget adjust <TARGET> --invocations <N> --reason <TEXT>
rhei budget forget <TARGET> --reason <TEXT>
```

All four resolve the whole project and its single account even when the target
is one member rhei, and `--rhei` narrows display only.

`init` moves the project from the window contract to the lifetime contract with
the given allowance, clamped by `defaults.invocation_lifetime_max`. It is
optional: a project that never runs it is bounded by the window contract and is
never refused for not having run it.

`show` is read-only and reports, for each of the three dimensions, the
effective bound, its value source and limiting source, consumed, outstanding,
remaining, the contract in force, the window day key where one applies, the
account's health, and the project identity. For spend it reports the amounts in
the account's currency and additionally how much of the day was charged at the
fallback rather than measured, by the three marks of §FS-rhei-budgets.6.2.

The marks go on their own line beneath the spend line, in the words the halt
uses, and the line is absent where the whole day was measured:

```text
project spend: $20.00 consumed + $0.00 outstanding / $400.00; $380.00 remaining (window (2026-09-26Z))
  estimated:   1 unpriced (charged at $20.00 each)
```

`init` and `adjust` take no spend argument and gain none. There is no spend
allowance to grant: spend is bounded by a window always §FS-rhei-budgets.3.4,
and a window is renewed by the passage of time rather than by an audited grant.

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
performs no mutation. `show` is the command that has to satisfy that in every
sub-case, because it is the one a refusal sends an operator to: it may not fail
to open an account it was asked to describe. On an account it cannot verify it
**reports** — naming the journal path, the witness path, what the recorded
history holds, and one runnable command per available remedy — writes nothing,
creates no account, directory, journal or receipt, and exits non-zero. An account whose journal is wholly absent
is the case where there is nothing to open at all, and it is reported like any
other rather than refused for the missing file. **Runnable** is literal: in that
sub-case the directory the journal belongs in is missing too, so the restore is
offered as the one command that creates it and copies — a remedy that fails with
`No such file or directory` is the ticket's own complaint in miniature.

Literal on the operator's own platform, too, because a command line is for the
shell they are holding. The restore is spelled in that shell — `mkdir -p … && cp
…` under a POSIX shell, `mkdir … && copy …` under `cmd`, where neither `cp` nor
`mkdir -p` exists — and every path any printed command interpolates is quoted in
that shell's own form. The quoting is not cosmetic: unquoted, a project path
holding a space makes `mkdir -p` read two words and create a directory tree
relative to wherever the operator happened to be standing, so the offered remedy
fails *and* writes outside the path it named.

The account's `health` is a closed vocabulary of two values, `verified` and
`damaged`. Where it is `damaged` the report additionally carries the `damage`
sub-case — one of `journal_absent`, `journal_truncated`, `chain_broken`, the
three of §FS-rhei-budgets.5.4 — and where it is `verified` there is no `damage`
to carry. A reader may depend on both: neither gains a value without this
section gaining it too.

Under `--format json` a `show` that cannot verify the account emits the
machine-readable error object of
[§FS-rhei-errors.5](rhei-errors.spec.md#5-machine-readable-errors) on stderr and
nothing on stdout, carrying the same facts the text report states as named
members beside `message` and `help`: `health`, `damage`, `project_id`,
`project_root`, `account`, `journal`, `witness`, `receipts`, `invocations`,
`restore`, and `forget` where the sub-case admits one. A harness reading a
damaged account gets one shape whichever state it finds, rather than prose it
cannot parse — and `project_id` and `account` mean there exactly what they mean
in a verified report, the identity and the account **directory**, whether or not
that directory is there.

So does `project_root`, and its spelling is fixed for both health states: it is
the target as the operator spelled it, which is what the text report's
`Project:` line prints beside the identity. Everything that names where bytes are
or which identity owns them is spelled **resolved** instead — `account`,
`journal`, `witness`, and every command the report offers — because the witness
index is keyed by the resolved root, and it is that key a retirement retracts.

`forget` retires a **root** rather than repairing an account: it is how an
operator says that the path was reused and the recorded account was not this
project's. Four cases, and only the last acts:

| the account at this path | `forget` |
|---|---|
| journal present and verifying | **refuses**, exits non-zero, writes nothing, and names `adjust` as what changes an allowance |
| journal present and damaged (`journal_truncated`, `chain_broken`) | **refuses**, exits non-zero, writes nothing, and names the restore of §FS-rhei-budgets.5.4 |
| no witness claims this root | **refuses**, exits non-zero, naming the path — the lawful **absent** state has nothing to retire, and a path with no record is far more often a typo |
| damaged with the journal wholly absent (`journal_absent`) | retires it |

The refusal on a sound account is the guarantee of
§FS-rhei-budgets.5.3 and not a convenience: without it this command would be a
way to reset a working balance, and the bound would hold only until someone ran
it.

The refusal where a journal is **present** follows from
§FS-rhei-budgets.5.4 rather than adding to it: a journal that is there **is**
this project's, so the remedy is to restore its tail, and retiring the root
would discard a real account while leaving that journal exactly as unverifiable
as it was — an operator left worse off than before they ran the command. Nothing
is lost by narrowing, because a witness and a journal both present always have
restore as their answer. A damaged report never offers `forget` in those
sub-cases either; this says what happens when it is asked for anyway.

Retiring is one audited move, and nothing is destroyed by it. The witness
directory is moved under a `retired/` name beside the live ones, keeping
`history.jsonl` byte for byte; a receipt is written beside it recording the
actor, the UTC instant, the reason and the exact argv — the audit shape
`adjust` already carries — together with the root, the uuid and the counts the
history held; and the root's entry is retracted from the witness index, so the
path resolves to no account. That last part is not optional: an entry left
behind would hand the next project at the path the retired project's identity,
and a retirement that leaves the identity in place has not retired the root.

Because the uuid is gone, anything keyed to it starts fresh — a ticket's budget
identity is `ticket:<project-uuid>:<…>` (§FS-rhei-budgets.5.2), so travel
recorded against the retired account is not in the new one. On a reused path
there is no such key to lose, the plan files being a fresh copy. Anywhere else
that is a reason to restore rather than to forget, which is why `forget` refuses
a sound account and why a damaged report puts restore first.

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
| cost accounting | unchanged in how it measures, prices and rolls up. What is new is that one record it already wrote is now read by the spend dimension ([§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)) |
| a plan with no spend field | there is no spend field to omit: `spend_per_day` is not plan-declarable, and every plan runs under the machine's value or the built-in (§FS-rhei-budgets.2.1) |
| `rhei budget init` / `adjust` | neither gains a spend argument, because there is no spend allowance to grant (§FS-rhei-budgets.10) |
| git | the account is machine state and is gitignored; a project that commits it anyway is *adopted* on the next machine ([§FS-rhei-budgets.5.4](rhei-budgets.spec.md#54-absent-damaged-adopted)) |

Six things do change for someone, and they are the point rather than a side
effect:

1. A project that starts more than 200 agent processes in a UTC day now stops.
   In this workspace's whole history the busiest day was 58.
2. A ticket that makes more than 80 moves now stops. The largest ever seen is 20.
3. `rhei transition` by hand now spends one travel unit. Silence becomes a count.
4. A **damaged** account is an error where there was previously no account at
   all. A **missing** one is not: absence is lawful and silent.
5. A project charged more than $400.00 of measured spend in a UTC day now
   stops, and an invocation Rhei cannot price or cannot measure is charged
   20.00 rather than nothing. The busiest project-day in this workspace's whole
   archive was $96.37, so the ceiling is not what a working project meets — but
   more than twenty unmeasurable invocations in one day is, and that is the
   sharpest edge in the bound rather than an oversight §FS-rhei-budgets.6.2.
6. A project whose price book names a currency the account does not hold is
   refused before any agent starts, where it ran before §FS-rhei-budgets.5.5.

And one thing stops being true: `rhei reset` no longer returns a ticket to a
state from which it can consume without limit.

## Related Specifications

- [Requirements — Bounded Neural Work](../requirements/bounded-neural-work.spec.md) — why the bounds exist and where the defaults came from
- [Neural Admission Architecture](../architecture/neural-admission.spec.md) — the boundary, the lock order, and the receipt chain
- [Run Specification](rhei-run.spec.md) — the execution loop the admission checkpoint sits in
- [Agents Specification](rhei-agents.spec.md) — the inner timeout and attempt budgets
