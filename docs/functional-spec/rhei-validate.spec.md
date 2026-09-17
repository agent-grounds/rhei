# FS-rhei-validate: `rhei validate`

Validate a Rhei plan or Directory Workspace against the Rhei plan language,
the resolved state machine, project settings, and runtime context checks. The
command is read-only and exists to make execution predictable before a worker
or orchestrator mutates plan state. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

## 1. Usage

```bash
rhei validate [RHEI_PLAN_OR_WORKSPACE]
rhei validate --watch [RHEI_PLAN_OR_WORKSPACE]
rhei --state-machine <PATH> validate [RHEI_PLAN_OR_WORKSPACE]
```

`<RHEI_PLAN_OR_WORKSPACE>` may be a single `.rhei.md` file, a Directory
Workspace root, or a Panta project directory; omitted, the target is resolved
by walking up from the current directory ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)). When a workspace
root is passed, validation loads `index.rhei.md` and the workspace task files.
A project target validates the whole merged graph.

### 1.1. Why there is no `--rhei`

Unlike `rhei list`, `rhei run`, and `rhei reset`, `rhei validate` takes no
`--rhei` narrowing flag. Narrowing those commands selects *tickets to act on*,
which is well defined. Narrowing validation would have to select *diagnostics
to report*, and a project's diagnostics are not partitioned by rhei: the state
machine, merged settings, link bases, and cross-rhei `**Prior:**` resolution
are all project-wide, and a load failure in one rhei is what stops the others
from resolving. A flag that filtered the reported subset would hide real
errors behind an apparently narrower green — the opposite of what validation
is for. Validate the project; the diagnostics name their own rhei.

## 2. Options

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--watch` | No | false | Re-run validation when the plan or resolved states file changes |
| `--state-machine <PATH>` | No | built-in/default discovery | Global option selecting an explicit states YAML file |

## 3. State Machine Resolution

Validation uses the state-machine resolution order defined in the
[Plan Language Specification](rhei-plan-language.spec.md#13-state-machine-resolution):
explicit `--state-machine <PATH>` first, rhei-local `**States:**` declarations,
Panta default inheritance for rheis that omit `**States:**`, omitted effective
declarations as the built-in `rhei` machine, declared `**States:** rhei` with
built-in fallback, and declared custom names only when a matching
auto-discovered file is available.

If a plan declares a non-default state machine name and no matching
auto-discovered file is available, validation fails and directs the caller to
pass `--state-machine`.

## 4. Behavior

1. Load and parse the plan. Single-file validation and Directory Workspace task
   file validation collect every recoverable parse error before returning so
   users can fix related issues in one pass. Workspace index parse errors remain
   fail-fast because later task-file diagnostics may depend on index structure.
2. Resolve the state machine and validate plan semantics, including state
   values, task ids, dependencies, node policy, terminal and gating states,
   counted-loop syntax, artifact contracts, and task read exclusions.
   [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification) [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification)
3. Load merged global and project settings, then validate referenced agents,
   models, MCP servers, skills, and snapshot settings used by the state
   machine. For each execution that uses static agent and mode selection,
   validation resolves the effective pair and rejects a selected mode that the
   selected agent's non-empty `modes` map does not declare. This check follows
   the same precedence and selector-bypass rules as execution, so it does not
   reject a shadowed settings fallback or speculate about a mode when no agent
   is effective. [§FS-rhei-agents.1.4.1](rhei-agents.spec.md#141-mode-resolution-order) [§FS-rhei-snapshots](rhei-snapshots.spec.md#fs-rhei-snapshots-rhei-session-snapshots-specification)
4. Validate snapshot plan context and report orphaned snapshot diagnostics as
   warnings when a snapshot cache exists. [§FS-rhei-snapshot-operations](rhei-snapshot-operations.spec.md#fs-rhei-snapshot-operations-rhei-snapshot-operations-specification)
5. Report every ticket that reached a successful terminal state while one of
   its `**Prior:**` dependencies is still unsatisfied as a **warning** naming
   the ticket and each blocking prior with its state. Such a plan contradicts
   the dependency semantics it declares, and no other surface reveals it: a
   terminal ticket drops out of `rhei list --blocked` and out of readiness
   entirely, so the plan reads as healthy. The condition is reachable through
   the deliberate `rhei transition` escape hatch
   ([§FS-rhei-transition-cmd.3](rhei-transition-cmd.spec.md#3-behavior)), by editing a `**Prior:**` onto an
   already-completed ticket, and by a prior that was later cancelled — all
   legitimate authoring moves. It is therefore a warning, never an error:
   validation must surface the inconsistency without making an existing plan
   unloadable.
6. When at least one task in the loaded graph declares `**Consumes:**`, add
   exactly one graph-level warning, regardless of how many tasks or references
   consume exports:

   ```text
   **Consumes:** declares export data-flow for prompt injection, not filesystem visibility. Workers can read undeclared sibling exports under runtime/exports/. For a blind round, schedule participants concurrently and brief them not to inspect sibling exports; neither measure enforces blindness once an export exists.
   ```

   The warning is advisory: it does not change plan validity, export
   resolution, readiness, or the filesystem a worker can read
   (§FS-rhei-plan-language.3.12).
7. Exit non-zero when any validation error remains. Warnings do not make the
   command fail.

`rhei validate` does not acquire task locks, run callbacks, spawn agents,
spawn programs, create runtime files, or rewrite the plan.

For each `**Excludes:**` entry, validation applies
[§FS-rhei-plan-language.3.13](rhei-plan-language.spec.md#313-task-read-exclusions) and rejects:

- malformed kinds or paths, root escapes, and symlink escapes;
- a task or export reference that does not resolve in the authored graph;
- duplicate logical, canonical-alias, or contained targets;
- an exact, canonical-alias, or ancestor-directory overlap with a consumed
  export, the current task source, the active state-machine source, or a
  required state input or handoff; and
- an excluded task that can enter a named snapshot-inheritance state.

Resolution uses declared exports rather than runtime file existence, so an
unwritten future export is valid. Diagnostics name the task, authored entry,
resolved target when available, and the conflicting declaration. Readable-root
support is not a validation precondition: a profile with no `deny_read`
adapter is valid and receives composition-only enforcement.

### 4.1. Unresolved `**Prior:**` references

A `**Prior:**` that resolves to no ticket is reported under **the id the author
wrote**. A dotted reference whose leading segment names no rhei is kept
unqualified at load precisely so this error can quote the source
([§AR-rhei-panta.3](../architecture/rhei-panta.spec.md#3-identity-and-id-namespacing)); reporting it under a citing-rhei prefix would name an id
that appears in no file and cannot be searched for.

Such a reference is ambiguous — a mistyped rhei name or a mistyped rhei-local
hierarchical id — so the message rules out both readings: it names the missing
rhei with the project's rhei ids, and states that the citing rhei has no ticket
under that id either.

A correction is offered only when it is actionable. The leading segment is
matched against the project's rhei ids within a small edit distance, and the
resulting id is suggested only when it **resolves to an existing ticket other
than the citing task**. A suggestion that does not resolve trades one dead end
for another, and one that names the citing task proposes a self-dependency.
Names shorter than three characters yield no suggestion at all: below that
length every id is within one edit of every other, so a near miss carries no
signal.

A prior under a *known* rhei is an ordinary missing ticket and is reported
without further explanation.

### 4.2. Diagnostic parity across scopes

A parse error must read the same whether the plan was reached directly
(`rhei validate plans/auth.rhei.md`) or through its project (`rhei validate`
inside a Panta project). Both forms report **every** recoverable problem in the
offending file, not just the first, and both render the file path relative to
the invocation directory when that is shorter than the absolute path. Whichever
spelling is selected, the complete path is emitted on one physical diagnostic
line even when it is longer than the renderer's wrap width; `/`, `\`, and `-`
inside a path are not line-break opportunities.

Parity matters most for the errors that cascade. A task heading authored under a
content section rather than `## Tasks` fails first as *"Metadata field appears
outside a task"* on a line the author did not get wrong; only the structural
*"Tasks section must be the final `##` chapter"* diagnostic — which recovery
reaches last — explains the mistake. Reporting one error per file would hide it
behind the symptom, in the invocation form `rhei init` steers new authors toward
([§FS-rhei-init](rhei-init.spec.md#fs-rhei-init-rhei-init)).

The project loader still stops at the first failing rhei entry: a project whose
second rhei also fails reports the first one, and the next run reports the next.
Completeness is promised *within* a file, not across a project.

### 4.3. Task export diagnostics

Validation enforces the declaration relationships of
[§FS-rhei-plan-language.3.12.1](rhei-plan-language.spec.md#3121-declaration-integrity) without reading export files. A consumed producer
must exist, must directly appear in the consumer's `**Prior:**`, and must
declare the named export. Self- and ancestor-consumption receive their own
errors. A missing-export error lists every export that the resolved producer
does declare, including an explicit empty list, so a typo can be repaired from
one diagnostic.

One authored mistake produces one primary error for a consumed reference. A
missing producer already reported through the same unresolved `**Prior:**` is
not reported again through `**Consumes:**`. A missing producer named only by
`**Consumes:**` gets one missing-producer error and no derivative missing-edge
error. Self- or ancestor-consumption gets its specific error without pairing or
direct-edge follow-ons. These suppressions do not hide independent errors on
other references.

Unused `**Provides:**` entries produce no diagnostic. Duplicate provided names
and duplicate consumed references remain parse errors. The successful-terminal
consumer warning in §4 remains the sole coherence warning when a direct
producer is non-terminal; export validation does not add a second warning for
the same ordering contradiction. The graph-level `**Consumes:**` visibility
advisory in §4 remains present independently of export-integrity diagnostics.

## 5. Watch Mode

With `--watch`, the command resolves the same state machine once, prints a
watch-start message, runs an initial validation pass, and then re-runs
validation when the plan file or resolved states file changes.

Watch mode reports each pass independently. Each successful pass reports the
`**Consumes:**` warning once when that pass's graph contains a consumer; file
events do not repeat it outside a validation pass. A failed pass does not
terminate the watcher; file watcher initialization errors do.

## 6. Output

On success:

```text
Validation succeeded
```

Warnings are printed after the success line:

```text
Validation succeeded
warning: <diagnostic>
```

For a graph with `**Consumes:**`, successful output is the success line followed
by the exact advisory from §4 with the normal `warning: ` prefix. A graph
without `**Consumes:**` retains the existing `Validation succeeded\n` output
byte for byte. Existing warnings retain their wording and occur once at their
existing trigger frequency.

On a semantic validation failure, the diagnostic names the resolved
state-machine sources that the validation pass used. When the pass used one
source, the existing sentence is retained byte-for-byte, including for a
compatible explicit override and a pass using only the built-in machine:

```text
I validated this plan using '<project>/states.yaml', but found a problem.
```

When the pass used several sources, the diagnostic lists every source before
the existing batched errors. Each file is grouped by source identity, not by
equal machine contents, and names the rheis governed by it. The project default
comes first; remaining groups are ordered by owning rhei id, and owners sharing
a source are ordered by rhei id. Rheis that inherit the project default are
named on its entry. A checked default with no inheriting rheis is identified as
`project default` without claiming a rhei owner.

```text
I validated this plan using these state-machine sources:
  - '<project>/states.yaml' (project default; rhei: audit)
  - '<project>/billing/states.yaml' (rhei: billing)
but found a problem.
```

A built-in source is called `the built-in default state machine`; the
diagnostic never fabricates a path for it. The source set is the same resolved
set used by validation, so reporting does not repeat or alter resolution.

This source presentation is shared by persistent-source semantic validation
failures reached through `validate`, validate watch mode, `new`, `next`,
`complete`, and `run`. The watch startup banner is separate and unchanged.
`instantiate` retains the single-source presentation for its rendered output,
because that output is removed when validation fails and therefore cannot
serve as a persistent source to inspect.

All other failure content is unchanged: the semantic errors and allowed-state
lists, remedy and help, validation scope, and non-zero exit behavior. Success
and warning output are unchanged, as are `rhei states` text and JSON output. A
conflicting per-rhei declaration under an explicit override remains a
resolution refusal rather than a semantic validation report.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) - parse and semantic constraints
- [States Specification](rhei-states.spec.md) - state machine format and defaults
- [Agents Specification](rhei-agents.spec.md) - settings and agent/model references
- [Snapshots Specification](rhei-snapshots.spec.md) - snapshot runtime model
- [Snapshot Operations Specification](rhei-snapshot-operations.spec.md) - snapshot CLI and orphan diagnostics
