# FS-rhei-viz: Flow Visualization

The **Flow view** is Rhei's primary visualization surface: one page that answers
the operator's two standing questions — *what is my Rhei doing, and why* — for a
plan or a Directory Workspace of plans. It pairs a navigable view of plan shape
with a surroundings inspector for any node and the resolved state machine the
plan runs under, and it leads with whatever is working right now.

The same renderer serves two modes from one model: a **dynamic** surface that
updates live during `rhei run`, and a **static** artifact that can be frozen at
the end of a run or generated offline from a plan. Both use the same asset and
renderer, so they look and behave identically; their output differs only by the
runtime overlay (§8). A page frozen at the end of a run inlines the final live
snapshot, so it equals the live page except that it stops updating; a page
generated offline from a plan has no runtime overlay, so it omits live-only
content (running slots, accounting, the real agent transcript).

The visual language — monospace typography, calm desaturated color, near-zero
motion, the console-first feel shared with the terminal — is governed by
[§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux). This spec defines *which* views exist and *what* data they show.
For the plan grammar see the [Plan Language Specification](rhei-plan-language.spec.md);
for the state machine see the [States Specification](rhei-states.spec.md).

The surface ships as the `rhei viz` command (§7.2). The `xtask` static renderer
(`cargo xtask examples viz <name>`) dogfoods it against the canonical design
fixtures `examples/inflight-dashboard` and `examples/disjoint-tracks`.

## Goals

1. **One page, the whole plan.** The resting view shows the plan, every task, and
   every subtask, each marked with its current state — done, active, live,
   blocked,
   gated, failed, retired, or idle — without reading raw markdown and without a
   click.
2. **"Why," in the fewest clicks.** Any node — not only the running ones — can be
   entered to see its surroundings: what it depends on, what it unblocks, where it
   sits in its state machine and where it can go next, the prompt it runs, and its
   artifacts. Walking from a node to a neighbor and back costs one keystroke
   [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals).
3. **Show the machine, not just the plan.** The resolved state machine is drawn as
   a graph — one per disjoint workflow — so the shape of the process is legible
   alongside the shape of the work.
4. **Lead with live work.** The view surfaces tasks that are running now and
   exposes each live task's streaming agent output. A live task whose agent keeps
   an interactive stdin open also offers a way to intervene (§5); intervention is
   opt-in per agent, so the composer appears only where messaging can succeed.
5. **Dynamic and static parity.** The live surface and a static page render from
   one model with one asset, one layout, and one behavior; their output differs
   only by the runtime overlay — liveness, running slots, accounting, and the
   real agent transcript (§8).
6. **Self-contained and calm.** The page carries its CSS and JavaScript inline,
   with no external scripts, stylesheets, fonts, or network assets, and it obeys
   the console-first language of [§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux).

## Non-Goals

- No general plan editing, mutation, transitioning, or diff mode from the
  surface. It is read-only with respect to plan state except for the narrow live
  human-gate transition action in §5.1: open-in-editor spawns the operator's
  editor (§11) and intervene messages a running agent (§5), but neither writes or
  advances the plan.
- No remote deployment. The live surface is loopback-only; the static surface is
  a local file.
- No redefinition of the look-and-feel; that is [§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux).
- The dense chart overviews (Gantt, Cube, Sankey) are supplementary scanning aids
  (§12), not the primary surface.

## 1. The Flow Surface

The page is a single screen with five regions, top to bottom:

- **Header** — the plan title, a plan/workspace selector when more than one plan
  is present, and a one-line state-machine summary.
- **Strip** — a List/Graph mode toggle, summary counts, and the glyph legend.
- **Running-now panel** — chips for tasks currently assigned to a runtime slot;
  hidden when no top-level task is running (§5).
- **Flow** — two panes. The left pane is the plan in **List** mode (§2) or
  **Graph** mode (§3); the right pane is the **Surroundings** inspector for the
  selected node (§4). In list mode the outline is a fixed column and surroundings
  grows to the right edge; in graph mode the DAG fills the left and surroundings
  is a fixed column. Below ~1060px the panes stack.
- **Machines** — the resolved state machine drawn as one graph per disjoint
  workflow, with a clickable legend and a state-detail panel (§6).

The surface is keyboard-first and mouse-optional, per [§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux) §7:
`j`/`k` or arrows move the selection through the plan in source order, `g` and
`l` switch to Graph and List mode, and the selection drives both the surroundings
inspector and the machine-detail panel together. All content is selectable and
copy-friendly.

The selected node is reflected in the URL hash, so a page can be **deep-linked**
to a specific node: opening the surface with `#<node-id>` selects that node and
opens its surroundings instead of the default live-leading view, and the
inspector head offers a "copy link" affordance that yields such a URL. This makes
the static artifact addressable for async review ("see task auth.3.1 here"). Hash
updates use history replacement so keyboard navigation does not flood browser
history; editing the hash or using back/forward reselects.

### 1.1. State Category and Glyph

Every persisted state is reduced to one of seven **categories**, derived from
the resolved machine's flags first and the state name second, so that custom
vocabularies classify correctly. The rows are evaluated top to bottom and the
first match wins, so the `active` catch-all only claims a state no earlier row
matched:

| Category | Glyph | Meaning | Derivation |
| --- | --- | --- | --- |
| `done` | `✓` | completed | state `completed` |
| `blocked` | `⊘` | attention | state `blocked` |
| `failed` | `✗` | attention | state `failed` |
| `gate` | `⏸` | awaiting a person | machine `gating`, a poll declaring `waiting_on` ([§FS-rhei-states.2.5](rhei-states.spec.md#25-waiting-on-a-person)), or `human-review` |
| `retired` | `⊝` | terminal, not done | `cancelled`, `archived`, other terminal |
| `idle` | `·` | not started | `draft`, `pending`, or any profile's `initial` |
| `active` | `●` | active state, not necessarily running | any non-terminal state not matched above |

A person-waiting poll shares the `gate` row rather than earning an eighth
category because it is the same thing to the eye: work that is somebody's turn
and nobody's to hurry. It differs from a gate only in who moves it on — the
poll resumes itself when the answer lands — and that is a scheduling fact, not
a status the operator reads off a glyph. A poll with no `waiting_on` is
unaffected and stays `active`.

Category, not raw state, drives the calm whole-line coloring of list rows and the
fill of graph nodes, so the eye reads status at a glance. During a dynamic run,
`live` is a runtime overlay category for tasks assigned to a running slot; it is
the single place motion is allowed: a spinner on list rows and marching-ants on
graph nodes, both stilled to a static dot under `prefers-reduced-motion`, per
[§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux) §4. State pills always carry the exact state text and its shared
calm color from [§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux) §3.2; color is never the only signal.

During a dynamic run, `task_runtime[id].in_slot` is an overlay on top of the
state-derived category: any task assigned to a live slot is shown as `live`, even
if its persisted implementation state is otherwise `idle` such as `pending`.
For an active agent slot, `task_runtime[id].template_context` carries the
invocation's concrete target/model/agent values so prompt and artifact previews
match the running process.

### 1.2. Summary and Legend

The strip shows total top-level tasks and category counts for active, blocked,
gate, done, and failed. When a runtime overlay is present, it also shows how many
slots are currently active — the count of live work, including nested subtasks
that occupy a slot — so the running indicator never reads zero while work is
visibly underway. Counts and legend update in place on refresh (§7).

## 2. List Mode (Outline)

The left pane renders the plan as an indented outline: each top-level task
followed by its descendants, indented by id depth. Each row carries the
category glyph (or live spinner), the node id, the title, a left-aligned state
pill, and — for nodes with children — a subtree progress count (`done/total ✓`).
Clicking a row, or moving the keyboard selection onto it, selects the node and
populates surroundings. List mode is the default; it is the calmest overview and
the resting state of the surface.

Outline rows remain contained inside the pane. Long ids and titles truncate
inside the row rather than pushing state pills, progress counts, or adjacent
panes out of place, and long task labels are not exposed through native browser
tooltips that can cover neighboring panes.

## 3. Graph Mode (Dependency DAG)

The left pane renders the **prerequisite** graph over top-level tasks: nodes are
laid out in dependency layers by longest path over `**Prior:**` edges among
top-level tasks, with curved edges from each prior to its dependent. Node fill
follows the state category (§1.1); live nodes animate, stilled under reduced
motion. Clicking a node selects it. Graph mode answers "what unblocks what" at the
plan level; per-node prerequisite detail across all levels lives in the
surroundings inspector (§4).

## 4. Surroundings Inspector

The right pane is the inspector for the selected node — the heart of "why." For
the selected task or subtask it shows, top to bottom:

- **Head** — glyph, id, title, the state pill, and flags (`initial`/`terminal`/
  `gating` from the machine, `waiting on <label>` when the state is a poll that
  declares one (§FS-rhei-states.2.5), plus `root task` or `depth N`), followed
  by the state's description. The label is shown, not just carried: the `⏸`
  glyph of §1.1 says the node is somebody's turn but not whose, and that is the
  one thing the operator needs to act on it.
- **Last state** — always shown as its own section for the selected node. The
  collapsed row is labeled `last state` and shows the state immediately before
  the current persisted state. Expanding the row shows the ordered earlier
  states derived from the command-written central state-transition ledger at
  `runtime/state-transitions.log`. Ordinary movement rows are deterministic and
  timestamp-free: `<task-id> <from>@<to>`. A validated adjacent
  `!force-v1`/movement pair contributes that movement once and makes its
  exceptional reason available to the inspector; malformed pairs are reported
  rather than rendered as history (§FS-rhei-complete.3.1). New commands write
  state history to that central ledger. The live and static renderers may combine the central
  ledger with `runtime/transitions.log` to repair older or partially written run
  histories; legacy per-task result headings are used only when no central
  history exists for that task. If no last state or earlier state is recorded,
  the expanded row says so instead of hiding the section.
- **Dependencies** — two columns: **depends on (Prior)** with each prerequisite as
  a chip marked satisfied when terminal, and **unblocks** with the nodes waiting
  on this one. Unresolved external priors render as flat chips. A "waiting on"
  line names any prior not yet terminal.
- **Came from** — the incoming transitions: which states this state can be entered
  from, with guard conditions, plus a `(from any · *)` marker when a wildcard rule
  applies; an initial state says so.
- **Next state** — the outgoing transitions: the states this one may move to, with
  guard conditions and a `(from *)` marker on wildcard-derived edges.
- **Prompt** — the state's effective agent instructions **instantiated for this
  node**: selected prompt-template instructions first, inline state
  instructions after that, scalar template variables (`{task_id}`,
  `{task_title}`, `{visit_count}`, `{visits}`, `{model}`, …) resolved inline and
  highlighted, and `{input/output.<name>.path}` rendered as artifact links.
  Shown only when the state declares prompt-template or inline instructions.
- **Intervene / output** — for nodes assigned to a runtime slot, the streaming
  agent terminal and capability-gated message composer; for offline active-state
  previews, a representative transcript with delivery disabled (§5).
- **Children** — descendants with their glyphs, ids, titles, and states, and a
  `done/total ✓` header; shown only for nodes with children.
- **Artifacts** — the state's input (`in ◂`) and output (`out ▸`) contracts as
  links, with `{task_id}` and visit-count templates resolved for this node and
  optional artifacts marked. A state that declares no artifacts of its own
  borrows the previous state's outputs (the state before the current one in the
  last-state ledger), labeled with the state they come from: a node parked in a
  declaration-less state — typically a gating `needs-human` — still surfaces
  the report whose transition parked it, so the inspector answers "why is this
  waiting" without a trip to the filesystem.

The inspector must remain contained inside its pane at every supported
breakpoint. Long child ids and titles preserve the state column by truncating
within the row, while artifact paths wrap inside the pane instead of creating
horizontal overflow.

Every chip in came-from, next-state, and the machine legend is clickable: a
transition chip highlights the target state across the machine graphs (§6) while
keeping the task in context; a dependency chip selects that neighbor. Clicking
across the inspector is how an operator walks the plan and the machine without
losing their place. Content swaps obey the single ≤150ms opacity fade of
[§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux) §4.

## 5. Running Execution View

The view leads with work that is actually running. The **running-now panel**
shows one chip per top-level task currently assigned to a runtime slot — id,
short title, state pill, spinner — and is hidden entirely when no top-level task
has `task_runtime[id].in_slot`. On load the surface auto-selects the first
running node; if there is no runtime slot, it falls back to the first
state-derived `active` node. Selecting a chip selects its node.

When the selected node is currently assigned to a runtime slot, the surroundings
inspector adds an **intervene** block: the agent's live output rendered as a real
terminal — dark in any theme, scrollback, ANSI color — that appends new lines in
place from the run's `AgentOutput` stream [§FS-rhei-run-tui.1.2](rhei-run-tui.spec.md#12-live-agent-traffic). The block also
surfaces the slot's latest invocation cost, input/output and cached token counts,
accounting coverage, and elapsed time [§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting). In a static
offline render, a state-derived `active` node may show a representative
transcript, but it is not counted as running.

**Messaging is capability-gated.** The message composer is shown only when the
running agent keeps an interactive stdin open and is therefore reachable — the
per-agent `intervene_stdin` opt-in, surfaced per slot in the snapshot as
`task_runtime[id].intervene` (§8). When a live agent is *not* reachable — a
one-shot or EOF-driven transport — the block states plainly that the agent can't
be messaged live and names the remedy (set `intervene_stdin` on the agent's
profile and rerun), rather than dead-ending the operator or inviting input that
would fail after they type. Intervene messages a running agent; it never
transitions or edits the plan [§AR-rhei-viz-flow.7](../architecture/rhei-viz-flow.spec.md#7-intervene-the-single-mutation-boundary).

The same channel is reachable from the terminal, for operators who run without a
browser open: `rhei intervene --task <id> [--slot <N>] -m "<message>"` discovers
the live run's loopback address and delivers the message through the identical
`/intervene` boundary and capability gate as the composer. It is the headless
sibling of the composer, not a second code path [§AR-rhei-viz-flow.7](../architecture/rhei-viz-flow.spec.md#7-intervene-the-single-mutation-boundary). A surface
attached with `rhei attach` is the third client of that same boundary
[§FS-rhei-run-headless.5](rhei-run-headless.spec.md#5-rhei-attach): the loopback server is the run's **control server**,
and the browser page is one thing it serves rather than the reason it runs.

In the static surface (§7.2) the terminal shows a representative transcript so the
layout has realistic shape, and the composer is shown disabled — its messages are
illustrative and are not delivered.

### 5.1. Human Gate Transitions

When the selected node's current machine state is `gating: true`, the live
dashboard may offer a **Human gate** action block. The block lists the state's
explicit outgoing transitions as human choices and submits the selected task,
`from` state, and `to` state to the run's loopback server. The server must apply
the same explicit human-transition semantics as `rhei transition`: compare the
task's current state to `from`, validate that `to` is a legal outgoing
transition, honor callbacks and callback redirects, write the plan through the
normal atomic transition path, and report the effective target state.

Because it applies the same semantics, it also inherits the terminal-result
obligation ([§FS-rhei-transition-cmd.3.2](rhei-transition-cmd.spec.md#32-terminal-result-on-entry)). A human finishing a ticket by hand is
the case where the reason matters most, so the block carries a single-line
**Result** field beside the choices and submits it as the transition's result
message, exactly as `rhei transition --result` does. The field is always
offered — a result is legitimate on any hop — and is marked as needed when the
chosen target is a `final: true` state. The marking says *needed*, not
*required*: a result already on disk satisfies the obligation just as well, and
labelling the field required would have told the operator a blank release was
impossible in a case where it succeeds. It is never filled in on the operator's
behalf: submitting it blank sends no message, and the server refuses a terminal
release that has neither a message nor a result already on disk, with the
rejection reason naming `rhei transition <id> --from <state> --to <state>
--result "<why>"`. A blank or whitespace-only field is the same as no field at
all. The rejection is rendered in the block, and the run is unaffected either
way: it picks up an applied transition on its next pass.

This is the only plan-state mutation allowed from the dashboard. It is available
only while the loopback dashboard is live and only for tasks currently in a
gating state. The static and frozen surfaces render gate choices as inert
inspection affordances, not as working controls. Intervene remains separate: it
messages a running agent and never transitions or edits the plan (§5).

## 6. State-Machine Graphs

The machine panel draws the resolved state machine as a graph so the workflow is
legible alongside the work. Disjoint workflows render as **separate graphs**: the
machine is split into connected components over non-wildcard transitions, and each
component is drawn on its own.

- **Multi-state components** render first, ordered by canonical state order (§10);
  a machine with more than one is labeled as `N disjoint` tracks, matching node
  kinds routed through `profiles`/`node_policy` (e.g. a feature, a bugfix, and a
  research track).
- **Isolated** single states render together.
- **Terminal** states reachable only via `from: "*"` wildcards render as a final
  graph labeled "reachable from any state (wildcard)," rather than gluing the
  tracks together through the escape edges.

Each graph uses a layered layout by longest path. Cycles — counted review/fix
loops, rework and reopen edges — are tolerated: a back edge (to a node still on
the DFS stack) is excluded from layering and drawn as a dashed loop with a
distinct arrowhead, so a `review ⇄ fix` loop reads as a loop. Each node is a
state pill colored by §10, carrying its flags in a tooltip. A legend lists every
state with its description and doubles as the prompt index.

Clicking a state — in a graph or the legend — opens the **state-detail** panel:
its flags (`initial`/`terminal`/`gating`, `counted ×N` when the state
declares `visits: N`, and `waiting on <label>` when its poll declares one
(§FS-rhei-states.2.5)), description, and the raw prompt template for the state.
Selecting a task highlights its current state across every graph and shows the
prompt *instantiated* for that task in the surroundings inspector (§4), so the
operator sees both the template and its resolution. The plan's overview prose, if
present, is shown above the graphs as "what this Rhei is doing."

## 7. Dynamic and Static Rendering

One renderer and one model (§8) drive two modes.

### 7.1. Dynamic (live during `rhei run`)

The live surface is served on the dashboard's loopback server alongside `rhei run`
[§FS-rhei-run-tui.1.6](rhei-run-tui.spec.md#16-browser-dashboard) and refreshes through the same polling loop as the rest of
the dashboard. Liveness is shown by content changing in place, never by movement,
per [§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux) §4: a poll updates row text, pills, running-now chips, and the
agent terminal where they sit, with no layout shift, stable ordering, and the
operator's scroll position and text selection preserved. The surface tolerates a
temporary plan-reload failure by rendering the last-good snapshot already cached.
If the loopback transport becomes unavailable after a live snapshot, the surface
keeps the last-good plan data but clears runtime-only "running now" affordances
after a short retry grace, so a closed run cannot leave rows spinning forever.
Artifact and log links open through the open-in-editor route (§11). When the run
finishes and the loopback server exits, the surface writes a frozen,
self-contained HTML artifact under `runtime/` from the final snapshot, so the
operator keeps an inspectable view.

### 7.2. Static (offline or frozen)

The static surface is a single self-contained HTML file generated from a plan — or
a Directory Workspace, merging the `index.rhei.md` with any standalone plan files
— together with its resolved state machine, with the model embedded inline and no
live polling. Artifact and log links are **illustrative**: the files they point to
materialize under `runtime/` only during a live run. The agent terminal shows a
representative transcript and the intervene composer is inert (§5). This is what
`cargo xtask examples viz <name>` produces today and what the frozen run-end
artifact (§7.1) reuses; it is also what the `rhei viz` command renders.

`rhei viz` defaults its output to the workspace's `runtime/` directory —
`runtime/<input>.html` for a plan file, `runtime/rhei-viz.html` for a workspace
directory — the same location the run-end freeze (§7.1) writes to, so generating
a view never drops an HTML file beside a checked-in plan. `--output <FILE>`
overrides the location.

Tickets carry the same project-qualified ids the rest of the CLI uses
(`auth.1`), including for a bare rhei rendered directly ([§AR-rhei-panta.2](../architecture/rhei-panta.spec.md#2-load-model)).

### 7.3. Panta projects render as one graph

Pointed at a project directory, `rhei viz` loads the merged project
([§AR-rhei-panta.2](../architecture/rhei-panta.spec.md#2-load-model)) and renders **one** graph: every rhei's tickets in the
single project-qualified id space, cross-rhei `**Prior:**` edges drawn like any
other edge, and Directory Workspace rheis included alongside single-file ones.
Per-task state history is read under each ticket's owning rhei execution root,
not the project root, because that is where a project keeps its ledgers
([§AR-rhei-panta.5](../architecture/rhei-panta.spec.md#5-execution-root-and-per-rhei-runtime)).

Pointed at a **member rhei** of a project, `rhei viz` renders that project
narrowed to the named rhei ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)). Narrowing keeps the one-hop
neighbours a kept ticket's `**Prior:**` points at, and the ancestors a kept
ticket hangs from. Dropping the far end of a cross-rhei edge would erase the
dependency rather than scope it, leaving a page that reads as if the ticket had
no prerequisite at all — the specific misrepresentation this view exists to
prevent.

A page therefore never needs a limitation banner: what it renders is the graph
the rest of the CLI operates on.

## 8. Data Contract

Both modes consume one model. The live `/snapshot` payload is a superset of the
embedded static bundle:

```ts
type Snapshot = {
  plan_title?: string;
  plan_state?: string;          // derived, §9
  about?: string;               // plan overview prose, shown above the machine
  accounting?: AccountingRunSummary;
  tasks: TaskRow[];             // id, title, parent, depth, state, visit_count?, prior, history?
  machine: Machine;             // the resolved state machine, flattened (below)
  capabilities?: Capabilities;   // live-only mutation affordances
  // plus existing run, slot, journal, ready/deferred, and link fields
};

type Capabilities = {
  gate_transition?: boolean;     // true only when POST /transition-gate is wired
};

type StateHistoryEntry = {
  from: string;                  // parsed from `<task-id> <from>@<to>`
  to: string;
};

type Machine = {
  name: string;
  states: MachineState[];
};

type MachineState = {
  name: string;
  description?: string;
  instructions?: string;        // effective prompt template, variables unresolved
  visits?: number;              // counted-loop budget when declared
  initial: boolean;             // entry state of at least one profile (§FS-rhei-states; initiality is per-profile)
  terminal: boolean;
  gating: boolean;
  waiting_on?: string;          // person this state's poll waits on, when it declares one (§FS-rhei-states.2.5)
  transitions: Transition[];    // explicit edges first, then applicable wildcards
  inputs: Artifact[];
  outputs: Artifact[];
  template_context?: TemplateContext;
  template_contexts?: TemplateContext[]; // authored fanout variants for static previews
};

type TemplateContext = {
  target?: string;
  target_slug?: string;
  model?: string;
  model_provider?: string;
  model_name?: string;
  agent?: string;
  agent_mode?: string;
};
type Transition = { to: string; condition?: string; wildcard: boolean };
type Artifact = { name: string; path: string; description?: string; optional: boolean };
```

Rules:

- **The host supplies the machine.** `rhei run` resolves the state machine and
  passes it to the surface; the static renderer resolves it from the plan's
  `**States:**` declaration (an override, then a matching sibling `states.yaml`,
  then the built-in default for `rhei`). The surface never parses plans or state
  YAML itself. [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification)
- **Transitions are flattened per state.** Each non-terminal state carries its
  explicit outgoing edges plus any `from: "*"` wildcard edges that apply to it,
  each marked `wildcard`, so the inspector shows the real set of legal exits.
- **Initiality is per-profile.** A machine has no single initial state; each
  profile declares its own entry state, and a state may be the entry of one
  profile but not another [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification). The flattened `initial` flag is the
  union — true when a state is the entry of at least one profile — which is all
  the surface needs to mark track-entry states across disjoint graphs.
- **Templates resolve per node.** `{task_id}`, `{task_title}`, `{visit_count}`,
  `{visits}`, `{model}` and similar scalars, and `{input/output.<name>.path}`
  artifact references, are resolved against the selected node when rendering its
  prompt (§4) and artifact links. A live render substitutes the running task's
  real values, including the concrete target/model/agent from its slot. A static
  render resolves authored single-target/model states directly, preserves any
  `-N` visit suffix from the plan row as `visit_count`, and renders multi-target
  or multi-model fanout states once per authored context so artifact paths and
  prompt previews do not point at guessed values.
- **Compact rollups in `/snapshot`, detail elsewhere.** Each task row may carry
  compact direct and subtree accounting rollups; invocation-level detail is served
  from a separate loopback endpoint so polling stays light. [§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)
- **Intervene capability is per slot.** For a task assigned to a live slot,
  `task_runtime[id].intervene` is `true` only when that slot's agent keeps a
  writable stdin held open so a `/intervene` message can be delivered now (the
  agent's `intervene_stdin` opt-in). The surface gates the message composer on
  this flag (§5); it is absent on a static render, so the composer is never
  offered as deliverable offline.

## 9. Plan-State Derivation

A `.rhei.md` plan carries no explicit top-level state. The surface derives
`plan_state` from top-level tasks only, using the resolved machine to classify
terminal states:

| Condition | Derived plan state |
| --- | --- |
| All top-level tasks are `draft` | `draft` |
| All top-level tasks are `completed` | `completed` |
| All top-level tasks are terminal and at least one is not `completed` | `archived` |
| Any top-level task is assigned to a running slot, or is active-like | `active` |
| Otherwise | `pending` |

Terminal states are those the machine flags terminal (built-ins: `completed`,
`cancelled`, `archived`, `failed`). Active-like means a non-terminal state in the
`active` category (§1.1): not `draft`, not `pending`, and not a profile entry
state. A plan of `pending` tasks therefore derives `pending`, not `active`, under
any profile.

## 10. Level-Grouped State Vocabularies

Plan, task, and subtask levels may draw from different state vocabularies, and the
surface keeps them visually distinct so similarly named states at different levels
are not conflated. States sort by this canonical order, with unknown states sorted
alphabetically after known states:

```text
draft -> pending -> in_progress -> in-progress -> needs-review -> review ->
prove -> consolidate -> fix -> agent-review -> agent-review-fix ->
human-review -> active -> completed -> blocked -> failed -> cancelled ->
archived
```

Custom non-terminal states not in the built-in color map get a stable
name-derived color, desaturated to the same level as the calm palette so a
project-specific state never out-shouts a built-in attention state and is still
distinguishable from terminal work, per [§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux) §3.2.

## 11. Open-in-Editor Route

Artifact and log links resolve through a loopback `GET /open?path=<rel>` endpoint
rather than `file://` navigation, so a click opens the file in the operator's
editor on the same machine. The route:

- accepts only a workspace-relative `path`; absolute paths and paths that escape
  the workspace root are rejected with `400`;
- resolves the editor from `RHEI_EDITOR`, then `VISUAL`, then `EDITOR`, then a
  platform opener (`xdg-open`, `open`, or `cmd /C start`);
- launches the editor detached on the resolved absolute path and replies `204`
  without waiting; a launch failure replies `500`;
- remains loopback-only, like the rest of the dashboard server.

The endpoint spawns a local process but never reads, writes, or transitions plan
state, so the surface stays read-only with respect to the plan.

## 12. Supplementary Dense Views

For scanning many nodes at once, the surface may offer dense chart overviews
alongside the Flow view. They consume the same `/snapshot` data and the same
console-first language, and are secondary to the Flow view rather than the primary
surface:

- **Gantt** — one row per item (plan row, each top-level task, each child indented
  beneath its parent) against state axes split into per-level groups when level
  vocabularies differ (§10).
- **Cube** — a dense top-level-task by descendant-slot heatmap, with the derived
  plan state as a full-width strip; can switch from state coloring to a
  subtree-cost heatmap. [§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)
- **Sankey** — ribbons from each top-level task to its descendants' states, ribbon
  thickness equal to the descendant count, or to cost. [§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)

Plans without descendants render a useful monochrome empty state rather than a
blank chart, per [§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux) §7.

## Related Specifications

- [Console-First Visualization UX](rhei-viz-ux.spec.md) — the shared look-and-feel
  every surface follows. [§FS-rhei-viz-ux](rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux)
- [`rhei run` TUI and Transition Journal](rhei-run-tui.spec.md) — the terminal
  sibling, the loopback dashboard host, and the `AgentOutput` stream the live
  terminal renders. [§FS-rhei-run-tui](rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal)
- [Cost Accounting](rhei-cost-accounting.spec.md) — the accounting data the
  running view and cost overlays present. [§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)
- [States Specification](rhei-states.spec.md) — the state machine, profiles,
  artifact contracts, and counted loops the machine graphs and inspector render.

## Future Work

- Filter or dim the plan by state, level, or category.
- Diff visualization against another snapshot or git ref.
- Cross-plan navigation when a workspace bundles many plans.
- Wider intervene controls (pause, retry, redirect) beyond messaging.
