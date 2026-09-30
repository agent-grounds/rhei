# REQ-bounded-neural-work: Every unit of neural work is bounded before it starts

Autonomous work is only predictable if starting it cannot start an unbounded
amount of more of it. Every unit of neural work Rhei starts therefore has a
finite, engine-checked bound in force before the first subprocess is spawned —
and has one **without anyone declaring anything**, because a bound that has to
be authored is absent from exactly the plans that were written before anyone
thought about bounds. [§GOAL-rhei-outcomes](../functional-spec/goals.md#goal-rhei-outcomes-goals)

Rhei reached `agent-grounds/rhei#232` at $75.58, of which $66 was spent in
review and fix rounds past a configured ceiling of two, because nothing bounded
the ticket's total moves and every fresh `rhei run` started the count again.
That is the failure this requirement removes.

The bounds hold identically on every supported platform. [§REQ-cross-platform](cross-platform.md#req-cross-platform-one-tool-on-linux-macos-and-windows)

## 1. The five levels

Bounding one unit of neural work is five separate questions, and no level
substitutes for another:

| Level | Bounds | Realized by |
|---|---|---|
| 1. One round | how long one orchestrated agent turn may run | `agent_timeout` [§FS-rhei-agents.7.1](../functional-spec/rhei-agents.spec.md#71-configuration) |
| 2. One visit | how many times a single state entry may be spawned | `attempts:` [§FS-rhei-agents.3.2.3](../functional-spec/rhei-agents.spec.md#323-attempt-budget) |
| 3. One ticket | how many moves a ticket may make in its whole life | `transition_limit` [§FS-rhei-budgets.2](../functional-spec/rhei-budgets.spec.md#2-where-a-bound-comes-from) |
| 4. One project | how many neural starts a project may be admitted | the invocation contract [§FS-rhei-budgets.3](../functional-spec/rhei-budgets.spec.md#3-the-invocation-contracts) |
| 5. One project-day | how much measured spend a project may be charged | `spend_per_day` §FS-rhei-budgets.3.4 |

Levels 1 and 2 are the ground as it stood before this requirement. Levels 3, 4
and 5 are what it adds. Levels 3 and 4 are **counts** — not time, and not
money. Level 5 is money, and it is the one level whose unit is not a count: it
is charged from what an accounting record measured rather than from anything
admission tallies for itself.

Two adjacent state-entry bounds are neither of these five and are not changed
by this requirement: `visits:` bounds how many times a ticket may *enter* one
state ([§FS-rhei-transitions.4.3](../functional-spec/rhei-transitions.spec.md#43-counted-loops)), and the run loop's pass bound ends a run that
makes no progress ([§FS-rhei-run.3](../functional-spec/rhei-run.spec.md#3-execution-loop)). A level-3 or level-4 count is independent
of both: `visits:` and `attempts:` may refresh or increment without touching
either count, and neither count is refunded when they do.

The levels **compose downward only**. An inner timeout, attempt, visit or poll
counter never extends an outer count, and releasing an inner counter never
credits an outer one. The four local non-spend exceptions of
[§FS-rhei-agents.3.2.3](../functional-spec/rhei-agents.spec.md#323-attempt-budget) stay local: an attempt given back is an attempt, never an
invocation.

## 2. Bounded by default, refusing nothing that runs today

Every plan, machine and workspace that runs today runs after this requirement,
with **zero new declarations** and no new refusal for a missing field, a
missing settings key, or a missing account. Explicit initialization is optional
and is never a migration prerequisite.

That is possible because the built-in default is the bound. Where machine-global
settings configure no value for a count dimension, the built-in default *is* the
machine's value and therefore the machine's ceiling: there is no configured
state, and no unconfigured state, in which a count dimension has no bound.

The machine's value is a **ceiling**, not merely a default. A project setting, a
plan, or a profile may declare a lower value and is honored; one that declares a
higher value is accepted, clamped to the machine's value, and reported as
clamped. Asking for more than the machine allows is never a validation refusal —
otherwise a template written on one machine would either be invalid on another
or would raise the cap of the machine that pays for it. [§FS-rhei-budgets.2](../functional-spec/rhei-budgets.spec.md#2-where-a-bound-comes-from)

**Level 1 is the exception, and it is recorded here rather than resolved away.**
Everything above is scoped to the count dimensions — levels 3 to 5 — which is
what lets it promise no new refusal: each of them has a built-in value, so none
of them can be unbound. Level 1, how long one orchestrated agent round may run,
has no built-in default. It is the one bound that must still be authored, and a
state that resolves none is refused rather than defaulted — by `rhei validate`,
by `rhei run --dry-run` and by `rhei run` alike (§FS-rhei-agents.3.2.2).

That is a real strain against the lead of this requirement, which wants a bound
*without anyone declaring anything*, precisely because an authored bound is
absent from the plans written before anyone thought about bounds. Two things
make it the right asymmetry rather than an oversight. A timeout is not a ceiling
that can be clamped like a count: exceeding it kills an agent mid-round, so a
built-in number would silently bound work at a value nobody chose for their
workload. And level 1 is the one bound whose absence breaks deterministic
completion rather than merely leaving a count unbounded, which is why it is a
validation error where the count dimensions are not. Shipping a built-in
`defaults.agent_timeout` would close the asymmetry; that is a different decision
from this one and would have to be argued as such.

## 3. Two counts and an amount

**Ticket travel** is the number of applied transitions a ticket may make over
the lifetime of its identity. Exactly one unit is consumed per applied edge,
whoever selected it — an agent outcome, a program, a callback, an ordinary
self-loop, or a person typing `rhei transition`. Initial placement, poll waits,
no-edge completions, stalls, and refused admissions consume none. A fanout
consumes the one travel unit of the ticket's one applied edge.

**Project invocations** are the neural starts a project may be admitted. One
unit per admitted start, including every retry, every neural poll attempt, and
each arm of a fanout. There is exactly **one durable account per project**,
shared by every rhei of the project, every descendant added later, every
concurrent process, and every nested runtime; every admission from any of them
serializes against it.

That account holds one of two named contracts at a time, bounding different
quantities, and the specification never describes one as the other:

- the **window contract** bounds invocations admitted during the current
  window, is what a project has until it is explicitly initialized, and is
  renewed solely by the passage of the window. It makes no claim about a
  project's lifetime total.
- the **lifetime contract** bounds invocations ever admitted, is what an
  explicit `rhei budget init` establishes, and is never renewed.

A finite lifetime total as the *default* would eventually stop every healthy
long-lived project and ask an operator to top it up — a block by another route,
which [§REQ-bounded-neural-work.2](bounded-neural-work.spec.md#2-bounded-by-default-refusing-nothing-that-runs-today) forbids. A window is a rate: it stops a project that is spinning and
never meets a project that is working.

**Project spend** is the measured cost a project may be charged in one UTC day.
It is not a third count. No unit of it is tallied at admission; its amount
comes from the cost accounting record a completed invocation already produces
§FS-rhei-cost-accounting, and the project-day is its whole unit. It holds no
lifetime contract, for the reason the paragraph above gives: a lifetime total
of money stops every healthy long-lived project in the end, and money renews on
the same window an invocation count does.

Before each admitted start a worst case for the next request is reserved
against the day, and once the invocation's measured cost is known the reserve
is replaced by that amount. An invocation whose cost cannot be measured is
charged the worst case and never nothing, because a dimension that reads zero
for every transport it cannot price is a bound that is silently absent exactly
where it is least watched. §FS-rhei-budgets.6.2

## 4. Nothing creates capacity

Neither count is ever created by an operation that is not one of its two lawful
sources. `rhei reset`, a fresh `rhei run`, `--rhei` selection, snapshots,
appended work, live member admission, a restart, a copy or a move of a plan
file, and a nested runtime create no travel and no invocation capacity, and in
window mode none of them advances the window.

Fresh invocation capacity has exactly two lawful sources: the passage of the
window, and an audited `init` or `adjust` under the machine's ceiling.

Fresh travel has exactly one lawful source: a genuinely new ticket identity,
which is a ticket whose id this project's account has never bound. A ticket
that is copied, moved, or re-instantiated under an id the account has already
bound keeps its history, so reset-and-rerun converges on the travel bound
rather than escaping it. No operation may assert a ticket identity the account
did not settle.

Everything above is about the **counts**, and the scope is deliberate rather
than incidental. A travel or invocation unit, once consumed, is consumed, and
nothing downstream of the start it paid for gives it back
[§FS-rhei-budgets.6.2](../functional-spec/rhei-budgets.spec.md#62-reserve-then-settle). **Money is not a count.** A day's spend is an amount
measured after the fact, so the number the ledger holds for one invocation is
expected to change exactly once — from the worst case reserved before the
request to what the request actually cost — and that revision may be downward.
Reading it as minting would be reading an estimate being corrected as capacity
being created.

Fresh spend capacity therefore has exactly one lawful source, the passage of
the window, and a spend reservation has exactly two lawful writes: the worst
case appended at admission, and the one settle that replaces it with the
measured amount or leaves it standing where nothing could be measured. A settle
touches no travel unit and no invocation unit in either direction, which is
what keeps the paragraphs above it true after one has been written.

## 5. Exhaustion is a halt where the ticket stands

The check is at admission, before the work it would bound. A refused admission
fires no transition, writes no task result, invents no terminal outcome, and
rewrites nothing the ticket already earned; the ticket stays exactly where it
is with its artifacts intact. The run continues with other admissible work and
exits non-zero once a pass makes no progress, naming every halted ticket.

A halt is only useful if it says what to do, so it names the dimension, the
effective bound, the consumed, outstanding and remaining amounts, the accounting
mode, the source that set the value, the machine as the limiting source when the
value was clamped, and **exactly the one remedy that raises the active limiter** —
the machine-global settings key when the ceiling limits, the audited `adjust`
when an explicit allowance limits, and the renewal instant when the window
limits. It never names an inner value the ceiling would clamp.

Every bound in force is visible with its value and its source *before* capacity
is spent, not only after it is exhausted. [§FS-rhei-budgets.9](../functional-spec/rhei-budgets.spec.md#9-visibility)

## 6. The residual gap

A program or a callback that calls a provider directly is outside the
invocation count. Rhei did not start that request and cannot see it; only the
confinement boundary of `agent-grounds/rhei#107` closes that gap, and this
requirement does not claim otherwise.

Ancestry is the one door through which anything a program does *is* counted: a
nested `rhei run` inherits the project's one account through an authenticated
ancestry path, charges it, and must additionally fit the ancestor's reservation
envelope and deadline. It opens no balance of its own, and it can neither
outlive nor outspend its ancestor. [§FS-rhei-budgets.7](../functional-spec/rhei-budgets.spec.md#7-nested-runs)

Provider-billed spend is the fifth bound and it **is** here, in the one grade
the evidence this engine already holds can carry: **measured** spend, charged
from the cost accounting record §FS-rhei-cost-accounting writes for a completed
invocation. That is the owner's ruling of 2026-09-25 on
`agent-grounds/rhei#107`. What stays on that issue is the stricter grade —
spend as the provider bills it, which needs a request broker, credential,
egress and process-tree confinement, and the qualification of a real transport,
none of which a count needs. The stricter grade is a later obligation and not a
precondition for this one.

The measured grade carries a residual of its own, stated here rather than
discovered later. A reserve is a worst case, not a promise: an invocation whose
actual cost exceeds what was reserved for it overshoots the ceiling by the
difference, and the day then carries the actual. At most one invocation can be
in that position at a time, because the next admission reads the settled
amount, and how far it can reach is bounded by `agent_timeout` and by the two
counts rather than by anything on the money side.

## 7. The defaults are measured, not chosen

A default nobody can source is a default nobody can supersede honestly. The
numbers below were measured over this workspace's real run records — 30
distinct `runtime/state-transitions.log` files and 436 `runtime/spawns/*.json`
records with `kind: "agent"` under `~/ag/*/panta/`. The synthetic
`tests/e2e/fixtures/accounting-archive/*.json` were deliberately **not** used:
a fixture proves arithmetic, never a maximum.

### 7.1. Healthy, defined first

A ticket history is **healthy** when its last recorded move landed in a terminal
state — `completed`, `cancelled`, or `resolved`. That is the only available
evidence that the history ended for a reason of its own rather than because a
run stopped: no budget-halted run existed when the measurement was taken, and a
ticket left mid-flight is an unfinished sample, not a maximum.

272 of 305 ticket histories qualified. 30 were still in flight and 3 were
waiting at a human gate; all 33 were excluded from the maxima below.

### 7.2. What the healthy histories reached

| dimension | n | median | p95 | max (healthy) | max (all) |
|---|---|---|---|---|---|
| lifetime travel per ticket | 272 | 1 | 9 | 17 | 20 |
| agent starts per project per UTC day | 21 | 9 | 42 | 51 | 58 |
| agent starts per project, lifetime | 16 | 8 | — | — | 138 |

The travel maximum of 17 is `agent-grounds/ephor#43`'s supervising ticket node,
completed. The day maximum of 51 is `~/ag/grund/panta` on 2026-09-23.

### 7.3. The multiplier, and the three numbers

| setting | value | derivation |
|---|---|---|
| `defaults.transition_limit` | **80** | 4 × 20, rounded to a legible number |
| `defaults.invocations_per_day` | **200** | 4 × 51, rounded to a legible number |
| `defaults.invocation_lifetime_max` | **6000** | 30 × 200: a month at the built-in rate |

The multiplier is ×4 in both count dimensions, applied to the all-sample maxima.
Four, because the corpus is one workspace over about four weeks and its longest
template — the `grounded-ticket` lifecycle on its foundational path — is the
longest workflow anyone here has written: a template with twice the review
rounds and a supervisor that re-enters twice as often still fits under 80. A ×2
margin would be met by a plan with four review rounds instead of two, which is
not a runaway.

`invocation_lifetime_max` is a ceiling and not an allowance: nothing consumes it
and no project receives it. It clamps `rhei budget init` and every `adjust`.

The busiest real day in this workspace's history used 29% of the daily default,
and the busiest ticket 21% of the travel default. A healthy continuing project —
the tool-report triage loop, which runs for as long as the workspace exists —
never meets either, and never needs an operator, because a window is a rate
rather than a total. A project that spins reaches 200 in a day and stops.

### 7.4. What the measurement could not see

Four gaps, stated rather than papered over, because they bound how much the
numbers above are worth:

1. **Retries are undercounted.** Two retries of the same visit overwrite one
   `runtime/spawns/` record, so the invocation counts are a **lower bound**, not
   an exact count. The ×4 margin absorbs it.
2. **Accounting is sparser than spawning.** The accounting roots hold 163
   invocation records against 436 agent spawns, because usage extraction is not
   supported for every transport. That is itself the argument for admission
   reading a count ledger of its own rather than accounting records
   ([§FS-rhei-cost-accounting](../functional-spec/rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)). The spend bound of level 5 neither softens that argument nor
   is an exception to it: the counts are still derived by counting
   reservations, and no accounting record moves one. Re-measured when the spend
   default was taken, the sparsity had grown worse — 165 records, only 80 of
   them carrying an amount. That is precisely why the spend dimension charges an
   unpriced or unmeasurable invocation its reserve rather than nothing. A money
   bound that read zero wherever extraction is unsupported would be absent for
   exactly the transports no other bound watches.
3. **There is no explicit-mode history at all**, because the mode did not exist
   when the measurement was taken. `invocation_lifetime_max` is therefore
   reasoned from the lifetime totals of window-mode projects — about 43 × the
   largest observed (138) — which makes it the weakest of the three numbers and
   the first that should be re-measured once explicit accounts have a history.
4. **None of it measured money.** The three numbers above are counts, taken
   over spawn records and transition logs, and they say nothing about what a
   project spends. The spend default of level 5 rests on a different corpus, a
   smaller one, and a restated definition of healthy, which is why
   §REQ-bounded-neural-work.7.5 states it separately rather than adding a row
   to §REQ-bounded-neural-work.7.2's table.

### 7.5. The spend default, and the four things it could not see

`defaults.spend_per_day` is **400.00** in the account's currency, and it is
measured rather than chosen — but over a different corpus and under a restated
definition of healthy, so it is stated here rather than added to
§REQ-bounded-neural-work.7.2's table as though it came from the same evidence.

The corpus is every `rhei.accounting.invocation.v1` record this workspace holds
under `~/ag/*/panta/`, with the synthetic
`tests/e2e/fixtures/accounting-archive/*.json` excluded for the reason
§REQ-bounded-neural-work.7 already gives. The unit is the **project-day**,
because that is what the bound bounds.

| dimension | n | median | max |
|---|---|---|---|
| measured spend per project per UTC day | 8 | $10.94 | $96.37 |
| measured cost of one invocation | 80 | $1.69 | $17.71 |

`defaults.spend_per_day` is 4 × $96.37, rounded to a legible number:
**400.00**. The multiplier and the reason for it are
§REQ-bounded-neural-work.7.3's, unchanged.

The **fallback reserve is 20.00**, the largest single invocation rounded up,
and it deliberately carries **no multiplier**. The ×4 of
§REQ-bounded-neural-work.7.3 is a margin on a *bound*; a reserve is an estimate
of one request, and the margin is already carried by the ceiling that estimate
is checked against. Multiplying here would charge the margin twice, and it
would charge it hardest against the invocations that cannot be measured at
all — the ones already paying the worst case
[§FS-rhei-budgets.6.2](../functional-spec/rhei-budgets.spec.md#62-reserve-then-settle).

Four things this measurement could not see. §REQ-bounded-neural-work.7.1 is
explicit that a measurement which does not say what it measured supersedes
nothing, so these are stated as plainly as the number itself:

1. **n = 8.** Eight project-days, against the 21 behind
   `defaults.invocations_per_day`. It is the smallest corpus any default here
   rests on.
2. **Healthy had to be restated.** §REQ-bounded-neural-work.7.1 defines a
   healthy sample as a ticket history ending in a terminal state, and an
   accounting record is not a ticket history — it belongs to one invocation.
   Here healthy is *every invocation record whose run is not the run that
   halted*. That is a weaker filter and a different one; it is not
   §REQ-bounded-neural-work.7.1's definition applied to a new corpus, and a
   re-measurement that wants to supersede this number has to say which of the
   two it used.
3. **Only 80 of 165 records carry an amount.** The rest were written by a
   transport whose usage could not be extracted or for a model the price book
   does not price. Both maxima above are therefore a lower bound on a lower
   bound.
4. **The failure this requirement is named after is not in the corpus.**
   `agent-grounds/rhei#232` at $75.58 is not among these records at all. The
   largest project-day the corpus does hold is $96.37, and that is what the ×4
   was taken over. A default measured from a corpus missing the very case it
   was written for is the first of the four numbers that should be re-measured.

A later measurement that repeats [§REQ-bounded-neural-work.7.1](bounded-neural-work.spec.md#71-healthy-defined-first)'s healthy definition over a wider corpus
supersedes these numbers. One that does not say what it measured does not.
