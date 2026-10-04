# AR-rhei-viz-flow: Flow Visualization Architecture

This document expands [§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization) (Flow Visualization) and [§FS-rhei-viz-ux](../functional-spec/rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux) into
the component architecture they imply: which crates own the model, the
derivation, and the renderer; how the live (`rhei run`) and static (offline /
frozen) surfaces render from one model; and the boundaries that keep the surface
read-only with respect to plan state. [§FS-rhei-run-tui](../functional-spec/rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal) [§FS-rhei-cost-accounting](../functional-spec/rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)
[§FS-rhei-states](../functional-spec/rhei-states.spec.md#fs-rhei-states-rhei-states-specification)

## 1. Context: two renderers, one design

The Flow design exists today in two places that share an intent but no code, and
the spec's parity requirement ([§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization) §7, §8) is the instruction to
collapse them onto one model and one renderer.

| | Static (the Flow prototype) | Dynamic (the legacy dashboard) |
|---|---|---|
| Code | `xtask/src/viz.rs` + `xtask/assets/viz-template.html` | `crates/rhei-tui/src/dashboard.rs`, `dashboard/state.rs`, `dashboard/html.rs` |
| Model | `Plan { plan_title, plan_state, about, tasks, machine }` — matches §8 | `SnapshotPayload { slots, recent, accounting, plan_state, tasks: TaskRow[], … }` — **no `machine`, no `about`** |
| UI | Flow design (List/Graph + surroundings + machine graphs) | Tab strip: Running / Tasks / Accounting / Links / Logs |
| Runs | offline, from a plan file | live, on the loopback server during `rhei run` |

`xtask/src/viz.rs` states the intent in its own header: it *dogfoods the
`rhei viz` command and keeps the data shape and derivation rules consistent with
the spec so the implementation migrates cleanly.* The static side is therefore
the **canonical source** of the Flow design; the live dashboard is the surface to
be replaced.

A second fact constrains the design: **`rhei-tui` depends on neither `rhei-core`
nor `rhei-validator` today.** It receives plan data through a `PlanLoader` closure
and a `StateArtifactContracts` map handed in by the host
(`DashboardSink::start_with_plan_and_artifacts`, `rhei-tui/src/dashboard.rs`).
Preserving that decoupling is a recorded decision (§10).

## 2. Target: one model, one renderer, two hosts

```
        rhei-core (AST: Rhei, Task)         rhei-validator (StateMachine, profiles, transitions)
                  \                                   /
                   \                                 /
                ┌──────────────────────────────────────────────┐
                │  rhei-viz  (build layer)                       │
                │  • builder: (plan + resolved machine) → VizModel│
                │  • machine flattening (explicit+wildcard;       │
                │    profile-initial union, §8)                   │
                │  • plan_state derivation (§9)                   │
                │  • state→category classifier (§1.1)            │
                └───────────────────────┬────────────────────────┘
                                        │ produces
                ┌───────────────────────▼────────────────────────┐
                │  rhei-viz-model  (pure layer, no rhei deps)     │
                │  • §8 serde structs (VizModel, Machine, …)      │
                │  • the single self-contained HTML/CSS/JS asset  │
                │  • render_static(model) → String                │
                └──────┬───────────────────────────────┬─────────┘
                       │ depends on (asset + structs)    │ depends on (builder)
                 ┌─────▼──────┐                    ┌─────▼──────────────────────┐
                 │  rhei-tui  │                    │  rhei-cli / xtask           │
                 │ (UI host)  │                    │  • `rhei viz` (static cmd)  │
                 │ serves `/` │                    │  • run wiring (machine +    │
                 │ builds the │                    │    loader into UI hosts)    │
                 │ SUPERSET   │                    │  • xtask dogfood (thin)     │
                 │ /snapshot; │                    └─────────────────────────────┘
                 │ renders TUI│
                 └────────────┘
```

The architecture rests on three "exactly once" invariants:

- **One asset.** A single self-contained HTML/CSS/JS string is the *only* renderer.
  The static path inlines the model into it and writes a file; the live path
  serves the same string at `/` and lets its JS re-render from `/snapshot`.
  A byte-identical *asset* ([§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization) §7) is guaranteed by construction, not by
  keeping two templates in sync; rendered output then differs only by the runtime
  overlay (§4). The current `dashboard/html.rs` is retired.
- **One model.** Both surfaces consume the §8 `VizModel`; the live payload is a
  superset (§4).
- **One derivation.** Category classification, plan-state derivation, and machine
  flattening exist once in `rhei-viz` and are called by both paths (§8).

## 3. Crate layering

| Crate | Role | Dependencies |
|---|---|---|
| `rhei-viz-model` (new) | §8 serde structs, the HTML/CSS/JS asset, `render_static` | none (pure) |
| `rhei-viz` (new) | builder from `(plan, resolved machine)`; flattening; derivation (§8/§9/§1.1) | `rhei-core`, `rhei-validator`, `rhei-viz-model` |
| `rhei-tui` | serves the asset at `/`; builds the superset `/snapshot`; renders the terminal TUI from the same model plus runtime events; owns `/open`, `/log`, `/intervene`, and gate/intervene sinks | `rhei-viz-model` only |
| `rhei-cli` | `rhei viz` static command; wires the resolved machine + plan loader into the dashboard | `rhei-viz`, `rhei-tui` |
| `xtask` | dogfood static renderer (thin wrapper over `rhei-viz`) | `rhei-viz` |

The model/builder split is deliberate (decision §10.2): `rhei-tui` gains only the
pure-struct dependency it needs to serialize the payload and serve the asset, and
the host (`rhei-cli`) still builds the model and feeds it through the existing
closure pattern. `rhei-tui` never learns to parse plans or resolve state
machines.

The browser dashboard and terminal TUI are two hosts over that same contract.
Both receive the host-built `VizModel`; live-only state is an overlay from
`RunEvent`s. Rendering may differ to fit the medium, but lookup, readiness, and
mutation boundaries stay in the shared UI model rather than in individual view
renderers. [§FS-rhei-run-tui.1.5](../functional-spec/rhei-run-tui.spec.md#15-tui-surface)

## 4. The data contract: static base + runtime overlay

[§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization) §8 states it: *the live `/snapshot` payload is a superset of the
embedded static bundle.*

```
SnapshotPayload (live) = VizModel (static base) + RuntimeOverlay
  VizModel       : plan_title, plan_state, about, tasks: TaskRow[], machine
  RuntimeOverlay : slots[], recent[], links[], accounting, ready/deferred,
                   finished, summary, *_at_ms
```

Reconciling today's two shapes into one model is mechanical and is the bulk of
the migration:

| Static (`xtask` `Plan`) | Live (`SnapshotPayload`) | Unify on |
|---|---|---|
| `title` | `plan_title` | `plan_title` |
| `state` | `plan_state` | `plan_state` |
| `tasks[].subtasks[]` (**nested**) | `tasks: TaskRow[]` (**flat**, `parent`/`depth`) | **flat `TaskRow[]`** (§8) |
| — | `machine`, `about` | **add to the live payload** |
| — | `slots`, `accounting`, `recent` | present live, absent static |

**Graceful degradation by field-presence** is what lets one renderer serve both
modes. The JS keys behavior off whether a field is present, never off a mode flag:
when the runtime overlay is absent (static), the running-now panel hides (§5), the
intervene composer is shown disabled, the agent terminal shows a representative
transcript, and cost overlays are empty. This is exactly [§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization) §7.2.

## 5. Data flow

### 5.1. Static path (`rhei viz`, xtask dogfood, run-end freeze)

```
plan.rhei.md (+ workspace index, + states.yaml/override)
  → rhei-core::parse / workspace          (parse tasks)
  → rhei-validator resolve machine        (override | own root | project root | built-in rhei)
  → rhei-viz::build(plan, machine)         (VizModel: flatten machine, derive state, classify)
  → rhei-viz-model::render_static(model)   (inline JSON into the one asset)
  → self-contained .html
```

### 5.2. Dynamic path (`rhei run`)

```
RunEvent stream  ──▶ DashboardState (rhei-tui)         live slots, traffic, accounting
plan loader + resolved machine (from rhei-cli)  ──▶ VizModel base (built via rhei-viz)
                         │
GET /snapshot  ──▶ SnapshotPayload = VizModel + RuntimeOverlay  (JSON)
GET /          ──▶ the one asset (rhei-viz-model)
                         │  JS fetches /snapshot on the existing poll loop,
                         ▼  re-renders in place (no layout shift), per §FS-rhei-viz-ux §4
                     browser
```

The run process must now hand the dashboard the **resolved machine** and the
plan's overview prose (`about`) in addition to today's plan loader and artifact
contracts; the dashboard includes the flattened machine in `/snapshot`.

### 5.3. Run-end freeze (correct by construction)

When the run finishes and the loopback server exits ([§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization) §7.1), the host
captures the final superset payload and calls the *same*
`rhei-viz-model::render_static`, writing a frozen self-contained HTML under
`runtime/`. Because the freeze reuses the static path, the frozen page is
identical to the live one except that it no longer updates — no separate export
renderer exists.

## 6. Live agent terminal: full durable log

**Decision (§10.3): the live terminal renders the full durable transcript, not the
`SLOT_TRAFFIC_LIMIT` ring.** Agent stdout/stderr already flow through
`spawn_agent_output_reader` (`agent_spawn.rs`) to (a) the durable per-task log
(`agent_log_path` / `program_log_path`, `run_agent_mode.rs`) and (b) the
`DashboardSlot.traffic` ring (capped at `SLOT_TRAFFIC_LIMIT`,
`dashboard/state.rs`). The ring is fine for an at-a-glance chip preview but cannot
back a scrollback terminal.

The terminal pane therefore sources the **durable log**, served by a dedicated
incremental endpoint, decoupled from `/snapshot` so polling stays light:

```
GET /log?task=<id>&state=<state>&from=<byte-offset>
  → tails the per-task durable log from <byte-offset>; replies with new bytes
    and the next offset. Loopback-only, like /open. Workspace-relative resolution
    only; paths that escape the workspace root are rejected (as in /open, §FS-rhei-viz §11).
```

`/snapshot` continues to carry the small `traffic` preview for running-now chips;
the full ANSI scrollback comes from `/log`. In the static surface there is no
`/log`; the terminal shows the representative transcript embedded in the model.

## 7. Intervene: the single mutation boundary

The surface is read-only with respect to plan state, with **exactly one hole**:
[§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization) §5 lets the operator message a running agent. This is the only
inbound, state-changing-adjacent path, and it is the focal point of any security
review.

**Hard architectural boundary.** `POST /intervene` carries a message destined for
a single running agent's **stdin and nothing else**. There is no path from the
loopback server to plan-state mutation: it cannot transition a task, edit the
plan, write task metadata, or invoke the orchestrator's transition logic. The
route resolves a slot, writes bytes to that slot's agent stdin, and returns.

```
POST /intervene { slot | task_id, message }
  → orchestrator slot registry → child stdin writer (that slot only)
  ✗ no transition, no plan write, no metadata mutation
```

**Two clients, one boundary.** The Flow composer is one client of `POST
/intervene`; the `rhei intervene` CLI is the other, for operators without a
browser. To let a separate process find the ephemeral loopback port, the
dashboard publishes its URL on startup to `runtime/dashboard.json` (`{ url, pid }`)
and removes the file when the run ends. `rhei intervene` resolves the run's
workspace from its `--plan`/`.` argument, reads that file, and POSTs to the same
route — so the CLI inherits the identical capability gate, audit log, and
read-only-with-one-hole boundary; it adds no new mutation path. A failure to
publish the file costs only headless intervention, never the run.

**New plumbing this requires.** Delivery is **agent-capability-dependent**. Most
stdin-prompt transports are EOF-driven: `agent_spawn.rs` writes the prompt and
closes stdin so the agent starts. Those agents, and agents invoked with a
one-shot prompt argument, are not interactively reachable. A profile that
explicitly declares `intervene_stdin` has its child stdin kept **piped and held
open** for the process lifetime, with the writable handle registered in a
per-slot registry the `/intervene` route can reach. `claude-code` is adapted to
Claude Code's stream-json stdin mode when `intervene_stdin` is set, so its `-p`
invocation stays live rather than carrying a prompt argument. Unsupported agents
report not interactively reachable rather than silently dropping the message.
Concurrent fanout invocations for the same task are distinct slot registrations;
releasing one invocation must not remove a still-running sibling.

**The surface gates the composer on this capability up front.** The registry
answers `reachable(task, slot)` from the same per-slot map `deliver` consults, and
the dashboard carries that answer in the snapshot as `task_runtime[id].intervene`.
The Flow composer renders only when it is true, so an operator learns an agent
can't be messaged *before* typing rather than after a failed send ([§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization)
§5). Because the gate and delivery share one registry, they cannot disagree: no
built-in profile enables `intervene_stdin` by default, so the composer stays
hidden until a profile opts in.

**Decision (§10.1): every intervention is logged.** Each delivered message is
appended to a durable audit trail at `runtime/interventions.log` — timestamp,
slot, task id, current state, byte length, and the message — and is echoed into
the task's durable agent log so it appears inline in the transcript and the live
terminal. An intervention that cannot be delivered (unsupported agent, closed
stdin, write failure) is logged with its failure reason and is not silently lost.
The audit trail makes the one mutation boundary observable after the fact, which
is what a security review will check.

## 8. Shared derivation: a single source of truth

The rules that classify and derive state must produce identical results in both
modes, so they live once in `rhei-viz` and are called by both the static builder
and the live dashboard. They are the logic normatively defined in the spec:

- **State→category classification** — [§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization) §1.1, evaluated top-to-bottom,
  first match wins, with `active` as the persisted-state catch-all; `live` is
  reserved for the runtime slot overlay.
- **Plan-state derivation** — [§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization) §9, with active-like tied to the `idle`
  category so a plan of `pending` tasks derives `pending`, not `active`, under any
  profile.
- **Machine flattening** — [§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization) §8: per-state explicit transitions plus
  applicable `from: "*"` wildcard edges (marked `wildcard`); `initial` is the
  **union over profiles** (true when a state is the entry of at least one
  profile), since initiality is per-profile ([§FS-rhei-states](../functional-spec/rhei-states.spec.md#fs-rhei-states-rhei-states-specification)), not a single
  machine-wide value.
- **Template context derivation** — [§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization) §8: counted task visits, authored
  target/model selectors, and multi-target/model fanout variants are flattened
  into the model so the renderer can instantiate prompts and artifact links
  without hard-coded demo values.

Today these are duplicated — `xtask` computes its own plan state and the dashboard
has `derive_plan_state_with_active_roots`. That duplication is precisely what lets
static and live drift; consolidating it into `rhei-viz` removes the drift source.

## 9. Convergence plan (ordered, low-risk)

Each step leaves both surfaces working; the user-visible flip is steps 4–5; new
behavior lands last.

1. Extract the canonical model + asset from `xtask` into `rhei-viz-model`
   (confirmed canonical source, §10.4); `xtask` becomes a thin caller.
2. Move classification, plan-state derivation, and machine flattening into
   `rhei-viz`; have both `xtask` and the dashboard call them.
3. Flatten the static task model to `TaskRow[]`; reconcile the field names in §4.
4. Add `machine` + `about` to the live `/snapshot`; serve the `rhei-viz-model`
   asset at `/` instead of `dashboard/html.rs`.
5. Fold the legacy tabs into the asset as the §12 *supplementary* surfaces
   (Tasks / Slots / Cost / Journal / Links / Gantt / Cube / Sankey); the Flow view
   becomes primary. Retire `dashboard/html.rs`.
6. Add the durable-log terminal endpoint `/log` (§6) and the run-end freeze (§5.3).
7. Add the `/intervene` channel with its stdin registry and audit log (§7), and
   the `rhei viz` subcommand (`rhei-cli` → `rhei-viz-model::render_static`).

## 10. Decisions recorded

1. **Intervene is the one mutation boundary, and it is logged.** `/intervene`
   delivers bytes to a single agent's stdin and nothing else; no server path
   reaches plan state. Every intervention (and every delivery failure) is written
   to `runtime/interventions.log` and mirrored into the task transcript (§7).
2. **Keep the model/builder split.** `rhei-viz-model` (pure) vs. `rhei-viz`
   (builder, depends on core+validator) keeps `rhei-tui` free of `rhei-core` and
   `rhei-validator` (§3).
3. **The live terminal sources the full durable log**, via a dedicated `/log`
   endpoint, not the `SLOT_TRAFFIC_LIMIT` ring (§6).
4. **The `xtask` prototype is the canonical renderer source** and is promoted into
   `rhei-viz-model`; `dashboard/html.rs` is retired, not extended (§1, §9).

## Related Specifications

- [Flow Visualization](../functional-spec/rhei-viz.spec.md) — the views and the
  data they show. [§FS-rhei-viz](../functional-spec/rhei-viz.spec.md#fs-rhei-viz-flow-visualization)
- [Console-First Visualization UX](../functional-spec/rhei-viz-ux.spec.md) — the
  shared look-and-feel. [§FS-rhei-viz-ux](../functional-spec/rhei-viz-ux.spec.md#fs-rhei-viz-ux-console-first-visualization-ux)
- [`rhei run` TUI and Transition Journal](../functional-spec/rhei-run-tui.spec.md)
  — the loopback host, the `AgentOutput` stream, and the durable logs. [§FS-rhei-run-tui](../functional-spec/rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal)
- [Cost Accounting](../functional-spec/rhei-cost-accounting.spec.md) — the
  accounting overlay the running view presents. [§FS-rhei-cost-accounting](../functional-spec/rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)
- [States Specification](../functional-spec/rhei-states.spec.md) — profiles,
  transitions, and per-profile initial states the machine flattening resolves.
- [Agent-Orchestrator Workflow](agent-orchestrator-workflow.spec.md) — the run
  loop and slots the dynamic surface observes. [§AR-agent-orchestrator-workflow](agent-orchestrator-workflow.spec.md#ar-agent-orchestrator-workflow-agent-orchestrator-workflow-architecture)
