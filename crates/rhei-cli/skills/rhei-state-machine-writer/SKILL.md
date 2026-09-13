---
name: rhei-state-machine-writer
description: Design and generate custom Rhei state machine YAML files from project specifications and team structures. Use when users need a workflow that doesn't fit the default rhei state machine — domain-specific phases, multi-team handoffs, approval gates, counted review loops, model-specific states, or automated callback integrations.
---

# Rhei State Machine Writer

Design a custom Rhei state machine only when the built-in `rhei` workflow is too generic. Produce a YAML file matching the project's real phases, actors, gates, retries, and automation hooks, ready to be referenced by a plan's `**States:**` declaration. The state machine writer runs before the plan writer: it defines the workflow the plan later uses.

## When To Use This Skill

Use this skill when the project needs any of:

- Domain-specific phases (`ingesting`, `qa`, `staging`, `release-ready`)
- Human approval gates (`security-review`, `legal-review`, `manager-approval`)
- Multi-team handoffs with explicit transitions between owners
- Counted retry/review loops using `visits`
- Model-specific or multi-model execution using `models` / `model` / `all_models`
- State-level artifact contracts (`inputs:` / `outputs:`) enforced by the runtime
- Transition callbacks or conditional routing
- Distinct flows per node kind (e.g. `task` goes through review but `bug` skips it)

Do not use it when the default `rhei` machine is enough: a simple pending → completed flow with no specialized phases, gates, or branching.

## Required Inputs

Gather both before designing. If either is missing, ask and stop.

### 1. Project Specification

Extract: distinct phases; their ordering and branching; quality gates and checkpoints; failure modes and recovery paths; per-state artifact contracts (files required before entering or after leaving a state); places where callbacks/automation should run. Sources: requirements docs, READMEs, `AGENTS.md`, existing plans, or the user's description.

### 2. Team / Actor Structure

Map actors to behavior: who must approve work; who performs it; handoff points between owners; states that must not advance autonomously; different flows for different node kinds (e.g. `bug` bypasses design); model-specific execution if different models own different states. Do not invent approval authorities or team structure.

## Output Contract

Emit one YAML file conforming to the current Rhei state-machine format. Sections: `name`/`version`, optional `models`, `states`, `transitions`, `profiles`, `node_policy`, and optional `callbacks` / `error_handling`.

When invoked from `rhei-template-writer`, emit template-ready YAML for `<template>/states.yaml`: the full machine, with any needed `{{...}}` instantiation variables preserved and the template-required ASCII diagram comment block before `name:`. Do not return only a transition sketch or prose description.

### Root fields

Required: `name` (meaningful project-derived identifier), `version`, `states`, `transitions`, `profiles`, `node_policy`.

Optional: `models`; `personality` (machine-level role-framing default); `callbacks` (platform-specific mappings — `cli`, `nodejs`, ...); `error_handling`.

### State fields

Common: `description`; `instructions` (strongly recommended for every non-terminal state); `final: true` on terminal states.

> `initial: true` is **not** a state field. Initial states are declared per profile under `profiles`; `node_policy` routes each node to a profile.

Optional workflow controls:

- `gating: true` — state requires explicit human action to exit
- `personality` — state-specific role framing (overrides the machine-level default)
- `visits: <integer>` — counted loop re-entry limit (minimum `1`)
- `execute_on: <scope>-<event>` — make a non-leaf task in this state a
  *supervisor* of its subtree, holding the subtree between visits. The scope is
  `child` (its direct children only) or `descendant` (any depth); the event is
  `terminal` (the task finished) or `transition` (every hop it takes), so the
  four legal values are `child-terminal`, `child-transition`,
  `descendant-terminal`, and `descendant-transition`. Requires
  an agent-bearing state and a self-loop transition (the release edge); never
  valid on `final`, `gating`, `program`, `poll`, `all_models`, or `all_targets`
  states
- `all_models: [<model>, ...]` — run the state once per listed declared model
- `model: <model>` — bind the state to one declared model (never set both `all_models` and `model`)
- `inputs:` / `outputs:` — artifact contracts (see *Artifact Contracts*)
- `agent:` — override the agent id `rhei next` assigns in this state

### Transition fields

Required: `from` (source state name, or `"*"` wildcard — typically a global cancellation edge); `to`; `description`.

A wildcard edge whose `to` is `final: true` is an escape hatch, not a way forward. `rhei run` never takes one to advance a task, so a state whose only matching rule is that edge strands every task that reaches it, and the machine is refused when it loads. Out of a `gating: true` state the same edge does count, because a human takes it with `rhei transition`. An explicit `from: <state>` edge counts whatever its target, `cancelled` included.

Optional: `on_leave` (callback on the source before state change); `on_enter` (callback on the target after); `condition` (expression `rhei run` uses to choose among outgoing edges); `exit_code` (routes by subprocess exit code under `rhei run`); `timeout`.

Condition operands are integer literals, `visitCount` and `visits`, `pollAttempts` and `pollMaxAttempts` on poll states, and `openDescendants` — the number of non-terminal descendants of the transitioning task, evaluated against the plan as re-read after the subprocess exits. `openDescendants` is how a parent's machine *selects* its terminal edge; the engine's descendants-first guard still decides whether that edge may be taken.

Callback style: `on_leave: "cli:bash ./workflow.sh handoff-review"`.

### Profiles

Every machine declares at least one profile. A profile is the *policy* applied to a node: where it starts and which states it may ever hold.

```yaml
profiles:
  default:
    initial: <state-name>
    allowed: [<state-name>, <state-name>, ...]
```

Rules:

- Every profile declares `initial` and a non-empty `allowed`, and `initial` is a member of `allowed`.
- `allowed` includes at least one `final: true` state.
- **Reachability:** for every non-final state in `allowed`, a path to a final state in `allowed` exists using only transitions whose `to` is also in `allowed` and that count as a way out (see *Transition fields*); and every non-initial state in `allowed` is reachable from `initial` the same way. This is the core constraint the Transitions rules below build on.

### Node policy

```yaml
node_policy:
  root: <profile-name>       # required — the plan root (always kind `rhei`)
  default: <profile-name>    # required — fallback for kinds not matched below
  by_type:                   # optional — per-kind overrides
    <kind>: <profile-name>
  overrides:                 # optional — ordered, first-match-wins
    - match: { type: <kind>, level: <n> }
      profile: <profile-name>
```

Rules: `root` and `default` are required and must name defined profiles. Every `by_type` key must be a declared non-root node kind; `rhei` is reserved and must never be a `by_type` key. Resolution order for a non-root node is `overrides` → `by_type` → `default`.

## Schema Skeleton

```yaml
name: <project-derived-name>
version: 1.0
personality: |
  <optional machine-level role framing>
models: [<model-name>, ...]            # optional

states:
  <state-name>:
    description: <what this phase means>
    personality: |                     # optional, overrides machine default
      <state-specific framing — supports template variables>
    instructions: |
      <what the actor does here; template variables like {task_id},
      {task_title}, {visit_count}, {visits}, {model}, {input.<name>.path},
      {output.<name>.path}, {meta.<key>} are resolved by `rhei next`>
    final: true                        # terminal states only
    gating: true                       # human-exit states only
    visits: 2                          # counted-loop states only
    execute_on: descendant-terminal    # supervising states only: {child,descendant}-{terminal,transition}
    all_models: [<model-name>, ...]    # OR model: <name> — never both
    inputs:
      - { name: <artifact>, path: <root-relative path with {task_id}/{state}/{model}>, format: markdown }
    outputs:
      - { name: <artifact>, path: <root-relative path>, format: markdown }

transitions:
  - from: <source>
    to: <target>
    description: <when and why>
    on_leave: <callback>               # optional
    on_enter: <callback>               # optional
    condition: <expression>            # optional
    exit_code: <integer>               # optional
    timeout: <duration>                # optional

profiles:
  default: { initial: <state-name>, allowed: [<state-name>, ...] }

node_policy:
  root: default
  default: default
  by_type: { bug: <other-profile> }    # optional
```

When `models` is present, a state may omit model selectors, set `all_models`, or set `model` — never both selectors on one state.

### Artifact Contracts

`inputs:` and `outputs:` make files first-class transition prerequisites — they are execution semantics, not markdown syntax:

- `rhei next` refuses to claim a task if any `inputs:` file for the current state is missing, with an explicit missing-artifact error.
- `rhei transition` / `rhei complete` refuse to leave a state until every `outputs:` file is written.

Paths resolve relative to the plan's execution root (the directory containing the `.rhei.md` plan for a Single-File Plan; the directory containing `index.rhei.md` for a Directory Workspace). Template variables (`{task_id}`, `{state}`, `{model}`) resolve at runtime. Prefer artifact contracts over prose like "write your review to foo.md and transition" — the runtime enforces them structurally, and `instructions` can reference them via `{input.<name>.path}` / `{output.<name>.path}`.

## Design Rules

### States

1. One state per distinct workflow phase.
2. Name states after the phase, not the team (`security-review`, not `security-team`).
3. Never set `initial: true` on a state — initial lives on profiles.
4. Mark at least one state `final: true`. Include both a success terminal and a cancellation terminal unless the workflow clearly needs only one. Every terminal state carries one artifact contract you do not declare: a task does not enter it without a non-empty `runtime/results/<task-id>.md`. Under `rhei run` the worker in the state before it writes that file — agents are told the path in their prompt, programs get `RHEI_RESULT_PATH` — so a `program:` state whose exit routes straight into a terminal state must write it, or the run stalls with the result reported as a missing output.
5. Mark human gates `gating: true` and say in `instructions` that the state must not transition out autonomously. `rhei next` refuses to claim gating tasks; `rhei complete` refuses to exit them.
6. Keep the machine proportional. Past roughly 15 states, consider splitting workflows.
7. Use `execute_on` when a parent should steer its subtree *while* it runs rather than only integrate it at the end. A supervising state needs three outgoing edges, in this order: the exhaustion edge (`condition: visitCount >= visits`), the finishing edge (`condition: openDescendants < 1`), and the unconditional self-loop that releases the subtree. The finishing edge does not have to point at a `final: true` state — it may point at any state that reaches one, so a supervisor that ends with a human ruling sends it to the `gating: true` state and lets the human take the last hop. Only `cancelled` does not count as reaching one: abandoning the subtree is not finishing it. Pick the value by what the parent is meant to judge: `child-terminal` wakes it once per finished child subtree (supervision at one level of decomposition, with each child free to supervise its own), while a `*-transition` value serializes the subtree at every hop and spends one supervisor invocation per hop — choose that deliberately. **Do not declare `outputs:` on a supervising state:** a satisfied completion condition makes `rhei run` advance a ticket without spawning it, so a file the supervisor wrote on visit 1 makes every later visit a silent no-op. Declare it as an `optional: true` **input** instead — the supervisor still gets the path, and `{if input.<name>.exists}` is how a supervisor with no session tells its first visit from the rest.
8. Use `visits` only when the workflow intentionally loops through the same state and should eventually escalate or take another exit. Visit 1 renders as the unsuffixed name; later visits render as `<name>-<n>`. (Never combine `visits` with `all_models`/`all_targets` — see the Worked Pattern gotcha.)

### Transitions

1. Declare every legal transition explicitly. Unlisted transitions are forbidden — this is the core safety property.
2. Model only real workflow paths.
3. Multiple outgoing transitions are distinct outcomes; document each in `description`. Under `rhei run`, route among them with `condition` / `exit_code`, not `instructions` prose.
4. Give every non-final state a way out — a path to a `final: true` state — and keep that path inside each profile's `allowed` set too (see *Profiles*). A machine that breaks either is refused when it loads, naming the state and the edge to add.
5. Provide a cancellation path from every non-final state, usually `from: "*"` to a `cancelled` terminal. It is an escape hatch and never a state's way forward, so a state still needs an edge of its own.
6. No outgoing transitions from final states.
7. Avoid prose-only gates for machine-critical decisions. If a decision changes what the machine may do, encode it as a gating state, artifact contract, transition `condition`, `exit_code` route, or callback result the runtime can observe.

### Profiles and Node Policy

1. Start with a single default profile. Split only when kinds have genuinely different flows (e.g. `bug` skips design review).
2. Name profiles for the policy (`light-review`, `reviewed`, `simple`), not the kind they apply to.
3. Each profile's `allowed` is wholesale — profiles are referenced by name, never merged.
4. Use `overrides` only when `by_type` cannot express the rule (e.g. "leaf-level tasks skip review").

### Instructions

1. Write for the actor in that state, describing the domain work only — exit conditions are encoded structurally (`outputs:` artifacts, transition `condition` / `exit_code`), not in prose. Gating states are the exception: their instructions address a human reader and must explicitly say "do not transition out of this state autonomously," naming who decides and what decision.
2. Reference concrete artifacts and checks, not vague review language.
3. Use template variables (`{task_id}`, `{task_title}`, `{visit_count}`, `{visits}`, `{model}`) instead of prose placeholders; reference contracts via `{input.<name>.path}` / `{output.<name>.path}`. Unknown variables are left verbatim, so free-form braces in prose are safe.

### Automation, queues, and publish states

1. **Idempotency for recurring sweeps.** A state or callback that repeatedly scans for work must name the durable marker, artifact, external id, or de-duplication key that prevents reprocessing the same item next run.
2. **Queue drain condition.** Queue-processing flows need an explicit "no ready items remain" condition, terminal path, or parking gate, or `rhei run` finds the same work forever.
3. **Publish states re-check approvals.** A state that releases, publishes, merges, deploys, or notifies externally must consume the review/approval artifacts as `inputs:` and verify the latest result is still affirmative immediately before the side effect.
4. **Publish states need a negative path.** If the required artifact is missing, stale, rejected, or inconsistent, route to review/blocked/cancelled/failed instead of publishing.
5. **Keep side effects idempotent.** Publish and callback states must be safe to retry after a crash — use external ids, lock files, status artifacts, or read-before-write checks.

## Worked Pattern: Multi-Round Discussion / Deliberation

When the workflow needs several agents to **deliberate** — take each other's positions into account across rounds and then converge or escalate — rather than just hand a task state to state, start from the checked-in, `rhei validate`-passing reference `examples/agent-discussion/` (state machine `discussion-states.yaml`, driven by `workflow.sh` callbacks). Its shape:

- **A `collect ↔ judge` loop.** `collect` declares `all_models: [...]` so the position callback fans out once per participant; `judge` is a single-model synthesizer. The loop is driven by the **judge's callback `nextState` redirect**, not by `visits`: the judge returns `converged` (consensus — records the decision artifact), `escalated` (a `gating: true` human handoff once the round budget is spent), or no redirect to fall through the default `judge → collect` transition for another round. Declare `converged` and `escalated` as transitions from `judge` so the redirects are legal targets.
- **Participants take each other into account** because round 2+ reads the previous round's digest artifact, and **argue from distinct stances** assigned per participant (here, competing project goals).
- **The decision gates real work:** a downstream task declares `**Prior:**` on the discussion task, so it stays blocked until the discussion reaches a terminal decision (`converged`) — otherwise it's just chatter.

**Critical gotcha — never combine `all_models` (or `all_targets`) with `visits` on the same state.** The engine runs such a state per-target *per-visit* and spins on a `state → state-2` self-loop that never advances. Counted loops (`visits`) are for **single-target** states (see `examples/review-fix-visits`); fan-out loops must be bounded in the callback (a `CAP`) and driven by a redirect.

## Workflow

1. Read the project specification and actor structure.
2. Decide whether the default `rhei` machine suffices. If yes, do not author a custom machine.
3. List the real phases, gates, retries, handoffs, and per-state artifacts.
4. Draft state names, descriptions, and instructions.
5. Add `gating`, state `personality`, `visits`, model selectors, and `inputs` / `outputs` only where they solve a real need.
6. Draft transitions, including cancellation and recovery paths.
7. Draft profiles and `node_policy` — start with one default profile; add `by_type` / `overrides` only when kinds need different flows.
8. Add callbacks only where the workflow truly integrates with external automation.
9. Write the YAML where the plan that declares it will find it (see *File Placement*). For a template, write to `<template>/states.yaml` and ensure the plan skeleton declares the same machine name via `**States:**`.
10. Validate with the CLI when available.

## Validation Checklist

Before returning the machine, verify:

- No state carries `initial: true`; at least one state is `final: true`; no final state has outgoing transitions.
- Every transition has `from`, `to`, `description`.
- Gating states are marked `gating: true` and their instructions forbid autonomous exit.
- Machine-critical decisions live in gating states / artifacts / conditions / exit-code routes / callback results, not prose.
- If `models` is present, every `model` / `all_models` entry is declared there; no state sets both `all_models` and `model`.
- `visits`, when present, is ≥ `1`, and never appears alongside `all_models` / `all_targets`.
- `execute_on`, when present, is one of `child-terminal`, `child-transition`, `descendant-terminal`, or `descendant-transition` on an agent-bearing state that declares a self-loop transition, plus an edge conditioned on `openDescendants` that reaches a final state other than `cancelled` — directly, or through a state such as a human gate that leads to one — and either `visits` or a `visitCount` exhaustion edge.
- `inputs:` / `outputs:` paths are execution-root-relative and use template variables, not hard-coded task ids.
- Recurring sweeps define an idempotency marker; queue flows define a no-ready-items drain path.
- Publish/merge/deploy/release/notify states re-check approval artifacts via `inputs:` and have a non-publish path for missing/stale/negative results.
- State names are lowercase hyphenated `IDENTIFIER`s (names with spaces/punctuation are legal but must be backticked in markdown).
- The abandon state — if the machine has one — is named `cancelled` (or `canceled`, the same reserved name). Only that name gets cancellation semantics: a cancelled prior does not satisfy a dependency, `rhei complete` never selects it, the run report marks it apart from success, and a transition into it waives the abandoned step's declared `outputs:`. `dropped`, `abandoned`, and friends are ordinary terminal states and get none of it.
- `name` is a meaningful project-derived identifier.
- Every non-final state reaches a `final: true` state, counting a `from: "*"` edge only where its target is not final — or where the state is `gating: true`, whose every edge counts.
- `profiles` present; every profile declares `initial` and a non-empty `allowed`; `initial` ∈ `allowed`; `allowed` contains ≥1 final; reachability holds (every allowed non-final reaches a final via `to`-in-`allowed` transitions, counted the same way).
- `node_policy` has `root` and `default`, both naming defined profiles; every `by_type` key is a declared non-root kind; `rhei` is never a `by_type` key.
- No orphan states (defined but unreferenced by any profile `allowed`, transition, or override).

When the CLI is available, validate with `rhei states --state-machine <path>` (add `--json` for machine-readable output). For template-authored machines still containing `{{...}}` placeholders, do not validate the raw `<template>/states.yaml` — instantiate first, then validate the rendered workspace's state machine.

## File Placement

Rhei finds a machine only where state-machine resolution looks, and only in a file named `states.yaml` ([§FS-rhei-state-machine-writer.5](../../../../docs/functional-spec/rhei-state-machine-writer.spec.md#5-file-placement)). Place it by what it governs:

- **A single-file plan** — `states.yaml` in the plan's directory.
- **A Directory Workspace** — `states.yaml` at the workspace root.
- **A Panta project's custom default** — `states.yaml` at the project root (the directory holding `index.panta.md`), named by the manifest's `**States:**`. Every rhei that declares no `**States:**` runs under it. When an adopted project has no matching project-root file, one unique matching member-root file may supply this custom default.
- **A Panta project's explicitly declared built-in `rhei` default** — when the manifest declares `**States:** rhei`, an optional matching `states.yaml` at the project root replaces the built-in machine. Without that project-root file, the project uses built-in `pending`/`completed`; a matching file found only in a member rhei root does not replace the default, whether the member inherits or restates `rhei`.
- **An omitted effective declaration** — after Panta inheritance is applied, a plan or rhei with no effective `**States:**` uses built-in `rhei` and ignores automatically discovered project-root and member-root files. A member that inherits the manifest's explicit `**States:** rhei` remains in the preceding case; its effective declaration is not omitted.
- **One rhei's own process** — `states.yaml` at that rhei's execution root, named by the rhei's own `**States:**` when that names a machine other than the project default. Every instantiated template ships this shape. A rhei whose `**States:**` restates the default's name runs the project default and resolves it exactly as the default resolves: its own root gets no precedence. It can therefore supply a custom default only through that default's unique name-match fallback; it can never supply the built-in `rhei` project default from a member root ([§AR-rhei-panta.4](../../../../docs/architecture/rhei-panta.spec.md#4-state-machine-binding)).

The YAML's `name` must equal the `**States:**` value that selects it. No other directory or file name is searched. A machine kept anywhere else loads only when every invocation passes `--state-machine <path>`, and that flag replaces resolution for the whole scope: it works for a plan, a workspace, or a project that runs a single machine, but it cannot supply one machine among several.

Write the machine before anything points at it. In a Panta project a rhei binds to one with `rhei new "<title>" --states <name>`, and that create resolves the name at create time: with no `states.yaml` declaring it, the create is refused and rolled back, and `--keep-on-error` is what writes the declaration anyway. The order is machine first, rhei second — a rhei points at a machine, never the other way round.
