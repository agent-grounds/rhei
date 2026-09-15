# FS-rhei-memory: Mid-Term Memory

A Rhei project is the **mid-term memory** of the work it governs: longer-lived
than any agent session, shorter-lived than the repository's specs and code. It
holds what was decided, what was produced, what failed, and what is still open
— in plan files, result files, exports, briefs, logs, and the transition
ledger. [§GND-rhei-purpose](../grund.md#gnd-rhei-purpose-governed-agent-work) [§FS-rhei-usage.4](rhei-usage.spec.md#4-the-plan-as-shared-memory)

This document specifies how an invocation of a task **reads** that memory. Under
`rhei run` every invocation is cold: the agent knows nothing but its prompt. So
the prompt must *reconstitute* the memory — tell the agent where it stands,
what happened before it, what happened to it, and how to find anything the
prompt left out — and it must do so by a fixed algorithm, at a bounded cost in
tokens. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

This spec extends the prompt of [§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition) with four sections. The
sections are graph- and runtime-level context, not configured in `states.yaml`,
and `rhei next` renders them for a manual worker exactly as `rhei run` does
(§5). It depends on:

- [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification) for the task hierarchy, plan formats, and content sections
- [§FS-rhei-panta](rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis) and [§AR-rhei-panta.5](../architecture/rhei-panta.spec.md#5-execution-root-and-per-rhei-runtime) for rheis, execution roots, and qualified ids
- [§FS-rhei-complete.3](rhei-complete.spec.md#3-result-file) for result files and the transition ledger
- [§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition) for the prompt this spec extends, and [§FS-rhei-agents.8.1](rhei-agents.spec.md#81-log-file-naming) for log paths
- [§FS-rhei-supervision.5](rhei-supervision.spec.md#5-prompt-composition) for the sections a supervisor already gets

## 1. Requirements

### 1.1. Everything Before Is Reachable

From any invocation, the agent can determine — without guessing, and without
help from anything outside the prompt and the files it names — for **every
task in the Panta** that is terminal when the prompt is composed: its qualified
id, its title, its final state, and its result. Tasks in the invocation's own
rhei and every transitive prior are listed in the prompt itself (§3.2); every
other rhei is reachable through the map in §3.4, which names each rhei's
execution root, so no terminal task in the project is unreachable from any
other. The ledger of each execution root ([§FS-rhei-complete.3.1](rhei-complete.spec.md#31-state-transition-ledger)) gives the
order in which they finished.

Reachability is navigation, not permission to read every payload. A task's
resolved `**Excludes:**` policy ([§FS-rhei-plan-language.3.13](rhei-plan-language.spec.md#313-task-read-exclusions)) may remove source bytes, but it never removes a task's
qualified id, title, state, relationships, result location, or the complete
project and execution-root map. An excluded result therefore remains findable
as a location even when its contents are not composed.

### 1.2. Composition Is Algorithmic

The prompt is a **pure function** of: the merged project graph, the `runtime/`
tree of every execution root, the resolved state machines, the resolved
settings, the invocation identity (task, state, visit count, execution
identity), the invocation's resolved exclusions, and the set of invocations of the current `rhei run` still in flight
when the prompt is composed (§4.3.5). Composition performs no summarization,
ranking, or selection beyond the rules written in §4: a summary is a fixed
slice of a file, an order is a stated order, a cap is a stated number, and a
truncation leaves a stated overflow line. The same inputs produce the same
bytes. Nothing that varies per run — a run id, a timestamp, a pid — appears in
the prompt; those travel in the environment ([§FS-rhei-agents.4](rhei-agents.spec.md#4-environment-variables)).

The in-flight set is the one input not derivable from disk: it is what the
scheduler happens to have spawned and not yet reaped. Under `--parallel 1` it
is empty or a single ticket and the prompt is reproducible; under
`--parallel > 1` it depends on scheduling, so `### In Flight` is not
reproducible there and a test must not pin its contents.

### 1.3. Bounded and Progressive

Every section is omitted when it has nothing to say, so a one-task plan under
the built-in machine gains a few lines, not a page. Every section that can
grow has a cap, and every cap is paired with an overflow line naming the
command or file that holds the rest. Detail is pasted once: a result already
pasted in full elsewhere in the prompt is referred to, not repeated. The prompt
gives the agent the map and the nearest detail; the agent fetches the rest.

## 2. The Store

The memory is whatever the project already writes; this spec adds no store.

| Memory | Where | Written by |
|---|---|---|
| Intent, decomposition, progress notes | plan files: task bodies, `**Prior:**`, `**Consumes:**`/`**Provides:**` | plan writer, workers, supervisors |
| Plan-level context | content sections of the rhei (`index.rhei.md`, or the H2 sections before `## Tasks`) and of `index.panta.md` | plan writer |
| Outcome of a task | `runtime/results/<task-id>.md` ([§FS-rhei-complete.3.2](rhei-complete.spec.md#32-result-file-format)) | `rhei complete`, `rhei transition --result`, workers, the engine's failure routes |
| Order of events | `runtime/state-transitions.log` ([§FS-rhei-complete.3.1](rhei-complete.spec.md#31-state-transition-ledger)) | every transition |
| Published outputs | `runtime/exports/<task-id>/<name>.md` ([§FS-rhei-plan-language.3.12](rhei-plan-language.spec.md#312-task-exports)) | the producing task |
| Direction from above | `runtime/supervise/<task-id>[/<state>].md` ([§FS-rhei-supervision.5.2](rhei-supervision.spec.md#52-the-brief)) | supervisors |
| Same-task state handoffs | declared `outputs:` of an earlier state ([§FS-rhei-states.3.2](rhei-states.spec.md#32-state-handoffs)) | the earlier state |
| Raw transcripts | `runtime/logs/task-…log` under the root `rhei run` was started from ([§FS-rhei-agents.8](rhei-agents.spec.md#8-log-capture)) | `rhei run` |

Each path is relative to the **execution root** of the rhei that owns the
task ([§AR-rhei-panta.5](../architecture/rhei-panta.spec.md#5-execution-root-and-per-rhei-runtime)): the workspace directory of a Directory Workspace rhei,
the project directory for single-file rheis. Transcripts are the exception:
one `rhei run` writes one log tree, under the root it was started from, so in a
Panta the transcripts of every member rhei sit together at the project root and
not under the member. A prompt therefore names that directory outright (§3.4)
rather than describing it as a path under something else.

## 3. The Sections

The four sections below join the prompt of [§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition) in this order:
`## Position` directly after `## State:` and the personality, before
`## Instructions`; `## Plan History` and `## Previous Visits` after
`## Supervisor Brief`; and two sub-sections inside `## Rhei Commands`.
Orientation comes before the instructions so the instructions are read with
the goal in mind; the broader memory comes after the task's own inputs because
the inputs are what the task acts on and the history is what it acts *within*.
Every source payload named below is filtered before its file bytes are read;
the navigation-only fields remain as §1.1 requires.

### 3.1. `## Position`

Where this invocation sits in the project, top down. The complete shape below
is for an ordinary task; a supervising task stops before the two context
blocks, as specified after the example.

```
## Position

Panta: {panta-title} › rhei `{rhei-id}`: {rhei-title} › {Kind} {ancestor-id}: {title} [{state}] › …
› **{Kind} {task_id}: {title} [{state}]** ← this invocation (visit {visit_count})

### Siblings

- {Kind} {id}: {title} [{state}]
- {Kind} {id}: {title} [{state}] — waits on this task

### Parent: {Kind} {parent-id}: {title}

    ```markdown
    {the parent's body}
    ```

### Rhei Context

    ```markdown
    {content sections of the owning rhei, verbatim}
    ```

### Project Context

    ```markdown
    {content sections of index.panta.md, verbatim}
    ```
```

- The chain line names every ancestor, root first, each with its state. A
  root task's chain is the Panta and the rhei alone.
- `{Kind}` is the node's own kind, title-cased — `Task`, `Bug`, whatever the
  plan's `structure.nodeKinds` declares ([§FS-rhei-plan-language.3.7](rhei-plan-language.spec.md#37-node-kind-validity)) — the form
  `## Child Tasks` and `rhei list` already print. `{state}` is the machine's
  name for the state, with the `-<visit>` suffix a counted loop writes into
  `**State:**` dropped ([§FS-rhei-plan-language.3.2](rhei-plan-language.spec.md#32-state-validity)), so every state name in the
  prompt has one form.
- `### Siblings` renders only when the task has a parent: the parent's other
  children, in plan order. A sibling that lists this task in `**Prior:**` or
  consumes one of its exports carries ` — waits on this task`. A root task has
  no sibling list; `## Plan History` and `### In Flight` cover the rest of the
  rhei.
- `### Parent` pastes the nearest ancestor's body in full, fenced, unless that
  task source is excluded; in that case the heading identifies the parent and
  names its source location without reading or rendering the body. Higher
  ancestors contribute one line each to the chain and nothing more — a
  four-level tree does not paste four bodies. The parent's body is the memory
  that matters most to a leaf: it is where the decomposition was decided and
  where the acceptance for the whole subtree is written.
- A task is **supervising** for prompt composition exactly when its current
  normalized state declares `execute_on` ([§FS-rhei-supervision.1.1](rhei-supervision.spec.md#11-the-execute_on-field)). The role follows the
  state declaration, not the task's depth, whether it has children, or the
  event that selected the visit.
- For an ordinary, non-supervising task, `### Rhei Context` and `### Project
  Context` paste the content sections of the owning rhei and of the Panta
  manifest, verbatim and in authored order. These are the plan writer's
  standing notes, and until now only a worker that opened the file read them.
  A bare rhei with no Panta manifest has no `### Project Context`. Excluded
  source files contribute no payload; required current task and machine
  sources cannot reach composition because validation rejects that overlap.
- For a supervising task, omit both context headings and their bodies. The
  rest of `## Position` is unchanged, and the prompt still carries the task's
  own content, child map, checkpoints, supervisor brief and declared inputs,
  previous-visit memory, and other applicable sections defined by
  [§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition), [§FS-rhei-supervision.5](rhei-supervision.spec.md#5-prompt-composition), and this spec. `### Reading the rhei` (§3.4) names
  the owning rhei document, project root, and read-only navigation commands, so
  the full standing context remains directly reachable without its bytes being
  copied into every supervisory handoff.

### 3.2. `## Plan History`

What finished before this invocation, one line per task.

```
## Plan History

Finished work, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root.

- {Kind} {id}: {title} — {state} — {summary}
- {Kind} {id}: {title} — {state} — see above
- {Kind} {rhei}.{id}: {title} — {state} — {summary}   (rhei `{rhei}`, prior)
… {n} earlier tasks not shown — `rhei list --rhei {rhei-id} --terminal`

### In Flight

- {Kind} {id}: {title} [{state}] — {assignee}
- {Kind} {id}: {title} [{state}] — this run

### Dependents

- {Kind} {id}: {title} [{state}] — prior
- {Kind} {id}: {title} [{state}] — consumes `{export}`
```

- The list covers every terminal task of the **owning rhei** and every
  **transitive prior** of this task in any rhei, including cancelled tasks —
  why something was not done is memory too. Tasks outside the owning rhei
  carry a trailing `(rhei `<id>`, prior)`.
- `{summary}` is derived by the rule in §4.3, never written by a model. An
  excluded result is not opened and renders `payload excluded` while keeping
  the task, state, and result location visible. A task
  whose result is already pasted in full in this prompt — under `## Prior Task
  Results`, `## Child Task Results`, or `## Checkpoints` — shows `see above`
  instead.
- `### In Flight` lists every non-terminal task in the Panta, other than this
  one, that carries an `**Assignee:**` or is spawned by the same `rhei run`
  pass: what other agents are touching right now. Omitted when there is none.
- `### Dependents` lists every task in the Panta that names this task in
  `**Prior:**` or names one of its exports in `**Consumes:**`, with the
  relation. This is who reads what this task writes; omitted when there is
  none.

### 3.3. `## Previous Visits`

What already happened to this task — rendered only when the task has at least
one ledger line or a result file, i.e. never on a task's first invocation.

```
## Previous Visits

Trail for this task: {from} → {to} → … → {state} (this visit, visit {visit_count}).

Result entries so far:

    ```markdown
    {runtime/results/<task-id>.md}
    ```

Previous log: `runtime/logs/{log file of the previous visit of this state}`

Retrying this visit: attempt {n}. The previous attempt {ending}. It did not
write `{result path}`, which a transition out of this state reads to finish
this task. Its transcript is `{previous attempt log}`.
```

- The trail is the state sequence of this task's ledger lines, in order — the
  first line's `from`, then every line's `to` — with the state being entered
  marked as this visit (§4.4.1). The engine has already written the line that
  moved the task here, so that state is annotated in place rather than
  repeated.
- The result file is pasted because it is where every earlier verdict on this
  task landed: a worker's `--result` message, and the engine's own entries when
  an earlier visit timed out or exited without its required outputs
  ([§FS-rhei-agents.3.2.1](rhei-agents.spec.md#321-runtime-semantics)). An agent retrying a state that stalled must know
  why it stalled. When the result is excluded, its path and the state trail
  remain but its contents are not read or pasted.
- `Previous log:` names the log file of the previous visit of this same state
  by the naming rule of [§FS-rhei-agents.8.1](rhei-agents.spec.md#81-log-file-naming), only if that file exists. The
  log is not pasted; it is a transcript, and the path is enough.
- The retry paragraph is rendered only when *this visit* has already been
  spawned — when a spawn record for this invocation belongs to this visit
  ([§FS-rhei-agents.8.4](rhei-agents.spec.md#84-spawn-records)). It exists because a re-spawn that is handed the same
  prompt as the attempt it is recovering from will do the same thing again: the
  invocation has to be told that it is a retry, what ended the last attempt, and
  which file that attempt was obliged to write and did not. The result path is
  named because naming it is the whole point — the built-in prompt already shows
  it as where a finished task's result is *read from*, which agents read as
  description rather than obligation.

### 3.4. `## Rhei Commands` Additions

Two fixed sub-sections follow the existing authority text and transition list.

```
### Reading the rhei

- This rhei: `{execution root}` — plan `{plan or index path}`, this task's file `{task file path}`
- Every rhei in this project and its execution root:
  - `{rhei-id}` — `{execution root}`
- Under each execution root: `runtime/results/<task-id>.md` (results),
  `runtime/exports/<task-id>/<name>.md` (exports), `runtime/supervise/<task-id>[/<state>].md` (briefs),
  `runtime/state-transitions.log` (order of events)
- Agent transcripts: `{logs directory of this run}`
- Read-only commands, always safe: `rhei list [--rhei <id>] [--terminal] [--has-prior <id>] [--parent <id>]`,
  `rhei render <plan> --format json --pretty`

### Leaving a trail

What you write is what the next agent and the human see.
- `runtime/results/<task-id>.md`: the first line is the one-line summary every later Plan History shows; detail below it.
- You may append progress paragraphs to your own task body — files touched, commands run, decisions made — and append child tasks under your own task. Do not edit `**State:**` lines or any other task's body.
```

`Reading the rhei` is the map that makes §1.1 true across rheis: it names
every execution root in the project, so the results of a rhei the prompt does
not list are one path away. A rhei whose execution root holds no plan document
— the synthetic `basin`, whose manifest is never authored ([§FS-rhei-panta.4](rhei-panta.spec.md#4-invisibility)) —
omits the `— plan …` clause and names this task's file alone. `Agent transcripts`
names the resolved `runtime/logs/` directory of this run — the one `Previous
log:` resolves against (§3.3) — because that tree belongs to the run and not to
a rhei (§2). Paths are rendered relative to the execution root named in the
prompt, or absolute when `RHEI_CHECKOUT_ROOT` differs from it, by the same rule
`{output.<name>.path}` follows ([§FS-rhei-states.4](rhei-states.spec.md#4-template-variables-in-instructions-and-personality)). `rhei next`, which exports
no checkout-root context, renders every such path absolute. `Leaving a trail`
describes artifacts and permitted edits; it says nothing about when to stop or
how completion is detected, which stay with the completion condition
([§FS-rhei-agents.3.1](rhei-agents.spec.md#31-completion-authority)).
The map is never filtered by exclusions. A listed path may therefore identify
an excluded source; `## Exclusions` in the agent prompt states the applicable
guarantee and prevents the map from being mistaken for permission.

## 4. Composition Algorithm

Given an invocation `I = (task, state, visit_count, identity)`:

### 4.1. Inputs

1. `G` — the merged project graph in plan order ([§FS-rhei-plan-language.1.2](rhei-plan-language.spec.md#12-directory-workspace-agent-teams-high-concurrency));
   for a bare rhei, its implicit Panta ([§FS-rhei-panta](rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis)).
2. For every rhei `R` in `G`: its execution root `root(R)` and, under it, the
   ledger `L(R)` and the `runtime/` tree.
3. The owning rhei `R₀ = rhei(task)`.
4. The `runtime/` directory of the invoking run — the one it writes `logs/`
   under ([§FS-rhei-agents.8](rhei-agents.spec.md#8-log-capture)), which is the root the run was started from and
   need not be `root(R₀)`.
5. `X` — the task's exclusions resolved against its effective checkout, every
   owning-rhei execution root, and the authored project graph immediately
   before composition ([§FS-rhei-plan-language.3.13](rhei-plan-language.spec.md#313-task-read-exclusions)).

Before opening any candidate payload in §4.2–§4.4 or the prompt sections owned
by [§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition), compare its logical and canonical source to `X`. Omit an exact file match or a descendant of an
excluded directory. This check precedes reading, summarizing, truncating,
fencing, or rendering. It covers exports, prior/child/current results, history,
parent and project context, checkpoints, briefs, state handoffs, and previous
visits. Required invocation inputs have already been rejected on overlap; they
are never silently omitted here.

### 4.2. Position

1. `chain` = ancestors of `task` from the root down; render each as
   `<Kind> <id>: <title> [<state>]`, joined by ` › `, prefixed by the Panta
   title and `rhei <R₀>: <title>`.
2. If `task` has a parent `P`: `siblings` = children of `P` in plan order
   minus `task`; cap 30, overflow line
   `… {n} more — rhei list --parent <P>`. Mark a sibling `— waits on this task`
   when `task ∈ Prior(sibling)` or `Consumes(sibling)` names an export of
   `task`. Paste `body(P)`, fenced (§4.5); cap 200 lines, overflow line
   `… truncated; read <task file path of P>`.
3. If the normalized `state(task)` declares `execute_on`, render neither
   `### Rhei Context` nor `### Project Context`. Otherwise, `### Rhei Context`
   = the content sections of `R₀`'s index (or the H2 sections before `## Tasks`
   of its single-file plan), verbatim, in authored order; `### Project Context`
   = the content sections of `index.panta.md`. Each rendered context is capped
   at 1000 lines with the overflow line `… truncated; read <path>`. Omit either
   when empty.

### 4.3. Plan History

1. `own` = terminal tasks of `R₀` other than `task` and its descendants
   already rendered under `## Child Task Results` or `## Checkpoints`.
   Order: by the position in `L(R₀)` of each task's last line entering a
   terminal state; tasks with no ledger line (imported plans) come first, in
   plan order.
2. `priors` = the transitive closure of `Prior(task)` minus `own`, in plan
   order, each tagged `(rhei <R>, prior)`.
3. When `T`'s result source is excluded, do not open it and render `payload
   excluded`; otherwise `summary(T)` = the first non-blank line of the **last** `## Result` entry
   of `runtime/results/<T>.md` under `root(rhei(T))`, excluding the heading
   line, cut to the first 120 characters followed by `…` — characters, not
   display columns; `(no result)` when the file is missing or empty. An entry
   opens with a plain `## Result` heading or with the `## Result — <identity>`
   a fanned-out fold writes ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)); the last of either kind is
   the standing verdict. A heading inside a fenced code block is **not** an
   entry: a file that quotes a verdict is showing one, not casting it
   ([§FS-rhei-plan-language.3.6](rhei-plan-language.spec.md#36-link-integrity)). If `T`'s result is pasted in full elsewhere
   in this prompt, `see above` replaces the summary. When that file does not
   exist and `T`'s body carries a `> **Result:**` block, the file **that block
   links**, resolved against `root(rhei(T))`, is read instead
   ([§FS-rhei-plan-language.3.8](rhei-plan-language.spec.md#38-result-block-consistency)) — a plan finished before ids were qualified
   keeps its account under the rhei-local name, and `(no result)` printed
   under a body that shows the link is a false statement. Nothing else is
   consulted.
4. Cap: 40 lines. Entries in `priors` are never dropped; entries in `own` are
   dropped **oldest first** until the cap holds, and the overflow line
   `… {n} earlier tasks not shown — rhei list --rhei <R₀> --terminal` is
   emitted once, first.
5. `### In Flight` = non-terminal tasks of `G` other than `task` with an
   `**Assignee:**`, or spawned by the current `rhei run` pass; plan order;
   cap 20, overflow `… {n} more — rhei list --non-terminal`. The trailing
   column is the `**Assignee:**` value, or the literal `this run` for a ticket
   this pass spawned: `rhei run` claims by spawning and writes no assignee, so
   the pass's own set is the only witness for its workers.
6. `### Dependents` = tasks of `G` with `task ∈ Prior(·)` or consuming an
   export of `task`; plan order; each with `— prior` and/or
   `— consumes <export>`; cap 30, overflow
   `… {n} more — rhei list --has-prior <task>`.
7. Omit `## Plan History` entirely when `own ∪ priors` is empty and neither
   sub-section renders. The preamble introduces the list, so it is emitted only
   when the list has at least one entry: with nothing finished but somebody
   working or waiting, `### In Flight` and `### Dependents` render under a bare
   `## Plan History`.

### 4.4. Previous Visits

1. `trail` = the state sequence of `L(R₀)`'s lines for `task`: the first
   line's `from`, then each line's `to`. Render when `trail` is non-empty or
   `runtime/results/<task>.md` exists; otherwise omit the section. If the last
   state of `trail` equals `state`, annotate that last state with
   ` (this visit, visit <visit_count>)`; otherwise append
   ` → <state> (this visit, visit <visit_count>)`. A self-loop leaves the state
   in `trail` twice and both stay: the second is the previous visit, not this
   one.
   The ledger retains state-only navigation and is not filtered.
2. If the current result source is excluded, keep its path but do not open it;
   otherwise paste `runtime/results/<task>.md`, fenced; cap 100 lines, keeping the
   **last** 100 with the overflow line `… earlier entries omitted; read <path>`
   first. The legacy fallback of §4.3.3 applies here too, and `<path>` names
   whichever file was read.
3. `prev_log` = the log named by the spawn record ([§FS-rhei-agents.8.4](rhei-agents.spec.md#84-spawn-records)) of
   `(task, state, identity, visit_count − 1)` — the last thing that actually
   ran there, whichever attempt of that visit it was — falling back to that
   visit's unsuffixed log file where no record answers, which is what a runtime
   written before records existed has. Emit the `Previous log:` line only when
   the named file is on disk.
4. `retry` = the spawn record of `(task, state, identity, visit_count)`, when
   one exists **and** it belongs to this visit: its `moves` equals the number of
   moves the ticket has made, i.e. the ticket has not left the state since that
   spawn. Render the retry paragraph from it — `attempt` + 1 as `{n}`, its
   `ending` and `code` as `{ending}`, its `log` as `{previous attempt log}` —
   and name the result path this invocation is handed as `{result path}`,
   omitting that clause on a state no terminal edge leaves. A record from an
   earlier visit is not a retry and renders nothing: re-entering a state is a
   fresh start, not a second attempt.

### 4.5. Fencing and Rendering

1. Every pasted body is fenced with a backtick run one longer than the longest
   run it contains, so a pasted `## Result` never becomes a prompt heading
   ([§FS-rhei-supervision.5.1](rhei-supervision.spec.md#51-the-supervisors-prompt)). `## Prior Task Results`, `## Consumed Exports`,
   and the handoff sections adopt the same fence.
2. Ids are **qualified** everywhere ([§FS-rhei-panta.6.3](rhei-panta.spec.md#63-completion-and-rollup)) — the form
   `rhei list` prints and `rhei transition` accepts.
3. Truncation is by whole lines, never mid-line, and the overflow line is a
   literal of this spec so tests can match it.
4. Caps are the numbers in this section. A settings surface may later override
   a cap with another number; it cannot introduce selection by relevance, and
   a cap of `0` removes the section rather than the overflow line.

## 5. Surfaces

- `rhei run` composes the sections for every spawned agent, in every mode. The
  shared `## Position` renderer applies the role rule of §3.1 and §4.2.
- `rhei next` text output renders the same sections in the same order after
  the instructions, as it does for the supervision sections today
  ([§FS-rhei-supervision.3.4](rhei-supervision.spec.md#34-manual-workers)); JSON output carries each as a string field named
  after the section: `position`, `plan_history`, `previous_visits`,
  `navigation`. It uses that same role-aware `## Position` renderer, so a
  supervising task omits inline rhei and project context on both surfaces and
  an ordinary task keeps it on both. Two differences follow from what that
  surface prints:
  - `rhei next` renders no `## Rhei Commands`, so the two sub-sections of §3.4
    would arrive with no `##` parent. On this surface they are wrapped in
    `## Rhei Navigation`; the JSON field stays `navigation`.
  - `rhei next` pastes neither `## Prior Task Results` nor `## Child Task
    Results`, so `see above` would point at nothing: a task whose result is
    pasted only under those sections shows its real summary here instead.
    `## Checkpoints` is rendered on both surfaces and counts on both.
- `rhei run --dry-run` keeps its `<prompt...>` placeholder ([§FS-rhei-agents.9](rhei-agents.spec.md#9-dry-run-output)).
  There is no command yet that prints the full composed prompt without
  claiming a ticket; claim-mode `rhei next` output ([§FS-rhei-next.3.2](rhei-next.spec.md#32-output-claim-mode)) is the
  way to see it today, and a dry-run form is tracked on the roadmap.

## 6. Example

In the supervised-delivery example, the second invocation of
`deliver.fix-1` in state `fix`, after round-1 reviews finished and the first
`fix` visit timed out:

```
## Position

Panta: Rhei › rhei `delivery`: Supervised delivery › Task delivery.deliver: Deliver subtree supervision [supervising]
› **Task delivery.deliver.fix-1: Fix round 1 [fix]** ← this invocation (visit 2)

### Siblings

- Task delivery.deliver.implement: Implement docs/functional-spec/rhei-supervision.spec.md [completed]
- Task delivery.deliver.review-1: Code review round 1 [completed]
- Task delivery.deliver.pm-1: Product review round 1 [completed]
- Task delivery.deliver.review-2: Code review round 2 [review] — waits on this task
…

## Plan History

Finished work, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root.

- Task delivery.deliver.implement: Implement … — completed — Landed execute_on, hold/release, checkpoints; 41 tests
- Task delivery.deliver.review-1: Code review round 1 — completed — see above
- Task delivery.deliver.pm-1: Product review round 1 — completed — see above

### Dependents

- Task delivery.deliver.review-2: Code review round 2 [review] — prior, consumes `resolutions`
- Task delivery.deliver.pm-2: Product review round 2 [pm-review] — prior, consumes `resolutions`

## Previous Visits

Trail for this task: pending → fix → fix (this visit, visit 2).

Result entries so far:

    ```markdown
    ## Result

    agent timed out in state 'fix' after 30m
    ```

Previous log: `runtime/logs/task-delivery.deliver.fix-1-fix.log`
```

The two reviews are `see above` because they are this task's direct priors and
already pasted in full under `## Prior Task Results`; the agent pays for each
result once.
