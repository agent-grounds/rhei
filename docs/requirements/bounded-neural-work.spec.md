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

## 1. The four levels

Bounding one unit of neural work is four separate questions, and no level
substitutes for another:

| Level | Bounds | Realized by |
|---|---|---|
| 1. One round | how long one orchestrated agent turn may run | `agent_timeout` [§FS-rhei-agents.7.1](../functional-spec/rhei-agents.spec.md#71-configuration) |
| 2. One visit | how many times a single state entry may be spawned | `attempts:` [§FS-rhei-agents.3.2.3](../functional-spec/rhei-agents.spec.md#323-attempt-budget) |
| 3. One ticket | how many moves a ticket may make in its whole life | `transition_limit` [§FS-rhei-budgets.2](../functional-spec/rhei-budgets.spec.md#2-where-a-bound-comes-from) |
| 4. One project | how many neural starts a project may be admitted | the invocation contract [§FS-rhei-budgets.3](../functional-spec/rhei-budgets.spec.md#3-the-invocation-contracts) |

Levels 1 and 2 are the ground as it stood before this requirement. Levels 3
and 4 are what it adds, and both are **counts** — not time, and not money.

Two adjacent state-entry bounds are neither of these four and are not changed
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

## 3. The two counts

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

## 4. Nothing creates capacity

Neither count is ever created by an operation that is not one of its two lawful
sources. `rhei reset`, a fresh `rhei run`, `--rhei` selection, snapshots,
appended work, live member admission, a restart, a copy, a move, and a nested
runtime create no travel and no invocation capacity, and in window mode none of
them advances the window.

Fresh invocation capacity has exactly two lawful sources: the passage of the
window, and an audited `init` or `adjust` under the machine's ceiling. Fresh
travel has exactly one: a genuinely new ticket identity. A ticket that is
copied, moved, or re-instantiated keeps its history, so reset-and-rerun
converges on the travel bound rather than escaping it.

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

Provider-billed spend is a fifth bound, and it is not here. It needs a request
broker, credential, egress and process-tree confinement, and the qualification
of a real transport, none of which a count needs. It stays on
`agent-grounds/rhei#107`.

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

Three gaps, stated rather than papered over, because they bound how much the
numbers above are worth:

1. **Retries are undercounted.** Two retries of the same visit overwrite one
   `runtime/spawns/` record, so the invocation counts are a **lower bound**, not
   an exact count. The ×4 margin absorbs it.
2. **Accounting is sparser than spawning.** The accounting roots hold 163
   invocation records against 436 agent spawns, because usage extraction is not
   supported for every transport. That is itself the argument for admission
   reading a count ledger of its own rather than accounting records
   ([§FS-rhei-cost-accounting](../functional-spec/rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)).
3. **There is no explicit-mode history at all**, because the mode did not exist
   when the measurement was taken. `invocation_lifetime_max` is therefore
   reasoned from the lifetime totals of window-mode projects — about 43 × the
   largest observed (138) — which makes it the weakest of the three numbers and
   the first that should be re-measured once explicit accounts have a history.

A later measurement that repeats [§REQ-bounded-neural-work.7.1](bounded-neural-work.spec.md#71-healthy-defined-first)'s healthy definition over a wider corpus
supersedes these numbers. One that does not say what it measured does not.
