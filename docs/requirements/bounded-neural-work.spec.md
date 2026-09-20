# REQ-bounded-neural-work: Every Rhei-started neural unit is bounded before it starts

Rhei makes autonomous work predictable only when starting it cannot create an
open-ended financial obligation. Every unit of neural work that Rhei starts,
directly or transitively, therefore has finite, engine-checkable time,
invocation, provider-billed spend, and ticket-travel bounds before the first
billable request is possible. This serves §GOAL-rhei-outcomes and applies on
every platform under §REQ-cross-platform.

## 1. Scope

The requirement covers every subprocess Rhei starts that can reach a model,
including built-in and custom agents, embedded uses of the runtime,
model-capable programs and callbacks, nested Rhei runs, fallbacks, extensions,
and descendants. Classification as an agent, program, or callback is not proof
that the process is bounded.

An externally launched worker-authority session is outside the requirement:
Rhei neither starts nor controls it. Non-neural infrastructure and shell costs,
flat subscriptions, and work performed outside a provider-billed neural
transport are outside the monetary promise.

## 2. Admission, not observation

Before spawn, one shared admission boundary must prove and durably reserve the
complete worst-case provider-billed charge of the resolved invocation. A
configured maximum, timeout, local token estimate, usage stream, or
after-the-fact threshold is not such proof. When any part of the possible
charge is unknown, unpriced, unsupported, delayed, or unconfined, admission
fails closed.

The bounds compose. An inner timeout, attempt count, visit count, poll count,
or request cap never extends an outer ticket or project bound; nested work fits
inside every ancestor reservation; and releasing an inner counter never
credits an invocation or provider-spend allowance.

## 3. Persistent outer bounds

One finite invocation allowance and one finite provider-spend allowance belong
to the lifetime of the Panta project. A bare rhei is an implicit one-rhei
Panta. Every member rhei, later descendant, resumed or narrowed run, and
concurrent process charges that same persistent allowance. Every ticket also
has finite lifetime travel. Reset, snapshots, re-instantiation, edits, copying,
run completion, process restart, and calendar time never create fresh
capacity.

Reservation is durable before spawn and atomic across competing processes.
Parallel fanout either reserves every arm and the scheduled ticket's one travel
unit or reserves nothing. Missing, unreadable, corrupt, or conflicting durable
state refuses new work rather than reconstructing a fresh balance.

## 4. Settlement and breach

Only complete, authoritative, uniquely matched, fully priced billing evidence
may settle a monetary reservation downward. A start that happened, or might
have reached the provider, keeps its invocation unit; incomplete or ambiguous
evidence keeps its full monetary exposure. Attempt refunds and provider
refunds do not by themselves refund either allowance.

Actual provider charges are retained even when a provider or containment
contract breaches its reserved maximum. A breached qualification is unusable
until its complete proof is re-established; increasing an allowance alone
does not make it qualified again.

## 5. Halt and visibility

Refused admission halts the affected ticket in its current state, exits the run
non-zero, and names the exact exhausted bound or missing evidence. It never
silently advances, fabricates a task result, rewrites an earned outcome, or
cancels already admitted work. An admitted invocation keeps its own containment
bound and may apply the edge whose travel unit it reserved.

Every surface that shows autonomous work also shows each applicable ceiling,
consumed amount, outstanding reservation, remaining capacity, qualification,
and precise halt reason while capacity is being spent, not only after
exhaustion.

## 6. Proof obligation

A transport is qualified only for an exact binary, configuration, operating
system, model, and provider billing contract after evidence proves capture
latency or a durable capture barrier, committed and in-flight request maxima,
post-kill provider exposure, and every nested or fallback path. Portable fake
providers prove engine arithmetic and recovery; they do not qualify a real
transport. Real qualification evidence must cover Linux, macOS, and Windows
before that tuple is enabled.
