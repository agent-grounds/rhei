# AR-neural-admission: One serialized account beneath every neural start

The scheduler does not construct a neural subprocess directly. It submits a
resolved launch to the neural-admission component, which either returns an owned
reservation or a typed refusal. That one boundary sits beneath every scheduler,
embedded runtime, retry, poll attempt, fanout arm, and nested run, which is what
makes the counts of [§FS-rhei-budgets](../functional-spec/rhei-budgets.spec.md#fs-rhei-budgets-bounded-ticket-travel-and-project-invocations) true of *every* start rather than of the
ones someone remembered to route through it. It realizes [§REQ-bounded-neural-work](../requirements/bounded-neural-work.spec.md#req-bounded-neural-work-every-unit-of-neural-work-is-bounded-before-it-starts).

This component counts. It does not price, broker, confine, or qualify anything:
those belong to the provider-spend obligation on `agent-grounds/rhei#107`, and
importing them here would make a count depend on evidence a count does not need.

## 1. The boundary

```text
Admitted { reservation }
Refused  { reason_code, bound_snapshot }
```

The request carries the project identity, the ticket identity, the attempt
identity, the resolved launch tuple, the fanout group, and the ancestry token
where the run is nested. `travel_units` is `1` for the first arm of a group and
`0` for every later one, which is the fanout rule of [§FS-rhei-budgets.4.1](../functional-spec/rhei-budgets.spec.md#41-travel)
expressed as arithmetic rather than as a special case.

A source-level dependency rule keeps subprocess creation below this module, so
that adding a new spawn path cannot accidentally omit admission.
[§AR-agent-orchestrator-workflow.3.3](agent-orchestrator-workflow.spec.md#33-orchestrator-engine)

## 2. Components

| Part | Owns |
|---|---|
| `budget::types` | the identities, the bound snapshot with its two-valued provenance, and the refusal codes |
| `budget::account` | resolving a project to its account directory and uuid, and establishing an absent one |
| `budget::authority` | the external witness: open, verify byte-identical, adopt, append |
| `budget::journal` | the hash-chained receipt log, its lock, append-and-sync, and `adjust`'s floor |
| `budget::window` | the day key, the highest-day-key guard, and the renewal instant a halt quotes |
| `budget::replay` | deriving consumed and outstanding for the active contract from the chain |
| `budget::admission` | `preview`, `reserve`, `record_start`, `release_unstarted`, `release_travel`, and `authenticate_ancestor` |
| `budget::events` | the typed events the run's surfaces encode |

Settings resolution and the ceiling clamp stay where every other setting
resolves, in `rhei-cli::cli::settings_*`: the clamp is a step layered over that
unchanged chain rather than a second precedence system. The commands of
[§FS-rhei-budgets.10](../functional-spec/rhei-budgets.spec.md#10-rhei-budget) are a thin layer over `journal` and `replay`; validation and
dry run use a read-only transaction that shares resolution and replay code but
cannot append.

## 3. Lock order

The order [§FS-rhei-reset.2](../functional-spec/rhei-reset.spec.md#2-behavior) already fixes, with the two new locks appended so
that nothing deadlocks against reset or complete:

```text
metadata sidecars (sorted)
  -> task paths (sorted)
    -> execution-root locks (sorted)
      -> budget authority
        -> budget journal
```

An ordinary admission needs only the last two. Assigning a ticket identity that
does not exist yet additionally takes that ticket's metadata lock, which is why
metadata sits above rather than below. Releasing an unstarted reservation
([§FS-rhei-budgets.6.2](../functional-spec/rhei-budgets.spec.md#62-reserve-then-settle)) acquires the dead run's execution-root lock, and it does
so *before* the authority and journal locks like any other caller — the proof of
non-start is a lock acquisition, so it may not be attempted while holding the
account.

One fanout group is one journal transaction: every arm is validated, every unit
is reserved, the receipts are synced once, and only then do the launches
proceed. If any arm refuses, nothing is appended.

## 4. The chain and the witness

Each append writes the same bytes twice — to the witness first, then to the
project journal — and syncs both. A crash between the two leaves the witness
ahead, which is the **damaged** state of [§FS-rhei-budgets.5.4](../functional-spec/rhei-budgets.spec.md#54-absent-damaged-adopted) and refuses new
work naming both paths.

Opening verifies the whole chain: contiguous sequence, each line's
`previous_hash` equal to the SHA-256 of the exact preceding line's bytes, and
one project identity throughout. A journal that verifies but has no local
witness is **adopted** — the witness is written from the journal's own bytes.
That is the one place this design is deliberately looser than the money path it
was carved from, and the reason is that refusing every fresh clone would refuse
work that runs today.

The witness records every canonical root it has seen under its uuid, so a
project directory that is deleted and recreated is still recognized as the same
account.

## 5. Replay, not balance

No balance is stored. `consumed` and `outstanding` are derived by replaying the
chain under the active contract: for the lifetime contract, over every
reservation; for the window contract, over the reservations stamped with the
current day key.

This is why a window needs no renewal event, and therefore has nothing to forge,
replay twice, or lose ([§FS-rhei-budgets.3.3](../functional-spec/rhei-budgets.spec.md#33-the-window-is-a-key-not-a-refill)). It is also why lowering a ceiling
rewrites nothing: the ceiling is applied to the derived numbers at read time,
not baked into them at write time.

The day key comes from `budget::window`, which reads the clock through one
injectable seam ([§FS-rhei-budgets.3.3.1](../functional-spec/rhei-budgets.spec.md#331-reading-the-clock-from-the-environment)) and then takes the maximum of that day
and the highest day key the chain records. Every caller reads the clock through
that seam; a second direct read anywhere in the module is the bug this
centralization exists to prevent.

## 6. Ancestry

A nested run receives a non-secret descriptor naming the project, the parent
reservation, and the maximum descendant envelope in invocation units. It carries
no credential, because there is nothing to credential: the descendant charges
the same account through the same locks.

Admission authenticates the token by finding that reservation in **this
project's own journal**. It must exist, have a recorded start, not be released,
carry a descendant envelope above zero, and carry a deadline the descendant does
not exceed. Anything else is `ancestor_unavailable`; a descendant that would
take the ancestor's descendants past its envelope is
`ancestor_envelope_exhausted`. Both refuse before spawn, and replay re-derives
every ancestry claim from the chain rather than trusting a cached number.

## 7. Where it sits

The account is per **project** and lives outside `runtime/`, which is the one
place this design strains [§AR-rhei-panta.5](rhei-panta.spec.md#5-execution-root-and-per-rhei-runtime)'s per-rhei execution root: every
other durable thing Rhei writes belongs to one rhei's runtime tree. It is
deliberate. A count that lived under an execution root would be reset with it,
and a count that lived per rhei would let a project add a rhei to buy capacity.

There is exactly one call site in the run loop, at the existing pre-spawn point,
and one in the shared transition path where travel is charged. Adding a third is
how this boundary stops being one.
