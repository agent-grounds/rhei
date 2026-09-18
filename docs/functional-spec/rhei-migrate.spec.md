# FS-rhei-migrate: `rhei migrate`

`rhei migrate` provides explicit, reviewable compatibility operations for
authored plans whose text predates a stricter Rhei rule. Migration never makes
old text mean something different because of its inferred age: it rewrites the
authored language into the current valid form, after which ordinary validation
and execution apply unchanged. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

This specification defines only the direct export-producer dependency
migration. It is not a general validator-version framework.

## 1. Export-prior migration

```text
rhei migrate export-priors [RHEI_PLAN_OR_WORKSPACE]
rhei migrate export-priors --dry-run [RHEI_PLAN_OR_WORKSPACE]
```

`export-priors` repairs each otherwise-valid `**Consumes:** <producer>:<name>`
whose producer is missing from the consumer's direct `**Prior:**`. It does not
persist a format or validator version, infer when a plan was laid, or exempt
newly authored plans from [§FS-rhei-plan-language.3.12.1](rhei-plan-language.spec.md#3121-declaration-integrity).
Until migration writes the edge, the same unversioned text remains invalid for
both old and new plans.

The optional target accepts the same single-file plan, Directory Workspace,
Panta project, and upward discovery forms as `rhei validate`. There is no
`--rhei` option. A target that belongs to a Panta project widens to that entire
project even when it names one member file or workspace; a bare rhei remains a
one-rhei project. [§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)

### 1.1. Eligible relationships

A relationship is repairable only when its producer resolves to another task,
the producer declares the consumed export, and adding the direct edge produces
a valid complete graph. Several consumed names from the same producer require
one edge. Existing direct edges require none.

Relationship analysis distinguishes repairable missing edges from missing
producers, undeclared exports, self- or ancestor-consumption, and edges that
would introduce a duplicate dependency or cycle. Migration shares this
classification with validation; it does not implement a second interpretation
of export integrity.

### 1.2. Validation boundary

Before reporting or writing a migration, Rhei validates the complete loaded
project while provisionally treating only repairable direct edges as present.
Any independent parse, loading, state-machine, settings, dependency, export,
hierarchy, or execution-reference error refuses the command. Missing producers,
undeclared export names, self- or ancestor-consumption, duplicate dependencies,
cyclic proposed graphs, and unrelated invalidity therefore remain errors rather
than being hidden by a partial repair.

Export-file availability is not an authored-graph validation rule. A missing,
zero-byte, or whitespace-only required export does not prevent an eligible
rewrite, and migration never creates or changes export content. The ordinary
run preflight continues to refuse the consumer until the producer's nonblank
export exists. [§FS-rhei-plan-language.3.12.2](rhei-plan-language.spec.md#3122-consumer-availability)

## 2. Authored rewrite

Each edge is written to the file that owns the consumer. For a consumer with no
`**Prior:**`, Rhei inserts the field immediately after `**State:**`. Otherwise
it appends only absent producer references to the existing field, once each,
in first-appearance order across `**Consumes:**`. Existing references retain
their order and exact spelling.

A producer in the same rhei is written with its rhei-local id. A producer in a
different rhei is written with its project-qualified id. In either case the
reference uses the producer's declared node kind, including a custom kind,
rather than substituting `Task`.

### 2.1. Preservation and repetition

The rewrite changes only the bytes required to insert or append missing direct
producer references. It preserves unrelated authored bytes, line endings,
field and reference spelling, custom node kinds, task states, assignees,
results, and runtime artifacts. A graph that already has every required edge is
a byte-for-byte no-op. Re-running after a complete or partial migration adds no
duplicate and is likewise a byte-for-byte no-op for completed files.

`rhei list --blocked`, visualization, reset, and ordinary readiness need no
migration-specific interpretation: they consume the newly explicit authored
edge. A successfully terminal producer immediately satisfies that dependency;
a nonterminal or cancelled producer continues to block it under the existing
readiness rules.

## 3. Preview and output

`--dry-run` performs the same discovery, relationship analysis, validation,
locking, authoritative reread, and rewrite planning as a real migration, but
does not replace an authored file. It prints every proposed addition as:

```text
Would add <producer-reference> to <consumer-kind> <consumer-id> **Prior:** in <path>
```

A real migration prints each completed addition with `Added` in place of
`Would add`. Both modes end a nonempty successful operation with:

```text
Added N direct Prior edge(s) in M file(s)
```

For dry-run the summary describes what an immediate real invocation would add;
`Added` is the stable summary verb, not a claim that preview mutated a file.
When no relationship needs repair, the command prints `No export Prior
migration needed` and leaves every authored file byte-for-byte unchanged.

Paths and additions are deterministic: files use canonical path order, then
consumers use source order, then missing producers use their first `Consumes`
order. Counts are deduplicated by consumer-producer edge and by owning file.

## 4. Exclusion and commitment

Migration contends with execution and every other plan writer. Before preview
or mutation it tries, without waiting, to take the existing run lock for every
involved execution root in canonical order. Any live-run contention refuses
the whole operation and names the root; migration never edits a plan being
executed.

It then takes every affected plan's permanent sibling sidecar in canonical
path order and holds the complete set through its decision and commitment.
After acquiring the locks it re-reads the authoritative project, recomputes the
relationships and destinations, revalidates the complete proposed graph, and
stages every rewritten file. A relationship or file that changed while locks
were being acquired is decided from this reread, never from stale bytes.
[§AR-agent-orchestrator-workflow.3.3.1](../architecture/agent-orchestrator-workflow.spec.md#331-stable-writer-exclusion)

### 4.1. Replacement failures

No failure before the first replacement changes an authored plan. Every staged
file is installed by same-directory atomic replacement while its stable
sidecar remains held, so atomicity is per file; Rhei does not claim a
project-wide filesystem transaction.

If a later replacement fails, migration exits nonzero and separately lists the
files already completed and the files still remaining. A completed file
contains only edges that were valid in the prevalidated complete proposed
graph. The remaining files retain their authoritative pre-invocation bytes.
An idempotent retry replans the mixed state, skips completed edges, and can
finish the remainder.

## 5. Recovery sequence

For an otherwise-valid consumed relationship missing only its direct edge,
`rhei validate TARGET` and `rhei run TARGET` remain read-only and fail nonzero.
They name the consumer and producer and end their actionable diagnostic with a
copyable command:

```text
help: rhei migrate export-priors TARGET
```

An explicitly supplied target retains its shell-safe spelling in the help; an
omitted target is replaced by the discovered project or bare-rhei path so the
line remains copyable. Neither command performs migration implicitly, and
`validate --watch` never migrates after a file event.

After migration, `rhei validate` succeeds and retains the ordinary graph-level
`**Consumes:**` advisory. `rhei run` then applies ordinary dependency readiness
and export preflight. Thus a completed producer with an available declared
export releases the repaired open consumer without per-task hand edits, while
an incomplete producer or unavailable export still blocks it.

## 6. Discoverability and release record

The command appears in top-level CLI help under Authoring, shell completions,
the root README's CLI usage, and the functional-specification index. The
release changelog describes the explicit recovery command, the additive
validate/run help, the authored `Prior` diff, and the unchanged strict rule for
new plans. The changelog entry receives its pull-request number when the draft
pull request exists.
