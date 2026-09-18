# FS-rhei-usage: How Rhei Is Used

This document describes how Rhei plans are consumed by agents, humans, and programs. It covers the distinct roles that interact with a plan, the coordination patterns they follow, and how the state machine governs the workflow.

For the formal grammar see the [Plan Language Specification](rhei-plan-language.spec.md). For authoring patterns see the [Usage Guide](rhei-authoring.spec.md).

## 1. Roles

A Rhei plan is a shared artifact read and written by several distinct roles. Each role has a narrow mandate — what it may read, what it may change, and when it must stop.

### 1.1. Plan Writer

The plan writer creates and restructures plans. It translates a goal into a dependency graph of tasks with states, child task nodes, and prose context.

Responsibilities:
- Decompose work into independently completable tasks.
- Assign dependencies (`**Prior:**`) to encode sequencing.
- Set each task's authored `**State:**` to the starting state of its node's resolved profile (typically `draft` or `pending`).
- Maintain the DAG invariant — no cycles, no dangling references.
- Add or reorganize context sections (`## Overview`, `## Requirements`, etc.) before `## Tasks`.

The plan writer does **not** execute tasks or advance states during implementation. Structural edits and implementation progress are separate concerns.

### 1.2. Plan Worker

The plan worker picks up an existing plan and makes progress on it. It is driven entirely by the plan: the state machine defines what transitions are legal, `**Prior:**` edges define what is ready, and state instructions define what to do.

Responsibilities:
- Claim the next eligible task via `rhei next` (all priors completed, unassigned, not terminal or gating, and all required current-state inputs present).
- Work in the current state following the state's `instructions`.
- In interactive/manual execution, advance the state when exit conditions are met (e.g., `draft` → `pending`, `pending` → `agent-review`) or finish directly with `rhei complete` when review is not required.
- In `rhei run` execution, leave state mutation to the orchestrator: the worker produces the required artifacts and exits, and `rhei run` performs the transition.
- Stop at gating states (`human-review`) and terminal states (`completed`, `cancelled`).

The plan worker does **not** add tasks, reorder tasks, change dependencies, or delete sections. In interactive/manual execution it may edit `**State:**` values and progress logs inside a task body (including child task bodies). Under `rhei run`, spawned workers leave `**State:**` changes to the orchestrator and only produce the work and artifacts for the current state.

The Plan Worker role corresponds to `worker` [Completion Authority](rhei-agents.spec.md#31-completion-authority); `rhei run` corresponds to `orchestrator` authority. The same state definition is legal under both — what differs is who decides the state's work is done and drives the resulting transition.

### 1.3. Reviewer

The reviewer inspects completed work before it advances to the next state. In the default Rhei state machine, this role appears in two forms:

- **Agent reviewer** (`agent-review`): An automated pass that checks the implementation against the task description, its child task nodes, and repository conventions (lint, format, tests). It records concrete findings in the task body or in a required findings artifact when the active state machine declares one. The next transition is then chosen by the active execution model: manually by an interactive worker, or by `rhei run` after the reviewer subprocess exits.

- **Human reviewer** (`human-review`): A human inspects the work. No agent may transition out of this state autonomously. The human decides: return to `pending` (rework), `completed` (approve), or `cancelled` (abandon).

### 1.4. Human Operator

The human operator has full authority over the plan. They can:

- Transition tasks out of `human-review`.
- Cancel tasks at any point.
- Edit the plan structure (add tasks, change dependencies) — usually by re-invoking the plan writer.
- Override any state, including terminal states, when explicitly needed,
  through the attended `rhei transition --force --reason` and explicit
  `rhei recover` contracts (§FS-rhei-transition-cmd.6, §FS-rhei-recover).

The human is the only role that can unblock `human-review` gates. This is by design: certain decisions (ship/no-ship, scope changes, external approvals) require human judgment.

## 2. Coordination Through the State Machine

The state machine is the single coordination protocol between all roles. It defines:

1. **What states exist** and what each one means (via `description` and `instructions` fields).
2. **What transitions are legal** — any transition not declared is forbidden
   to ordinary callers. A human operator may use only the narrow, attended
   missing-edge recovery of §FS-rhei-transition-cmd.6; it does not make the
   edge part of the machine or bypass a declared edge's safeguards.
3. **Who acts in each state** — the `instructions` field tells the current role what to do and when to hand off.

This means agents do not need to communicate with each other directly. They communicate through artifacts and the authoritative plan state managed by the orchestrator: one agent writes outputs for its state, `rhei run` advances the task, and the next agent reads the resulting state and artifacts.

### 2.1. State Flow (Default Machine)

```
draft --> pending --> agent-review --> completed
  |         |  \          |               ^
  v         |   +-------->+               |
cancelled   |             v               |
            |        agent-review-fix ----+
            |             |
            v             v
        human-review   cancelled
            | 
            +--> pending
            +--> completed
            +--> cancelled
```

The `pending → completed` direct transition allows agents to finish simple tasks that do not require a separate review pass.

The `human-review` state is deliberately isolated: agents can send work into it (from `pending` or `agent-review`), but only a human can transition out of it. This separation ensures that human judgment gates are never bypassed by automated workflows.

Each arrow is a declared transition. Agents follow the `instructions` field on each state to know what work must be completed before the next handoff. The transition itself is then performed either by an interactive/manual worker or by the `rhei run` orchestrator.

### 2.2. Command Surface

Commands for authoring, inspecting, and running a workflow:

| Command            | What it does                                                                    |
|--------------------|---------------------------------------------------------------------------------|
| `rhei init`        | Sets up a Panta project in a gitignored `panta/` folder (or in place with `--here`): manifest, ignore rules, agent-discovery note ([§FS-rhei-init](rhei-init.spec.md#fs-rhei-init-rhei-init)) |
| `rhei validate`    | Checks plan structure, state-machine references, and workflow constraints ([§FS-rhei-validate](rhei-validate.spec.md#fs-rhei-validate-rhei-validate)) |
| `rhei render`      | Renders a plan in the requested output format ([§FS-rhei-render](rhei-render.spec.md#fs-rhei-render-rhei-render)) |
| `rhei viz`         | Opens the workflow visualization ([§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization)) |
| `rhei intervene`   | Sends a message to a reachable running agent with `--task <id> -m "<message>"` ([§FS-rhei-viz.5](rhei-viz.spec.md#5-running-execution-view)) |
| `rhei version`     | Reports the installed version ([§FS-rhei-version](rhei-version.spec.md#fs-rhei-version-rhei-version)) |
| `rhei run`         | Drives the full plan forward under orchestrator authority (`--rhei <id>` narrows a project-scoped run) |
| `rhei next`        | Claims the next ready task for a manual worker; `--task` may advance one eligible passive edge, while `--peek` is read-only and `--rhei <id>` narrows scope |
| `rhei transition`  | Atomically changes a task's state via compare-and-swap; `--result` carries the message a `final: true` target requires, and the attended `--force --reason` form repairs a missing edge ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result), §FS-rhei-transition-cmd.6) |
| `rhei recover`     | Explicitly rolls back or forward an interrupted forced transition; every other command refuses the marked execution root (§FS-rhei-recover) |
| `rhei complete`    | Terminal transition invoked by a manual worker: the inferred one-hop terminal target plus the shared transition carrying a literal `--result` or UTF-8 `--result-file` message |
| `rhei reset`       | Returns each task to the state it was authored in ([§FS-rhei-reset.2.2](rhei-reset.spec.md#22-authored-state)), removes `runtime/`; narrowed with `--rhei <id>` it removes only the in-scope tickets' keyed output ([§FS-rhei-reset.2.1](rhei-reset.spec.md#21-narrowed-reset---rhei)) |
| `rhei snapshot`    | Lists, shows, prunes, or continues from session snapshots captured by `rhei run` |
| `rhei roster`      | Inspects the effective agents, models, bindings, defaults, sources, and merge provenance for a project ([§FS-rhei-agents.1.1.7](rhei-agents.spec.md#117-inspecting-the-effective-roster)) |

`rhei run` and the manual-worker flow (`next` / `transition` / `complete`) are mutually exclusive per execution — they never overlap on the same task because `rhei run` holds transition responsibility for the states it drives. The typical manual-worker loop is `next` (claim) → work → `transition` (advance as needed) → `complete` (finish, record result, release).

When the Codex/OpenAI adapter reports the supported reset-bearing session-limit
signal, `rhei run` parks that work until the displayed UTC deadline instead of
reporting an ordinary failure. Foreground runs remain attached while waiting;
headless launch still returns a run id and `rhei attach` shows the provider,
reporting task, and deadline. Interrupting the wait is safe: the task stays in
its authored state, and a later `rhei run` resumes from the persisted deadline.
`rhei reset` deliberately discards that wait with the rest of the task's
runtime state. §FS-rhei-run.3.3

Ticket ids in command output are project-qualified — `<rhei-id>.<task-id>`, e.g. `plan.1` for a single-file `plan.rhei.md` — and `rhei list` accepts the same `--rhei <id>` narrowing as `run`, `next`, and `reset` ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)).

#### Naming a ticket

Every command that acts on one ticket accepts it the same two ways: as the
first positional argument (`rhei complete auth.1`, `rhei release auth.1`,
`rhei transition auth.1 --from …`) or through `--task`. The positional slot on
these commands is a *ticket or plan*: an argument that names an existing path
is the plan, and an id-shaped argument that names no path is the ticket, with
the plan resolved from the working directory as usual ([§FS-rhei-complete.2.1](rhei-complete.spec.md#21-ticket-targets)).
`--task` always names the ticket, which leaves the positional free to name the
plan for a caller that wants to be explicit about both.

One shape across the family matters more than the shape chosen. These commands
are used in sequence on the same ticket — claim it, advance it, finish it, or
hand it back — and a caller that has just typed `rhei complete auth.1` has no
way to guess that `rhei release auth.1` is a path error rather than the same
gesture. `rhei next` is the exception that proves the rule: it *selects* a
ticket rather than being given one, so it has no ticket argument at all.

#### Output streams

Human-readable output, machine formats (`--json`, `--format json`), and the
rendered documents all go to stdout; diagnostics and warnings go to stderr, so
`rhei list 2>/dev/null` is a clean ticket listing even when a rhei fails to
load ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)).
`rhei roster --json` follows the same split: its one schema object is stdout,
while a deprecated-settings warning or its single JSON error-with-help object
is stderr ([§FS-rhei-agents.1.1.7](rhei-agents.spec.md#117-inspecting-the-effective-roster)).

A consumer may stop reading stdout before a command finishes writing it —
`rhei list | head`, `rhei states | grep -q`, quitting a pager. That is normal
shell usage, not a failure: the command stops writing and exits without a
panic, a stack trace, or a diagnostic, exactly as `ls | head` does. Reporting
an internal error there would make every pipeline into `head` look like a bug
in the plan.

The `rhei snapshot` family includes `list`, `show`, `gc`, and `continue`.
`rhei run --from-snapshot` is the run-time override surface for ad-hoc
snapshot debugging; its constraints are specified in
[Snapshot Operations Specification](rhei-snapshot-operations.spec.md).
A complete checked example lives in
[`examples/snapshot-continuation`](../../examples/snapshot-continuation/README.md).

## 3. Usage Patterns

### 3.1. Pattern 0: Zero-Config Agent Execution

The simplest way to use Rhei. No callbacks, no `workflow.sh`, no glue code.

1. Configure your default agent once:

   ```bash
   mkdir -p ~/.config/rhei
   echo '{"agent": "claude-code"}' > ~/.config/rhei/settings.json
   ```

2. Create a plan (manually or via `rhei-plan-writer`).

3. Run it:

   ```bash
   rhei run plan.rhei.md
   ```

Rhei spawns the configured agent for each task, composing a prompt from the state machine instructions and the task content. The agent does the work for the current state, writes any required artifacts, and exits. `rhei run` then writes any configured session snapshot side effects and performs the state transition. No scaffolding required.

For state-specific agents (e.g., a different agent or model for review):

```yaml
# states.yaml
states:
  pending:
    model: impl-fast
    agent: claude-code
    agent_mode: yolo
    agent_timeout: 30m
  agent-review:
    model: review-deep
    agent: codex
    agent_mode: safe
    agent_timeout: 20m
```

Agents are referenced by string id only — `agent:` is resolved against the
merged `agents` registry (built-ins + `~/.config/rhei/settings.json` +
`<plan-root>/.agent-grounds/rhei/settings.json`). To use a CLI that isn't a built-in, add
it to the registry instead of inlining it on the state. Run-time overrides
are available via `--agent <AGENT>`, `--agent-mode <MODE>`, and `--model
<MODEL>` on `rhei run`.

For example, an operator can rerun an ordinary state target on another agent
and model without editing the machine:

```yaml
states:
  review:
    target: state-agent[yolo]:state-provider:state-model
```

```bash
rhei run plan.rhei.md --agent override-agent --model override-model
```

The effective target is
`override-agent[yolo]:state-provider:override-model`: the run flags replace
only the agent and model, preserving the authored mode and provider. A target
without a mode remains mode-less. This composition applies to ordinary state
and task targets; `all_targets` fanout remains selector-owned and ignores both
identity flags. [§FS-rhei-agents.1.5](rhei-agents.spec.md#15-partial-overrides)

For parallel execution of independent tasks:

```bash
rhei run plan.rhei.md --parallel 4
```

See [Agents Specification](rhei-agents.spec.md) for configuration, resolution order, timeout handling, and log capture.

### 3.2. Pattern 1: Single Agent, Start to Finish

One agent session acts as both writer and worker. This is useful when the agent session itself drives the workflow rather than `rhei run`.

1. User describes the goal.
2. Agent invokes the plan writer to produce a `.rhei.md` file.
3. Agent invokes the plan worker on the same file.
4. Worker iterates through tasks, advancing states, implementing, and logging progress.
5. Worker stops when all tasks are `completed` or blocked on `human-review`.

This is the default for small, well-scoped work within a single agent session. The plan still provides value: it gives the human a structured view of progress, and the agent a memory of what it has and hasn't done. For larger work or multi-agent workflows, Pattern 0 (`rhei run`) is preferred.

### 3.3. Pattern 2: Writer and Worker as Separate Sessions

For larger work, the plan writer and plan worker run in separate agent sessions — possibly days apart.

1. **Session 1 (planning):** Human describes a project. Agent produces a plan with tasks in `draft` or `pending`.
2. **Human review:** Human reads the plan, adjusts scope, promotes drafts to `pending`, reorders priorities.
3. **Session 2 (execution):** A new agent session invokes the plan worker. It reads the plan fresh, selects the first eligible task, and begins work.
4. **Subsequent sessions:** If the worker stops (human gate, end of session, ambiguity), a new session picks up where it left off by re-reading the plan.

The plan file is the handoff mechanism. No session state needs to survive between sessions — the plan captures everything.

### 3.4. Pattern 3: Parallel Workers on Independent Branches

When a plan's DAG has independent branches (tasks with no shared dependencies), multiple workers can operate in parallel.

```markdown
### Task 1: Set up database schema
**State:** pending

### Task 2: Build API endpoints
**State:** pending
**Prior:** Task 1

### Task 3: Write frontend components
**State:** pending

### Task 4: Integration tests
**State:** pending
**Prior:** Task 2, Task 3
```

Here, Task 1 and Task 3 have no dependency relationship. Two workers can implement them concurrently. Task 4 remains blocked until both Task 2 and Task 3 are completed.

For manual workers, coordination happens through `rhei next`, `rhei transition`, and `rhei complete`. A worker claims a task, implements it, and then completes it:

```bash
# Inspect what is next without claiming (read-only, safe for PM browsing)
rhei next --peek plan.rhei.md

# Claim the next automatically selected task (assigns it and may advance one passive initial edge)
rhei next plan.rhei.md

# Explicitly claim ready non-initial work (may advance one eligible passive edge)
rhei next plan.rhei.md --task 3

# ... agent does the work ...

# Complete: transition to terminal state, write result file, release assignment
rhei complete plan.rhei.md --task 3 --result "Schema migration applied successfully"

# Prefer a file (or `--result-file -` for stdin) for multiline or Markdown-heavy results
rhei complete plan.rhei.md --task 3 --result-file result.md
```

The file/stdin form keeps backticks, quotes, and line endings out of a
shell-expanded argument. It carries the loaded text through the same terminal
transition and result-artifact path as the inline form
([§FS-rhei-complete.2.2](rhei-complete.spec.md#22-result-sources)).

An auto-advancing claim commits only when the new state, resolved assignee, and
transition entry are all durable ([§FS-rhei-next.3.1](rhei-next.spec.md#31-behavior)). An ordinary
failure before that boundary restores the original task and can be retried
after its cause is removed, without `rhei reset`. A diagnostic that says
restoration itself failed instead names the affected paths and does not promise
retryability. Failures after commitment, including prompt-rendering failures,
leave the durable claim in place, and callback effects outside Rhei's files are
not undone.

For finer-grained control, workers can use `rhei transition` directly with a compare-and-swap:

```bash
rhei transition plan.rhei.md --task 3 --from pending --to completed
```

The `--from` flag is the key: the command acquires a file lock, verifies the task is still in the expected state, and only then writes the new state. If another worker already claimed the task, the command fails with a conflict error — the losing worker re-reads the plan and picks a different task.

This eliminates last-write-wins races without requiring an external scheduler. The plan file plus its lock file are the only coordination primitives needed.

For `rhei run --parallel N`, the same concurrency rule applies, but the spawned agents are not the ones issuing transition commands. Each spawned agent works only on the current state of its assigned task and exits. `rhei run` serializes the resulting state changes, which keeps transition authority centralized even when task work is distributed across multiple agents.

### 3.5. Pattern 3b: Highly Distributed Swarms (Directory Workspaces)

If parallel workers are distributed across multiple branches or machines, the single-file lock approach breaks down (leading to Git merge conflicts). In these highly concurrent scenarios, agents use **Directory Workspaces**.

Instead of a single `plan.rhei.md`, the plan is hosted as a directory with tasks separated into a `tasks/` directory (`tasks/db-schema.md`, `tasks/integration.md`). 
Because tasks are isolated in distinct files, Git effortlessly merges cross-branch progress without text collisions, mirroring the resilience of database-backed trackers like Beads.

### 3.6. Pattern 4: Human-in-the-Loop Checkpoints

Plans that require human approval use the `human-review` state as a gate.

```markdown
### Task 3: Deploy to production
**State:** human-review
**Prior:** Task 2

Review: All tests pass. Staging smoke tests succeeded. Ready for production deploy.
```

When a task reaches `human-review`:
- All agents stop work on that task.
- The human inspects the implementation, reads review notes, and decides.
- The human transitions the task to `pending` (rework), `completed` (approve), or `cancelled`.

Meanwhile, agents continue working on other branches of the DAG that are not blocked by this gate. The plan stays productive even when one branch is waiting on human input.

### 3.7. Pattern 5: Draft Expansion

For exploratory or long-running projects, tasks start as `draft` — placeholder titles that are not yet ready for execution.

1. Plan writer creates tasks in `draft` with rough titles but minimal descriptions.
2. When all prior tasks are completed, `rhei next` claims the draft task (sets `**Assignee:**` without changing its state).
3. The agent works in `draft`: it analyzes the current state of the project, determines the most elegant approach, and writes a concrete task description.
4. In interactive/manual execution, the agent transitions `draft` → `pending` via `rhei transition`. Under `rhei run`, the spawned agent exits after writing the concrete task description and any required draft artifacts, and `rhei run` performs the `draft` → `pending` transition.
5. Work then continues in `pending`, either in the same manual session or through the next `rhei run` pass.

This prevents agents from planning against stale or incomplete project state. The `draft` state is a signal: "this task exists in the plan but needs analysis before it can be specified and executed." Because `rhei next` claims without transitioning, the agent has a dedicated phase to do the analysis work before advancing the state.

### 3.8. Pattern 6: Programmatic State Transitions

Beyond agent-driven workflows, Rhei plans can be advanced programmatically through the transition callback system. State machines declare `on_leave` and `on_enter` callbacks that fire during transitions — they can approve, reject, or redirect transitions, turning the plan into an executable workflow engine.

Callbacks are supported for bash (via `--handlers`), TypeScript/JavaScript (NAPI), Python (PyO3), and Java (JNI). See [Transitions Specification](rhei-transitions.spec.md) for the formal `TransitionContext` / `TransitionResult` API and [Transition Callback Examples](rhei-callbacks.spec.md) for per-language code.

### 3.9. Pattern 7: Multi-Target Analysis with `all_targets`

When the same task should be run once per model and then synthesized, prefer one shared task state with `all_targets` over cloning one state or one task per model. `rhei run` executes the state once per listed target; artifact paths and prompts template on `{target}`, `{target.slug}`, `{agent}`, `{agent.mode}`, and `{model}`. A downstream `summarize` state then consumes the per-target artifacts and writes one final result.

See [States Specification — `all_targets`](rhei-states.spec.md#12-per-state-fields) for the schema, selector grammar, and a worked example.

### 3.10. Pattern 8: Living Workspace Expansion

A directory workspace can stay intentionally incomplete at authoring time and be
expanded by the orchestrator while `rhei run` is executing. This is useful when
follow-up work should only exist after a concrete artifact is produced.

One example is a multi-agent review loop:

1. A seed review task fans out into one agent run per configured model and writes per-model findings files.
2. After each spawned reviewer exits successfully, `rhei run` advances the task according to the declared transitions and records the consolidated result artifact for the review pass.
3. A coordinator step appends one verification task per consolidated review point.
4. Each verification task is then executed by its assigned agent. The agent records whether the issue is reproducible and relevant, writes its result artifact, and exits.
5. `rhei run` advances each verification task after the subprocess exits.
6. Only relevant findings cause new fix tasks to be appended to the workspace.

The workspace starts small:

```markdown
# Rhei: Living Review Loop
**States:** living-review-loop

## Overview
The orchestrator expands this workspace as review artifacts arrive.
```

```markdown
### Task review-seed: Run multi-model review and seed the living workspace
**State:** review

Write the review findings for this pass. The orchestrator will append
verification tasks after the review pass completes.
```

After `review-seed` completes, the orchestrator may append task files such as:

```markdown
### Task verify-cache-key: Verify and reproduce finding F-001
**State:** prove
**Prior:** Task review-seed
```

And after verification, only the relevant issues become fix tasks:

```markdown
### Task fix-cache-key: Fix finding F-001 after verified reproduction
**State:** prove
**Prior:** Task verify-cache-key
```

This keeps the Rhei truthful: speculative fixes do not appear in the task graph
until the review artifact justifies them. The checked-in example lives in
[`examples/living-review-loop`](../../examples/living-review-loop/README.md).

### 3.11. Pattern 9: Program States for Deterministic Steps

Program states execute a fixed command instead of spawning an AI agent — use them for deterministic workflow steps (builds, tests, linting, deployment) where an agent would add latency and cost without value. Programs communicate outcomes through exit codes, so transitions from program states can declare an `exit_code` condition that routes automatically. Program and agent states coexist in the same state machine: a common pattern is an agent-program feedback loop where the agent implements code, a deterministic build/test program verifies, and failures return control to the agent.

See [Program States Specification](rhei-programs.spec.md) for program declaration, the full exit-code evaluation algorithm, timeout handling, and validation rules.

### 3.12. Pattern 10: CI/CD Pipeline as a Plan

A Rhei plan can model a CI/CD pipeline where each task is a pipeline stage. The state machine encodes the pipeline's control flow, and callbacks integrate with build systems, test runners, and deployment tools.

```markdown
# Rhei: Release Pipeline
**States:** ci-pipeline

## Tasks

### Task 1: Lint and type-check
**State:** pending

### Task 2: Unit tests
**State:** pending
**Prior:** Task 1

### Task 3: Integration tests
**State:** pending
**Prior:** Task 2

### Task 4: Deploy to staging
**State:** pending
**Prior:** Task 3

### Task 5: Production deploy
**State:** draft
**Prior:** Task 4
```

Task 5 starts as `draft` — it will only be promoted to `pending` after staging is verified. The pipeline advances automatically through callbacks, but human gates can be inserted at any stage.

### 3.13. Pattern 11: A program creates follow-on work for its run

An unrestricted Panta run may grow by complete members while it is live. A
program state can finish by instantiating a template directly into its project,
and the enclosing run admits the published member at its next scheduling
checkpoint:

```yaml
states:
  prepare-follow-on:
    program:
      command:
        - rhei
        - instantiate
        - release-review
        - release=v2.0
        - --output
        - panta/release-v2-review
```

The program deliberately omits `--execute`: publication hands the new member
to the existing project-wide loop and recorded run. `--execute` instead asks
for a separate run and remains unsuitable inside a project run that already
holds the project lock. The follow-on keeps its own declared state machine,
execution root, logs, results, snapshots, and accounting. An explicit
`rhei run --rhei seed` does not widen; use an unrestricted project run when
the workflow is intended to compose new members dynamically
([§FS-rhei-run.2.5](rhei-run.spec.md#25-project-scope---rhei), [§FS-rhei-panta.6.2](rhei-panta.spec.md#62-rhei-run)).

## 4. The Plan as Shared Memory

The central design principle: **the plan file is the single source of truth**. Agents do not maintain internal state about what has been done or what comes next. They read the plan, act on it, write back to it, and validate. This has several consequences:

- **Resumability.** A new agent session can pick up any plan mid-execution. No context beyond the file is needed.
- **Auditability.** The plan's git history shows every state transition, every progress log entry, and who (or what) made each change.
- **Human legibility.** The plan is standard markdown. Humans read it directly — no dashboards, no query languages, no special tooling required.
- **Reconstitution.** Under `rhei run` every invocation starts cold, so the
  prompt rebuilds this memory from the plan and `runtime/` by a fixed
  algorithm — position, history, the task's own past, and a map of the rest
  ([§FS-rhei-memory](rhei-memory.spec.md#fs-rhei-memory-mid-term-memory)).
- **Composability.** Different agents (writer, worker, reviewer) interact with the same file through well-defined, non-overlapping edits. The state machine prevents conflicts by making illegal transitions impossible.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — formal grammar and semantic constraints
- [Plan Language Usage Guide](rhei-authoring.spec.md) — authoring patterns and walkthroughs
- [States Specification](rhei-states.spec.md) — state machine format and default states
- [Agents Specification](rhei-agents.spec.md) — agent configuration, invocation, timeout, and log capture
- [Program States Specification](rhei-programs.spec.md) — deterministic program execution, exit-code transitions
- [Transitions Specification](rhei-transitions.spec.md) — formal state transition system, callbacks, and YAML schema
- [Transition Callback Examples](rhei-callbacks.spec.md) — callback implementations across languages
- [Next Command](rhei-next.spec.md) — `rhei next` behavioral contract
- [Transition Command](rhei-transition-cmd.spec.md) — `rhei transition` behavioral contract
- [Complete Command](rhei-complete.spec.md) — `rhei complete` behavioral contract
- [Run Command](rhei-run.spec.md) — `rhei run` behavioral contract
- [Reset Command](rhei-reset.spec.md) — `rhei reset` behavioral contract
- [State Machine Writer](rhei-state-machine-writer.spec.md) — designing custom state machines from project specs and teams
- [Install Skills](rhei-install-skills.spec.md) — `rhei install-skills` command for agent integration
