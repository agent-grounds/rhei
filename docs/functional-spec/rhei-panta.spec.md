# FS-rhei-panta: Panta, the project root above all rheis

Panta is the single, invisible root of a Rhei project. It sits above every rhei
and every ticket, gives new rheis a default home, and is the one anchor from
which an operator can see the whole project as a single graph. Making the whole
project visible from one root, and keeping "add a rhei" a zero-friction action,
serve Rhei's monitoring and predictability goals. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

The name follows *panta rhei* ("everything flows"): Panta is the still point that
contains all the flows. The decision and its rationale are recorded in
[§DA-panta-root](../decisions/architectural/panta-root.md#da-panta-root-panta-is-the-per-project-virtual-root-above-all-rheis); the load model, on-disk layout, and id rules are specified in
[§AR-rhei-panta](../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture).

## 1. What Panta is

A Rhei **project** has exactly one Panta. Panta is a *virtual* node: it is never
authored, never written to a file as a node, and has no `**State:**`,
`**Prior:**`, `**Assignee:**`, or `> **Result:**`. It is the level-0 root of the
node hierarchy:

```
Panta            the project        (virtual, exactly one, kind `panta`)
├── Rhei  auth   a flow / plan       (kind `rhei`)
│   ├── Task auth.1   a ticket       (kind `task`)
│   └── Task auth.2
├── Rhei  billing
│   └── Task billing.1
└── Rhei  basin  the project basin   (synthetic when `basin/` exists)
    └── Task basin.3   an unfiled ticket
```

A **rhei** is a plan — a self-contained flow with its own tasks. A **ticket** is
a unit of work inside a rhei (the node kind is `task` by default; "ticket" is the
user-facing name for a work item). Panta owns the rheis; rheis own their tickets.

`panta` is a reserved node kind. It can never be authored, can never appear in
`structure.nodeKinds`, and there is never more than one Panta in a project.

## 2. Default home for new rheis

Creating a rhei without specifying where it goes places it under Panta. Panta is
the implicit default parent, so adding a rhei takes no location argument:
dropping a `<id>.rhei.md` file or a workspace directory into the project
directory is enough, and discovery picks it up on the next load
([§AR-rhei-panta.1](../architecture/rhei-panta.spec.md#1-on-disk-layout)). `rhei new` scripts that action, and writing the file by
hand stays exactly as valid ([§FS-rhei-new](rhei-new.spec.md#fs-rhei-new-rhei-new)):

```bash
rhei new "Authentication"                  # a rhei under Panta
rhei new "Rotate keys" --under auth        # a ticket inside rhei auth
```

There is no `--under` for a *rhei*, because there is nowhere for it to point.
The hierarchy is exactly Panta -> rhei -> ticket (§1), a rhei id is a single
segment, and discovery never descends past the project directory's immediate
children ([§AR-rhei-panta.1](../architecture/rhei-panta.spec.md#1-on-disk-layout)): a rhei nested inside another rhei would not be
found. `--under` therefore names where a *ticket* goes — the sense in which
new work is ever placed somewhere other than the default.

A ticket created with no owning rhei is placed in the project **basin**. The
basin is loaded as a level-1 rhei with id `basin`, so quick captures do not
require choosing a domain rhei first while the hierarchy remains Panta -> rhei
-> ticket. Basin tickets use ordinary rhei-local ids and project-wide ids such
as `basin.3`. `rhei new "<title>" --under basin` is the capture path, creating
`basin/` on first use ([§FS-rhei-new.3](rhei-new.spec.md#3-creating-a-ticket)).

`basin` is a permanently reserved rhei id, independent of whether any basin
content currently exists: a discovered domain rhei with id `basin` is a
load/validation error. Reserving it unconditionally avoids a
delayed-migration trap where a domain rhei named `basin` is valid until the
first unfiled ticket appears. Filing a basin ticket into a domain rhei is a
reparenting operation that changes its project id from `basin.<local-id>` to
`<target-rhei>.<local-id>`.

## 3. One unified view

Because every rhei hangs off the same Panta, a project loads and renders as one
graph rather than as many disconnected plans:

- **Status rolls up** Panta ← rheis ← tickets through one tree. Panta's status is
  always derived from its rheis and is never stored.
- **Dependencies resolve across rheis.** A ticket in one rhei may declare a
  `**Prior:**` on a ticket in another rhei; the reference resolves against the
  whole project graph. [§AR-rhei-panta](../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture)
- **Listing and monitoring** read the one merged graph: `rhei list` prints every
  ticket under its project-qualified id in a flat listing, indenting rhei-locally
  so the rhei prefix marks ownership ([§FS-rhei-list.4.1](rhei-list.spec.md#41-text-default)). Grouping the output
  under rhei-level headings with a per-rhei status rollup is deferred, tracked
  on the roadmap.

## 4. Invisibility

Panta is not shown to users as a node. Default listing, claim selection
(`rhei next`), rendering, and monitoring present rheis as the top level and omit
Panta. Runtime commands must never claim, transition, complete, cancel, or reset
Panta — it has no state to move. Tooling may reveal the root only behind an
explicit opt-in flag (for example `--show-root`) for debugging; the default
output never mentions it.

The synthetic `basin` rhei is **de-emphasized, not hidden**. Unlike Panta, it is
a real rhei that participates fully in readiness, scheduling, execution, and
rollup — `rhei run` and `rhei next` treat its tickets like any other rhei's. But
because its tickets are unfiled quick-captures rather than planned work, it is
ordered last: the basin loads after every discovered rhei, so its tickets come
last in default listing and in every surface that walks the merged graph.
Presenting it in a visually de-emphasized form (for example dimmed or
collapsed) so it never competes with planned rheis is deferred presentation
work, tracked on the roadmap; no surface implements it yet. It is
never placed behind an opt-in flag the way Panta is: unfiled work must stay one
glance away so it gets triaged rather than silently accumulating unseen.

## 5. Identity

A rhei is addressed by its id (for example `auth`). A ticket is addressed by its
project-wide path, formed by joining its rhei id with its rhei-local id
(`auth.1`, `auth.1.2`, `basin.3`). This makes ticket identities unique across
the whole project without authors coordinating ids by hand. The exact
id-extension and grammar rules are specified in [§AR-rhei-panta](../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture).

## 6. Project scope and command behavior

Every command resolves a **scope** from the target it is given. Pointed at a
project — a directory containing `index.panta.md`, or invoked inside one — a
command operates on the whole project. Pointed at a single rhei (a `.rhei.md`
file or a rhei workspace directory) it operates on that rhei alone. `--rhei <id>`
(repeatable) narrows a project-scoped invocation to named rheis.

**A target that carries no name of its own still names a rhei.** A workspace
directory reached as `.`, `./`, or `..` from a subdirectory — or named by the
`index.rhei.md` inside it, with or without a `./` prefix — carries no last
component to take an id from, so the id comes from *where the path resolves*:
the same rhei, with the same id, that `../billing` or an absolute path names
([§AR-rhei-panta.3](../architecture/rhei-panta.spec.md#3-identity-and-id-namespacing)). The current directory is the spelling an author reaches for
first, from inside the workspace they are already standing in, and it must not
be the one spelling that fails.

A path that *does* carry a name keeps it, and resolution never overrides it: a
symlink `billing` pointing at the workspace `myws/` is the rhei `billing` when
it is named directly, and `myws` when named from inside as `.`. Those are two
ids, so their tickets and the runtime artifacts named after them stay apart —
address one workspace one way.

**A rhei that belongs to a project always loads through it.** Pointing a command
at `panta/billing.rhei.md` loads the project and narrows to `billing` — it is
`--rhei billing`, not a plan read in isolation. A member rhei cannot be
understood alone: its `**Prior:**` may point across rheis ([§AR-rhei-panta.3](../architecture/rhei-panta.spec.md#3-identity-and-id-namespacing)) and
its state machine comes from the manifest ([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding)). Loading the file
by itself made a *correct* plan fail, because a cross-rhei prior has nothing to
resolve against, so `rhei validate <member>` reported errors that
`rhei validate` on the same project did not — false failures for any per-file
CI or pre-commit check. An explicit `--rhei` on the same invocation wins over
the id implied by the path.

Membership follows the same rule as identity: the project is the directory that
encloses *where the target resolves*, not the one left when its last component
is dropped. A member named `..` from its own `tasks/` is the member, loaded
through its project — reading it as the `tasks/` directory's neighbour would
load the member alone and call its valid cross-rhei prior missing.

One command does not narrow: `rhei validate` takes no `--rhei` at all
([§FS-rhei-validate.1.1](rhei-validate.spec.md#11-why-there-is-no---rhei)), so pointing it at a member rhei validates the whole
project and says so.

`rhei cost` and `rhei summary` load through the project like every other
command — they read the plan for its tickets and its state machine, and a
member's cross-rhei `**Prior:**` has to resolve for that reading to be right.
What does not widen with the load is the **accounting root**: pointed at a
member, they read that member's accounting, not the project's (§6.5). The two
were once written here as commands that "stay on the path they were given",
which described neither half correctly — the load always widened, and the
accounting root widened with it, which is the defect §6.5 exists to settle.

An **empty project** — an `index.panta.md` with no rheis yet, the state
`rhei init` leaves behind — is a valid project, not an error. Read commands
treat it as zero tickets: `rhei list` says the project has no tickets yet and
how to add one, and exits successfully. `rhei validate` succeeds but warns
that discovery found no rheis and restates where plans must live — a plan
misnamed (missing the `.rhei.md` suffix) or misplaced would otherwise be
silently invisible behind a green validation. Only work-claiming and mutation
surface the emptiness as their ordinary no-work outcomes.

A rhei that **fails to load** — a malformed heading, a bad frontmatter block —
fails the project load for every command that must reason about the whole
graph: validation, work claiming, transitions, runs, and resets all stop and
report the parse error, because a partial graph cannot decide readiness and a
cross-rhei `**Prior:**` into the broken rhei has nothing to resolve against.

`rhei list` is the exception. It is the surface an author reaches for *while* a
plan is broken, and failing it left no way to see the rest of the project —
not even `rhei list --rhei <healthy-one>`, which names a rhei that parses
fine. It therefore skips what it cannot load, prints one warning per skipped
rhei naming the file and the parse error, and lists everything else.

"Cannot load" covers the rhei's **identity** as well as its contents. An entry
whose id is malformed, reserved (`basin.rhei.md`), or already taken by another
entry joins the project no more than one that fails to parse, and the author
repairing it needs the same view of everything else meanwhile — dropping a
`basin.rhei.md` into a project otherwise wedged `rhei list` completely. A
duplicate id skips the colliding entry, not the one that claimed the id first,
so the healthy half of the collision keeps listing. Every other command still
fails on all of these: an ambiguous or missing rhei id makes ticket ids
ambiguous, and no command that resolves a graph can proceed on that.

A command invoked with **no target** resolves one by walking up from the
current directory, nearest match first. At each level, in order:

1. a directory containing `index.panta.md` is the project;
2. a directory whose `panta/` child contains `index.panta.md` resolves to
   that child — the conventional project folder `rhei init` creates
   ([§FS-rhei-init](rhei-init.spec.md#fs-rhei-init-rhei-init)), so bare commands work from the whole host repository;
3. a directory containing `index.rhei.md` is that workspace rhei;
4. in the invocation directory only: a directory containing exactly one
   rhei — counted the way project discovery counts them ([§AR-rhei-panta.1](../architecture/rhei-panta.spec.md#1-on-disk-layout)):
   a `*.rhei.md` file or a Directory Workspace subdirectory, hidden
   dot-prefixed names skipped — is that rhei.

Rule 4 never applies to ancestors. An explicit manifest (rules 1–3) is an
opt-in safe to adopt from any distance, but a loose plan file is incidental:
adopting one far above the invocation directory would let a forgotten
`notes.rhei.md` in a home directory silently receive writes from any
subdirectory below it.

An invocation directory holding more than one rhei but no `index.panta.md`
is ambiguous: the error names the candidates and both fixes — pass one
explicitly, or run `rhei init` ([§FS-rhei-init](rhei-init.spec.md#fs-rhei-init-rhei-init)) to make the directory a
project. When the walk reaches the filesystem root without a match, the
error says what was searched for and how to point the command at a plan, not
merely that a required argument is missing.

`rhei reset` is excluded from omitted-target resolution: it destroys runtime
state across its whole scope (§6.4), so an inferred target would turn a
mistyped or misplaced invocation into irreversible data loss. Invoked with no
target it fails, explaining that reset takes an explicit plan or project.

Within a project, every command — read-only and mutating alike — operates
**project-wide by default**. Loading, validation, listing, and rendering read
the merged project graph; `rhei run`, `rhei next`, `rhei transition`,
`rhei complete`, and `rhei reset` mutate it, routing every state, assignee,
result, and runtime-artifact rewrite back to the owning rhei file. Because a
single rhei loaded directly is the sole rhei of an implicit Panta
([§AR-rhei-panta.2](../architecture/rhei-panta.spec.md#2-load-model)), there is no separate "bare rhei" command path: targeting one
rhei is simply a one-rhei project, and `--rhei <id>` narrows a multi-rhei project
to named rheis.

The project is the unit an operator drives. Because they fan out across every
in-scope rhei, `rhei run` and `rhei reset` report their resolved scope and the
affected rheis before acting. The report exists for fan-out, so a one-rhei
project — the implicit Panta wrapping a bare rhei — has nothing to disambiguate
and stays quiet: no `Scope:` line is printed unless the invocation reaches more
than one rhei or the target is an explicit Panta project.

An id passed to `--rhei` that names no rhei in the project is an error listing
the available rhei ids, rather than a silently empty scope.

A ticket target passed to a command (`rhei complete <id>`, `rhei transition
<id>`, `rhei release <id>`, …) is either the project-qualified id (`auth.1`) or
a rhei-local shorthand (`1`). Every such command takes it the same two ways —
positionally or through `--task` ([§FS-rhei-usage.2](rhei-usage.spec.md#2-coordination-through-the-state-machine)). A rhei-local target resolves only when exactly one in-scope
rhei contains that ticket; ambiguity across rheis is an error that names the
qualified candidates. Output, artifacts, and ledgers always use the qualified
id regardless of how the target was written.

Runtime ticket metadata (`metadata.tasks.<id>.*` — visit counters, poll timers)
is written to the metadata document of the rhei that owns the ticket, keyed by
that document's own id space: a workspace rhei's `index.rhei.md` under
rhei-local ids, a single-file rhei's own frontmatter under rhei-local ids.

The synthetic `basin` rhei has no authored index to hold that metadata, so its
tickets store it in `index.panta.md` under their **project-qualified** ids
(`basin.3`) — the same keys the merged graph reads them back from. Without a
metadata document, basin tickets could not transition at all: every command
that advances a ticket must read and write its counters.

The state machine is per-rhei, defaulted by the project. The `index.panta.md`
declaration — or the built-in `rhei` machine when the manifest declares none —
governs every rhei that does not declare its own `**States:**`, plus the
synthetic `basin` rhei and the Panta root's node policy. A rhei that declares
its own machine runs under it ([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding)): a machine is a *process*,
and one project holds several processes the moment it holds two instantiated
templates. Restating a custom default is legal and inherits the same effective
machine *name*, but its explicit declaration still gives a matching
member-local `states.yaml` precedence. Restating the built-in `rhei` default
is the exception: that precedence does not apply, and the restatement is
equivalent to omission in every respect. Omitting the line instead inherits the
already-resolved project machine wholesale. Each ticket validates, transitions,
and completes under its owning rhei's machine; the only place two machines meet
is a cross-rhei prior, judged under the target's machine (§6.1).

### 6.1. Readiness and `rhei next`

Readiness is **project-global**. A ticket is ready when it is claimable — its
own descendants, if any, all terminal ([§FS-rhei-next.3](rhei-next.spec.md#3-default-behavior-claim-mode)) —
and every `**Prior:**` is terminal-and-not-cancelled, resolved across the whole
project graph — a ticket in one rhei may be blocked by a ticket in another.
Terminal status of each prior is judged against the machine of the rhei that
*owns the prior* ([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding)) — the target ticket's states mean what its
own process says they mean, wherever the waiting ticket lives.
Rheis and Panta are structural rollups and are never claimable. `--rhei` narrows
the candidate tickets but never narrows where their priors resolve: a candidate
may still be blocked by a prior outside the named rheis. A ticket named
explicitly with `--task` must itself be in scope; targeting a ticket outside the
named rheis is an error rather than a silent widening.

Because that is the one interaction a narrowed invocation cannot show in its
own scope, the no-work diagnostic must explain it rather than describe
out-of-scope work: under `--rhei` it names the scope, reports only in-scope
tickets, and marks a blocking prior that lives outside the scope as such
(`Task auth.1 (pending, outside the --rhei scope)`). Reporting an out-of-scope
ticket as the work in progress is wrong — it reads as a bug in narrowing.

Claim mode writes the `**Assignee:**` into the owning rhei's file, resolved
through the source map ([§AR-rhei-panta.2](../architecture/rhei-panta.spec.md#2-load-model)). `--peek` is read-only and never
writes.

### 6.2. `rhei run`

At project scope, `rhei run` orchestrates ready tickets across all in-scope rheis
under one loop, applying the project state machine ([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding)). It drives
tickets to terminal states; it never writes state to a rhei or Panta node. Concurrency
across rheis is bounded, and each spawned unit is attributed to its rhei in logs
and accounting. The loop stops when no eligible ticket remains in scope or a
gating state requires a human.

Before spawning, `rhei run` reports the resolved scope and the rheis it will
touch (§6). A bare rhei runs as the single rhei of its implicit Panta, so the
project-wide loop is the only execution path.

### 6.3. Completion and rollup

Result artifacts and their in-plan links are keyed by the **project-qualified**
ticket id: completing `auth.1` writes `runtime/results/auth.1.md` under the
owning rhei's execution root and links it as `[auth.1](runtime/results/auth.1.md)`.
Because every ticket gained its rhei prefix when bare rheis became implicit
Pantas, a plan completed before that change carries the rhei-local link
(`[1](runtime/results/1.md)`) beside a rhei-local artifact. Validation **accepts
that legacy form** so existing plans keep validating, and a ticket that is not
being completed keeps its existing link — an already-completed ticket keeps
pointing at the artifact that actually holds its result. Completing a ticket is
the one exception: `rhei complete` writes the qualified artifact, so it also
**refreshes that ticket's result link** to the file it just wrote — leaving a
legacy link beside a fresh qualified artifact would leave the plan green while
pointing at a file that no longer receives writes. There is no migration pass:
untouched tickets keep their form, and the two forms coexist in a long-lived
plan.

A result link is validated as a **pair**: text and target must describe the same
ticket id, both qualified (`[auth.1](runtime/results/auth.1.md)`) or both
rhei-local (`[1](runtime/results/1.md)`). A link that mixes the two forms, or
names any other id, is an error.

`rhei complete` finishes a ticket, leaf or not — a non-leaf ticket is a task in
its own right and is finished the same way, once its own descendants are
terminal ([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)). A rhei is done when all its tickets are
terminal, and Panta when all rheis are done, but this status is **derived, not
stored**: rheis and the virtual Panta have no `**State:**` to write, so no
cascade stamps `completed` up the tree — doneness is computed on read. Giving a
rhei an explicit profile through `node_policy.rhei`, so that it carries state
and rolls up like a non-leaf ticket, is deferred: the merged graph today has no
rhei nodes for a profile to bind to, so the key has no effect. Tracked on the
roadmap.

### 6.4. Reset, validate, list, viz

- `rhei reset` is project-wide by default; because it destroys runtime state
  across every in-scope rhei, it always takes an explicit target (§6) and
  surfaces the scope and the affected rheis before
  acting. `--rhei` narrows it. A narrowed reset removes only the runtime
  artifacts owned by in-scope tickets rather than whole `runtime/` trees:
  sibling single-file rheis share one execution root, so removing the tree
  would destroy an out-of-scope rhei's state. "Owned" means keyed by an
  in-scope ticket id — its result file, logs, declared artifact-contract paths,
  snapshot sessions, worktree refs, accounting captures and task index, its
  lines in the transition ledger, and its runtime ticket metadata (visit
  counts, poll timers) in the owning rhei's index. Leaving any of those behind
  would be a silent partial reset: a stale declared output can satisfy a
  required input on the next run, a stale ledger line claims a completion the
  plan no longer holds, and a stale visit count makes a counted loop resume
  mid-flight instead of restarting. Runtime records written before project
  qualification are keyed by the rhei-local id; the reset sweeps those legacy
  keys too, but only at an execution root whose every rhei is in scope —
  sibling rheis sharing a root make bare local ids ambiguous. Run-scoped output — the run report, the dashboard, accounting rollups —
  is not ticket-owned; a narrowed reset keeps it and **says so**, rather than
  letting the operator discover the difference ([§FS-rhei-reset.2.1](rhei-reset.spec.md#21-narrowed-reset---rhei)).
- `rhei validate` always checks the whole project graph: cross-rhei dependency
  resolution, project-qualified id uniqueness, rhei-id validity, and the reserved
  `panta`/`rhei` kinds.
- `rhei list` is project-wide, printing every rhei's tickets under their
  qualified ids ([§FS-rhei-list.4.1](rhei-list.spec.md#41-text-default)); existing filters
  (`--ready`, `--state`, `--assignee`, kind) apply across the project, and
  `--rhei` filters to a rhei. The `basin` rhei's tickets are ordered last in
  default output; visual de-emphasis is deferred (§4).
- `rhei viz` renders the merged project graph in the same id space the CLI
  uses, so its tickets carry their qualified ids (`auth.1`): every rhei's
  tickets in one graph, cross-rhei dependency edges drawn, and Directory
  Workspace rheis included. A member rhei renders that graph narrowed to itself,
  keeping the one-hop neighbours its priors point at ([§FS-rhei-viz.7.3](rhei-viz.spec.md#73-panta-projects-render-as-one-graph)). Panta
  is the implicit canvas, never a drawn root box. Rheis as *visually grouped*
  top-level bands, and a `basin` group placed last and de-emphasized, remain
  presentation work tracked on the roadmap (§4).

### 6.5. Cost and summary

`rhei cost` and `rhei summary` read accounting artifacts. Every other command
in §6.4 acts on tickets, and a ticket belongs to exactly one rhei; a record
belongs to an **accounting root**, and a project has several. What each
spelling means is therefore a question those commands alone have to answer.

Both take a positional and resolve it exactly as §6 says, and both take
`--rhei <id>` (repeatable), spelled and behaved as `rhei list`'s: an id the
project does not hold is an error naming the available ids, and an explicit
`--rhei` wins over the id the path implies. From that the invocation has a
scope — named rheis, or the whole project — and the scope decides which
accounting roots are read:

- **A member**, however spelled — the directory, its `index.rhei.md`, `.` or
  `..` from inside it, or nothing at all from within it — reads that member's
  accounting root and no other. A project spelling with `--rhei <id>` reads
  the same thing.
- **The project** reads the union: the run root, plus the execution root of
  every rhei in it ([§AR-rhei-panta.5](../architecture/rhei-panta.spec.md#5-execution-root-and-per-rhei-runtime)), as one set. `basin` is a participating
  execution root like any other and is read when `basin/` exists.

An accounting root is `runtime/accounting/` under a root so enumerated
([§FS-rhei-cost-accounting.2](rhei-cost-accounting.spec.md#2-runtime-files)). Two rules keep the union honest, because the
enumeration is not one-to-one in either direction:

1. **Roots are deduplicated by canonicalized path, never by spelling**
   ([§REQ-cross-platform.5](../requirements/cross-platform.md#5-paths-are-data)). Single-file rheis share the project directory as
   their execution root (§AR-rhei-panta.5), so the same root is reached
   several times and a root reached twice would count its records twice.
2. **The root decides when it belongs to one in-scope rhei; a record's
   `task_id` decides only where one root is shared.** Under `--rhei` naming a
   single-file rhei, the shared project root is read and its records filtered
   to that rhei's tickets. Filtering by `task_id` everywhere would drop the
   records written before tickets were project-qualified (§6.3); deciding by
   root everywhere would report every single-file rhei under a `--rhei` naming
   one of them.

One attempt is counted once. New records use their attempt-scoped
`invocation_id`; historical records use the legacy inference in
[§FS-rhei-cost-accounting.3.7](rhei-cost-accounting.spec.md#37-attempt-identity). Exact copies count once within one root or across roots, while
distinct same-run or cross-run attempts remain separate and each contributes
its measured usage and cost. A historical record without `run_id` remains
unattributed and contributes to whole-workspace history. Conflicts are handled
under [§FS-rhei-cost-accounting.11](rhei-cost-accounting.spec.md#11-failure-modes), never silently collapsed.

A root that holds no accounting directory, or an empty one, contributes
nothing and is not an error — a member that has never been run is an ordinary
member.

`rhei cost --task <id>` is bounded by that same scope, exactly as §6.1 bounds
`rhei next --task`. A ticket named with `--task` must itself be in scope,
however the scope was reached — `--rhei`, or the member the positional named —
and naming one outside it is an **error** that gives the scope and the rhei the
ticket belongs to, rather than a silent widening or an empty answer. Its
records sit in a root the reading never opened, so reporting no direct and no
subtree total would be an empty answer no reader can tell from a ticket that
genuinely cost nothing — the confusion this point exists to remove. A `--task`
id **no rhei in the project holds** is not that error: it is unknown, and the
reading goes on to report it as unknown, which is a different thing to be told.

Neither command writes, migrates records, or widens what it reads beyond the
scope it resolved. Where the records are *written* is not changed by any of
this: a run writes each invocation record under the execution root of the rhei
that owns the ticket, and its capture streams under the run root
([§FS-rhei-cost-accounting.5.1](rhei-cost-accounting.spec.md#51-price-book-selection)). This point is about the reading.

## Related Specifications

- [Plan Language](rhei-plan-language.spec.md) — grammar, the node hierarchy, and the virtual-root model [§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)
- [Panta Architecture](../architecture/rhei-panta.spec.md) — load model, on-disk layout, id rules [§AR-rhei-panta](../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture)
- [Panta Root Decision](../decisions/architectural/panta-root.md) — why Panta is a unified virtual root [§DA-panta-root](../decisions/architectural/panta-root.md#da-panta-root-panta-is-the-per-project-virtual-root-above-all-rheis)
