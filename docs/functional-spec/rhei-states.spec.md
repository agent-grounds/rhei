# FS-rhei-states: Rhei States Specification

This document defines the default states configuration for tasks in the Rhei
agent runtime. The authoritative machine-readable form lives in
[states.yaml](states.yaml); the writer-skill mirror is
[default-states.md](../../crates/rhei-cli/skills/rhei-plan-writer/references/default-states.md).

The built-in `rhei` machine is intentionally minimal. Every node starts in
`pending`, where the only instruction is "Do the task." The only built-in
transition is `pending` -> `completed`, and `completed` is the only terminal
state. Projects that need drafting, review, cancellation, human gates, retries,
artifact contracts, or richer routing should declare a custom state machine.

The state-machine surface also permits these optional fields for richer workflows:

- Per-state `personality: <string>` to inject role framing into `rhei next` for that specific state (supports template variables)
- Template variables in `instructions` and `personality` fields, resolved by `rhei next` at output time
- A sibling `prompt_templates/` directory to define reusable Markdown prompt files, and per-state `prompt_template:` references with concrete placeholder values
- Top-level `models: [<model-id>, ...]` to declare the model profile identifiers available to the machine
- Per-state `target: <selector>` to bind a state to one inline execution target
- Per-state `all_targets: [<selector>, ...]` to fan a state out across multiple execution targets
- Per-state `all_models: [<model-id>, ...]` to declare the full model set that may execute that state
- Per-state `model: <model-id>` to bind a state to exactly one declared model profile
- Per-state `agent: <agent-id>` to bind a state to a specific coding agent CLI
- Per-state `agent_timeout: <duration>` to set the maximum time an agent may work in this state
- Per-state `program: <string|object>` to bind a state to a deterministic program command (mutually exclusive with `agent`)
- Per-state `program_timeout: <duration>` to set the maximum time a program may run in this state
- Per-state `visits: <integer>` to cap total counted visits for that state
- Per-state `inputs:` / `outputs:` artifact contracts to require workspace files on entry/exit; individual inputs may be marked `optional: true` to skip the existence check while still exposing the path and an existence flag to agents and programs
- Per-state `handoff:` inheritance to inject declared same-task handoff artifacts from the previous state into the successor prompt
- Per-state `mcp_servers:` and `skills:` lists to attach MCP servers and agent skills to the agent subprocess for that state; individual entries may be marked `optional: true` to warn-and-continue rather than block when the tool is unavailable
- Per-state `execute_on: <scope>-<event>` to make a non-leaf task in that state a *supervisor* of its subtree, woken at every finished child, every child transition, every finished descendant, or every descendant transition, and holding the subtree in between ([§FS-rhei-supervision](rhei-supervision.spec.md#fs-rhei-supervision-subtree-supervision-specification))

When `models` is omitted, the machine behaves as it does today and states are
not model-constrained. For new workflows, prefer `target` / `all_targets`
because they encode the full execution identity: agent, optional mode, optional
provider, and model. The older `model` / `all_models` fields remain supported
as compatibility forms for model-centric workflows. `visits` is orthogonal to
both forms and may be combined with either single-target or fanout execution.

The `model` field is the semantic execution identity for the state. It resolves
through settings to a provider/model combo and is available to callbacks,
templates, logs, and multi-model execution. When `agent` is set on a state,
`rhei run` uses that agent transport to execute work in the state instead of
relying on callbacks alone. When `agent` is omitted, callbacks still resolve the
model; autonomous agent execution falls back to settings defaults and then the
model profile's `default_agent`. See [Agents Specification](rhei-agents.spec.md)
for configuration, resolution order, and invocation details.

An execution target selector is an inline shorthand for the full execution
identity. It uses one of these forms:

- `<agent>:<model>`
- `<agent>[<mode>]:<model>`
- `<agent>:<provider>:<model>`
- `<agent>[<mode>]:<provider>:<model>`

Examples:

- `claude-code:claude-opus-4-7`
- `claude-code[yolo]:anthropic:claude-opus-4-7`
- `gemini[yolo]:google:gemini-3.1-pro-preview`
- `codex[safe]:openai:gpt-5-codex`

## 1. Schema Additions

### 1.1. Top-level fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `models` | string array | No | The complete set of model profile identifiers available to the machine |
| `profiles` | map of name to `{initial, allowed}` | Yes | Named, reusable state profiles. Each profile declares the `initial` state and the `allowed` state subset for any node assigned to it. Referenced by `node_policy`. |
| `node_policy` | object | Yes | Maps nodes to profiles. Must define `root` (the Panta project root) and `default`. Optionally defines `rhei` (the rhei tier), `by_type`, and `overrides`. See [Node Policy](#9-node-policy). |

The `profiles` and `node_policy` blocks replace the earlier per-state
`initial: true` boolean. A state definition no longer carries its own initial
flag; the initial state is a property of each profile, so different node kinds
can start in different states within the same state machine.

### 1.2. Per-state fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `prompt_template` | string or object | No | Reusable prompt selected from sibling `prompt_templates/<id>.md`. String form is the template id. Object form is `{name, values}` where `values` supplies concrete scalar values for placeholders in that template. |
| `personality` | string | No | State-specific role framing printed by `rhei next` for that state |
| `role` | string | No | `cancellation` classifies abandonment independently of spelling; requires `final: true`. Omitted roles preserve bare-name inference. §1.4 |
| `gating` | boolean | No | When `true`, autonomous commands (`rhei next`, `rhei complete`, engine-triggered transitions) must not transition out of this state. Only explicit human-initiated transitions are allowed. |
| `concurrent` | boolean | No | When `true`, `rhei run` may work multiple ready tasks in this state simultaneously (up to `--parallel`). When `false` (the default), at most one ready task per pass is scheduled for this state and the rest are deferred to a later pass. This is a scheduling hint only — state entry, exit, and transition semantics are unchanged. Fanout invocations from a single task (`all_targets` / `all_models`) are not affected by this flag. |
| `poll` | object | No | Marks this state as a time-triggered *polling* state. Contains `interval` (duration string, e.g. `5m`) and `max_attempts` (integer ≥ 1). On each attempt the state's `agent` or `program` runs once and the engine evaluates transitions normally; a self-loop (`from: X, to: X`) is interpreted as "not done yet, retry after `interval`". Between attempts the `--parallel` slot is released and the task is not ready again until the interval elapses. After `max_attempts` attempts the engine will not take a self-loop and instead selects a matching exhaustion transition (typically `condition: pollAttempts >= pollMaxAttempts`); if none matches, the task fails. May also carry `waiting_on` (a short label naming the person or role the poll waits for), which declares the wait as a *person* wait rather than machine backoff. Mutually exclusive with `visits`. See [Polling States](#2-polling-states) below and [Run Specification — Polling States](rhei-run.spec.md#51-polling-states). |
| `visits` | integer | No | Maximum number of visits permitted for this state before the workflow must take a non-loop exit |
| `execute_on` | enum | No | Marks this state as a *supervising* state: `child-terminal`, `child-transition`, `descendant-terminal`, or `descendant-transition`. A non-leaf task in it is woken at checkpoints of its subtree — the scope picks whose moves it hears (its direct children, or any descendant), the event picks which (a terminal entry, or every transition) — and holds the subtree between visits. Requires a self-loop transition as the release edge. See [Supervision Specification](rhei-supervision.spec.md). |
| `target` | string | No | Inline execution target selector for one run of the state. Preferred over the legacy `model` + `agent` split for new workflows. A task may override this per work item with `**Model:**` or `**Target:**` unless `target_locked` is set; see [Plan Language — Task Execution Overrides](rhei-plan-language.spec.md#311-task-execution-overrides). |
| `all_targets` | string array | No | Inline execution target selectors for fanout execution. The state runs once per listed selector. Preferred over `all_models` for new multi-target workflows. |
| `target_locked` | boolean | No | When `true`, the state's execution identity is essential to the phase and must not be reassigned per task: any task-level `**Model:**` or `**Target:**` override ([§FS-rhei-plan-language.3.11](rhei-plan-language.spec.md#311-task-execution-overrides)) on a task entering this state is a validation error. Defaults to `false`. |
| `all_models` | string array | No | The complete set of declared model profile identifiers allowed to work this state |
| `snapshot` | object | No | Per-state session snapshot emit/inherit contract. Optional; details and closed-schema validation live in [Snapshots Specification](rhei-snapshots.spec.md). |
| `session` | enum | No | Session behavior for repeated visits: `cold` (the default) or `continue`. Continuation preloads this state's immediately preceding visit when possible and otherwise runs cold; see §FS-rhei-snapshots.4.7. |
| `model` | string | No | A single model profile identifier from the machine-level `models` list |
| `agent` | string | No | The coding agent CLI that executes work in this state. Must be an agent id resolved against the merged `agents` registry (built-ins → global → project `settings.json`). Inline agent objects are not permitted — define custom agents in the `agents` registry. See [Agents Specification](rhei-agents.spec.md). |
| `agent_mode` | string | No | Named flag set applied to the resolved agent for this state. Must match a key in the resolved agent's `modes` map. See [Agents Specification — Modes](rhei-agents.spec.md#22-modes). |
| `effort` | string | No | Agent reasoning effort, independent of execution target, model, agent, and permission/configuration mode. One of `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, or `max`. See [Agent Field](#5-agent-field). |
| `agent_timeout` | string | No | Maximum time an agent may work in this state before being killed (e.g., `30m`, `1h`). See [Agents Specification — Timeout Handling](rhei-agents.spec.md#7-timeout-handling). |
| `attempts` | integer | No | How many times **one visit** to this state may be spawned before `rhei run` halts the ticket. Distinct from `visits`, which bounds how many times the ticket may *enter* the state. Defaults to `2` — the invocation plus one informed retry. See [Agents Specification — Attempt Budget](rhei-agents.spec.md#323-attempt-budget). |
| `program` | string or object | No | The program command to execute in this state. String form runs via shell. Object form specifies `command`, `env`, `working_directory`, and `shell`. Mutually exclusive with `agent`. See [Program States Specification](rhei-programs.spec.md). |
| `program_timeout` | string | No | Maximum time the program may run before being killed (e.g., `10m`, `1h`). Same duration format and timeout handling as `agent_timeout`. See [Program States Specification](rhei-programs.spec.md#4-timeout-handling). |
| `inputs` | artifact array | No | Artifacts that must exist before the task can enter this state. Individual entries may be marked `optional: true` to skip the existence check. |
| `outputs` | artifact array | No | Required artifacts that must exist before the task can leave this state |
| `handoff` | object | No | Prompt-context inheritance for same-task state handoffs. See [State Handoffs](#32-state-handoffs). |
| `mcp_servers` | array | No | MCP servers attached to the agent subprocess for this state. Entries are ids from the `mcp_servers` settings registry or inline server definitions. Individual entries may be marked `optional: true`. See [MCP Servers and Skills](#7-mcp-servers-and-skills). |
| `skills` | array | No | Agent skills enabled for this state. Entries are ids from the `skills` settings registry or inline skill definitions. Individual entries may be marked `optional: true`. See [MCP Servers and Skills](#7-mcp-servers-and-skills). |

There is deliberately no field for "this terminal state requires a written
result". Every `final: true` state requires one, always, and the contract is
implicit rather than declared: see [Terminal Result](#33-terminal-result).

### 1.3. Validation Rules

- `profiles` must be present and non-empty. Each entry must declare `initial`
  (a state name) and `allowed` (a list of state names). See
  [Profiles](#8-profiles) for per-profile validation.
- `states.yaml` must not declare a top-level `prompt_templates` block.
  Reusable prompt definitions live as Markdown files in sibling
  `prompt_templates/`.
- When `prompt_templates/` is present, every `.md` file directly inside that
  directory declares one reusable prompt template. The template id is the file
  stem, so `prompt_templates/artifact-review.md` declares
  `artifact-review`. Each prompt file must contain non-empty Markdown text.
- A state's `prompt_template` must reference a prompt template declared in
  sibling `prompt_templates/`. Object form must declare a non-empty `name`,
  and every `values` entry must use a non-empty identifier key and a scalar YAML
  value. Arrays and objects are rejected as prompt-template values.
- Prompt-template placeholders use identifier-shaped `{placeholder}` syntax.
  Every placeholder in the reusable prompt text must be supplied in the state's
  `prompt_template.values` map. Runtime variables are used by assigning them as
  values, for example `task_id: "{task_id}"`. Non-identifier brace content such
  as JSON examples is literal.
- `node_policy.root` and `node_policy.default` are required and must name
  defined profiles. `node_policy.by_type`, when present, maps each declared
  non-root node kind to a defined profile. `node_policy.overrides`, when
  present, is an ordered list of `{match, profile}` entries. See
  [Node Policy](#9-node-policy) for resolution and validation rules.
- A state definition must not declare `initial: true`. The initial state is
  a property of each profile, not of the state itself.
- `state.target`, when present, must be a non-empty string matching one of:
  `<agent>:<model>`, `<agent>[<mode>]:<model>`,
  `<agent>:<provider>:<model>`, or
  `<agent>[<mode>]:<provider>:<model>`.
- `state.all_targets`, when present, must be a non-empty list of unique
  selectors following the same grammar as `state.target`.
- The normalized `target.slug` values produced by a single fanout state
  (`state.all_targets` or legacy `state.all_models`) must be unique within
  that state's fanout set for any snapshot-capable agent invocation;
  collisions are validation errors. The same target slug may appear in
  different tasks, states, snapshot names, or visits because those fields are
  part of the snapshot storage identity. See
  [Snapshots Specification — Target Slug](rhei-snapshots.spec.md#71-target-slug).
- `state.target_locked`, when present, must be a boolean. When `true`, a task
  entering this state must not carry a `**Model:**` or `**Target:**` override
  ([§FS-rhei-plan-language.3.11](rhei-plan-language.spec.md#311-task-execution-overrides)); such a combination is a validation error.
- `state.target` and `state.all_targets` are mutually exclusive.
- `state.target` and `state.all_targets` must not be combined with any of
  `state.model`, `state.all_models`, `state.agent`, or `state.agent_mode`.
- In a target selector, `<agent>` must resolve against the merged `agents`
  registry at validation time.
- In a target selector, `<mode>`, when present, must be a non-empty string and
  must match a key in the resolved agent's `modes` map when that map exists.
- In a target selector, `<provider>`, when present, must be a non-empty string.
- In a target selector, `<model>` must be a non-empty string.
- `models`, when present, must be a list of unique non-empty strings naming model profiles defined in settings.
- `state.model`, when present, must match an entry from the machine-level `models` list.
- `state.all_models`, when present, must be a list of unique non-empty strings drawn from the machine-level `models` list.
- A state must not declare both `all_models: [..]` and `model: <name>`.
- `state.all_models: []` is treated the same as omitting the field.
- When a state with `all_models` participates in snapshots, each resolved
  model invocation receives its own effective target tuple and `target.slug`;
  snapshot selectors treat it the same as `all_targets`.
- `state.visits`, when present, must be an integer greater than or equal to `1`.
- `state.agent`, when present, must be a non-empty string id. Object-valued `agent:` entries are rejected: define the custom agent in the `agents` registry in `settings.json` and reference it by id. The id must resolve against the merged `agents` registry at run time (built-ins → global → project).
- `state.agent` on a `final: true` state is a validation error (terminal states have no work to execute).
- `state.agent` on a `gating: true` state is a validation warning (gating states are human-only; the agent will never be invoked by `rhei run`).
- `state.agent_mode`, when present, must be a non-empty string and requires `state.agent` to be set. The mode name must match a key in the resolved agent's `modes` map, or the agent must declare no modes. See [Agents Specification — Mode Resolution Order](rhei-agents.spec.md#141-mode-resolution-order).
- `state.effort`, when present, must be the string `off`, `minimal`, `low`,
  `medium`, `high`, `xhigh`, or `max`. Type and vocabulary errors are rejected
  while the state machine loads. It is valid only on a state eligible for
  autonomous agent work, including one whose agent is selected from settings;
  it is rejected on `program` and `gating: true` states.
- A valid effort is checked against the effective agent profile after agent
  resolution and before scheduling or spawn. A profile without an `effort`
  mapping ignores it. A profile with a mapping must declare the selected
  canonical value or validation fails. Static selections are checked during
  plan validation; dynamically changed selections are checked before they are
  scheduled. Remote model-specific restrictions remain the native agent's
  responsibility.
- `state.agent_timeout`, when present, must be a valid duration string (e.g., `30s`, `5m`, `1h`, `2h30m`).
- `state.attempts`, when present, must be a positive integer. A value below `1` is raised to `1`: a visit always gets the invocation that makes it a visit.
- A state must not declare both `agent` and `program`.
- `state.program`, when present, must be a non-empty string or a valid program object with at least a `command` field. See [Program States Specification](rhei-programs.spec.md).
- `state.program` on a `final: true` state is a validation error (terminal states have no work to execute).
- `state.program` on a `gating: true` state is a validation error (gating states require human action; programs execute autonomously).
- `state.program_timeout`, when present, must be a valid duration string (e.g., `30s`, `5m`, `1h`, `2h30m`).
- `state.inputs` / `state.outputs`, when present, must be arrays of unique artifact definitions keyed by `name`.
- Artifact `path` values must be relative to the plan's execution root (the plan-file directory for a single-file plan, or the workspace root for a directory workspace; see [main spec — State Artifact Contracts](rhei-plan-language.spec.md#310-state-artifact-contracts)) and must not escape that root after template expansion.
- `artifact.optional`, when present, must be a boolean. Only valid on `inputs` entries; declaring `optional: true` on an `outputs` entry is a validation error (required outputs are always enforced).
- An `optional: true` input that is missing does not block state entry. Its `{input.<name>.exists}` variable resolves to `false` and its `{input.<name>.path}` resolves to the declared path regardless.
- `state.mcp_servers` / `state.skills`, when present, must be arrays. Each entry is either a non-empty string (registry id) or an object with at least an `id` field plus the inline definition fields accepted by the corresponding settings registry.
- Every string id and every `id` field in an inline entry must be unique within the state's list for that kind.
- Every registry id in `state.mcp_servers` must resolve in the merged `mcp_servers` settings registry. Every id in `state.skills` must resolve in the merged `skills` registry. Inline entries do not require a registry match.
- An `mcp_servers` or `skills` entry may declare `optional: true` (default `false`). When `optional: true`, a failure to start the server or locate the skill at spawn time does not block the agent; when `false`, it does. See [Agents Specification — Missing Tooling](rhei-agents.spec.md#6-missing-tooling).
- `state.mcp_servers` and `state.skills` on a `gating: true` state are a validation error (gating states are human-only; the agent will never be invoked).
- `state.mcp_servers` and `state.skills` on a state with `program:` set are a validation error (programs execute deterministically and do not consume tool surfaces).
- `state.mcp_servers: []` and `state.skills: []` are valid and mean "clear the inherited `defaults` tooling for this state" — not "ignore the field".
- `state.poll`, when present, must be an object with `interval` (a valid duration string, e.g. `30s`, `5m`, `1h`) and `max_attempts` (an integer ≥ `1`).
- `state.poll.waiting_on`, when present, must be a string that is not empty after trimming. It is optional; a blank value is a validation error rather than an absent field, because a poll that says it waits on someone must say on whom.
- `state.poll` on a `final: true` state is a validation error (terminal states have no work to execute).
- `state.poll` on a `gating: true` state is a validation error (gating states require human action; polling executes autonomously).
- `state.poll` combined with `state.visits` is a validation error. `poll.max_attempts` replaces the `visits` cap for the poll state and populates the same `stateVisits` counter.
- A state that declares `poll` must have at least one self-loop transition (`from: <state>, to: <state>`); without it the "retry" branch is unreachable.
- A non-poll state the machine declares a self-loop from is warned about when
  it declares neither `visits` nor a transition whose `condition` is bounded by
  `visitCount`: the loop has no budget and no exit that counts, so a run may
  re-enter it forever. Visits of such a state are counted regardless, so an
  authored `visitCount` exit works ([§FS-rhei-supervision.4.2](rhei-supervision.spec.md#42-self-loops-on-agent-states)).
- `state.execute_on`, when present, must be one of `child-terminal`, `child-transition`, `descendant-terminal`, or `descendant-transition`, and the state must be agent-bearing. `execute_on` on a `final: true`, `gating: true`, `program:`, or `poll:` state is a validation error — a state has one trigger, `poll:` (time) or `execute_on:` (its subtree) — as is combining it with `all_targets` or `all_models`. A supervising state must declare a self-loop transition — its release edge. Warnings and the full rule set are in [§FS-rhei-supervision.1.2](rhei-supervision.spec.md#12-validation-rules).
- `state.session`, when present, must be `cold` or `continue`; omission means
  `cold`. Authoring either value is legal only on a non-final, non-gating,
  non-program, non-poll agent state that declares a self-loop and whose
  execution target resolves to an effective `(agent, mode?, provider, model)`
  tuple. The legality rules apply to explicit `cold` as well as `continue` so
  an authored field never becomes a misleading no-op. `continue` must not be
  combined with `snapshot.inherit`; `snapshot.emit` may coexist. The full
  runtime contract is §FS-rhei-snapshots.4.7.
- Every non-final state must be able to reach some `final: true` state. A state
  is left by an edge whoever moves the task can take, which is what decides
  whether an edge counts here: every `from: <state>` edge counts, a `from: "*"`
  edge counts only when its target is not `final: true`, and out of a
  `gating: true` state every edge counts, wildcards included
  ([§FS-rhei-transitions.4.6](rhei-transitions.spec.md#46-wildcard-semantics)).
  A machine declaring a state that cannot is rejected when it is loaded, so
  every command that reads the machine refuses it before a task is scheduled
  rather than stranding one in the state hours later. One error names every such
  state, says which wildcard target it would not count as progress, and prints
  the transition line to add. A declared edge counts whether or not its
  `condition` or `exit_code` can ever hold, because that is not decidable here:
  the rule under-reports, never over-reports.
- A state that declares both `poll` and `snapshot.inherit` is a validation
  error in v1. Polling states may still emit snapshots on terminal exit when
  otherwise snapshot-capable. See [Snapshots Specification — Counted Loops, Fanout, and Polling](rhei-snapshots.spec.md#103-counted-loops-fanout-and-polling).
- Snapshot operations require a resolved effective target tuple `(agent, mode?,
  provider, model)`. Legacy `agent`/`model` and `all_models` states may use
  snapshots only when normal resolution can derive that tuple; otherwise
  explicit snapshot fields are rejected and auto-emit is skipped.

Counted-loop counters are task-instance data, not state-definition data. The state machine declares the cap with `visits`; runtimes persist the current per-task counts in task metadata and mirror the active visit in markdown by appending `-<n>` to `**State:**` for visits greater than `1`.

When a state declares `all_targets` and `visits`, the engine runs the state
once per listed target and each target-specific execution tracks its own visit
budget. The same scoping rule applies to `all_models`.

### 1.4. Reserved State Names

A state may declare `role: cancellation` to classify abandonment independently
of its name. This optional string accepts only `cancellation`; unknown roles
are rejected with the state name and supported value. An explicit cancellation
role requires `final: true`. When omitted, the bare names `cancelled` and
`canceled` still infer cancellation with their historical behavior; other names
have no cancellation role. Counted task states resolve to their declared base
before classification. No generated-name prefix has semantic meaning.

Five rules key on this effective cancellation classification:

- a `**Prior:**` in it does **not** satisfy a dependency, so a cancelled ticket
  never unblocks downstream work ([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints));
- `rhei complete` never selects it as a completion target;
- the run report marks it apart from success ([§FS-rhei-run-report.3.2](rhei-run-report.spec.md#32-task-tree));
- a transition **into** it waives the *source* state's declared `outputs:` —
  cancellation abandons the work, so that contract is moot
  ([§FS-rhei-transitions.4.5](rhei-transitions.spec.md#45-artifact-enforcement)). The terminal-result obligation still stands.
- a supervising state's `openDescendants` exit does not count reaching it as
  finishing: a supervisor whose every such path ends here is warned about, and
  the run halts on it, exactly as one with no exit at all
  ([§FS-rhei-supervision.1.2](rhei-supervision.spec.md#12-validation-rules)).

`canceled` retains the same inference as `cancelled`. Other names — `dropped`,
`abandoned`, `wontfix` — are ordinary terminals unless they explicitly declare
`role: cancellation`. An explicit role preserves the same five rules through
renaming and composition.

## 2. Polling States

A state marked with a `poll:` block is a *time-triggered* state: on each attempt
the state's `agent` or `program` runs once, the engine evaluates transitions,
and a self-loop transition is interpreted as "not done yet, come back in
`interval`". This is useful for waiting on external systems (CI runs,
deployments, review approvals exposed over an API) without occupying a
`--parallel` slot during the wait.

### 2.1. Shape

```yaml
states:
  ci-wait:
    description: Wait for the remote CI run to finish.
    program: ".rhei/check-ci.sh"
    poll:
      interval: 5m
      max_attempts: 12

  plan-approval:
    description: Wait for the author to answer on the issue.
    program: ".rhei/check-reply.sh"
    poll:
      interval: 10m
      max_attempts: 60
      waiting_on: author
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `interval` | duration string | Yes | Minimum wall-clock wait between attempts (e.g., `30s`, `5m`, `1h`). The `--parallel` slot is released during the wait. |
| `max_attempts` | integer | Yes | Upper bound on total attempts for this state within one task lifetime. Must be ≥ `1`. |
| `waiting_on` | string | No | Declares that this poll waits on a **person**, and names who in a short free-form label (`author`, `reviewer`, `security-team`). Absent means the poll waits on a machine and reads as ordinary backoff. See [Waiting on a Person](#25-waiting-on-a-person). |

### 2.2. Semantics

- **Attempt counter.** `poll.max_attempts` replaces the `visits` cap for the state. The same `metadata.tasks.<id>.stateVisits.<state-name>` counter records attempts; the counter starts at `1` on first entry and increments on every self-loop re-entry, identical to the `visits` accounting in [Transitions Specification — Counted Loops](rhei-transitions.spec.md#43-counted-loops).
- **Retry signal.** Any self-loop transition (`from: X, to: X`) selected by the normal transition-matching rules is interpreted as "not done yet." The engine persists `metadata.tasks.<id>.pollNextAttemptAt.<state-name> = now() + interval`, releases the slot, and stops working this task for this pass. On later passes the task is excluded from the ready set until `pollNextAttemptAt` has elapsed.

  That attempt is recorded as a **wait** — neither a failure nor a completion —
  whatever exit code matched the self-loop. Every surface that reports an
  outcome says `waiting` for it: the run event journal
  ([§FS-rhei-run-tui.1.7](rhei-run-tui.spec.md#17-journal-format)), the `--json` stream ([§FS-rhei-run-json.2.1](rhei-run-json.spec.md#21-records)), the live
  surfaces reading the same events ([§FS-rhei-run-tui.1.1](rhei-run-tui.spec.md#11-event-surface)), and the run report's
  ledger ([§FS-rhei-run-report.4](rhei-run-report.spec.md#4-transition-ledger)). A non-zero exit the machine itself declares
  as "come back later" is not a subprocess that went wrong, and an exit-`0`
  attempt that took the self-loop has not finished the state either, so neither
  `failed` nor `completed` describes it.

  The self-loop is also handled **internally**: the engine schedules the next
  attempt rather than applying a transition, so nothing is appended to the
  central state-transition ledger for it ([§FS-rhei-complete.3.1](rhei-complete.spec.md#31-state-transition-ledger),
  [§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)). Declaring the edge is how the machine grants the state
  permission to repeat; the ledger records where a task went, and a waiting task
  has not gone anywhere. The attempts are in the journal instead, one
  `start@`/`end@` pair each.
- **Exit.** Any non-self-loop transition exits the state normally and clears both `pollNextAttemptAt.<state-name>` and `stateVisits.<state-name>` for that state.
- **Exhaustion.** When `stateVisits.<state-name> >= poll.max_attempts`, the engine will *not* execute a self-loop transition even if one matches. Instead it re-evaluates transitions and picks the first matching non-self-loop. That edge is an ordinary **move**: it is applied, it appends its ledger line, and the attempt that selected it keeps the outcome its own exit earns rather than reading as a wait — the waiting is over, which is what exhaustion means. The recommended pattern is an explicit exhaustion transition:
  ```yaml
  - from: ci-wait
    to: ci-gave-up
    condition: pollAttempts >= pollMaxAttempts
  ```
  If no non-self-loop matches after exhaustion, the task remains in its current state and `rhei run` aborts the task with a clear error (same behavior as "no matching transition found").

### 2.3. Variables for transition conditions

Polling states expose two additional names alongside the existing `visitCount`
and `visits`:

- `pollAttempts` — alias for `visitCount` in a poll state (the current attempt index, `1`-based).
- `pollMaxAttempts` — alias for `poll.max_attempts`.

Both names are available only on transitions whose `from` state declares
`poll:`. Outside a poll state they are undefined.

### 2.4. Interaction with other state features

- **`agent` / `program`.** `poll` works for both. The attempt is one subprocess invocation whose exit code / outputs drive transition matching as usual.
- **`all_targets` / `all_models`.** Fanout composes: each per-target or per-model execution tracks its own `stateVisits.<state-name>` entry (same scoping as `visits`). Slot release applies per fanout invocation.
- **`concurrent`.** Independent: a `concurrent: true` poll state may have multiple tasks in flight simultaneously, each with its own `pollNextAttemptAt`.
- **`agent_timeout` / `program_timeout`.** Bound one attempt's duration; they do not bound the total polling wall-clock time. Combine with `max_attempts` to bound the total.

### 2.5. Waiting on a Person

`poll.waiting_on` answers a question the cadence cannot: *whose* answer ends
this wait. A poll that watches CI and a poll that waits for an author's reply
have the same shape and the same self-resuming schedule, yet only one of them
is a person's turn — and a surface that cannot tell them apart shows a run
waiting on a human being as live work.

- **Presence is the declaration.** A `poll` block that carries `waiting_on` is a
  *person-waiting* poll; one that does not is machine backoff. There is no
  second field and no enum: absence is the existing reading, so every machine
  authored before this field keeps it.
- **The value is a label, not an identity.** It is short free-form text naming
  who is being waited on for a reader — `author`, `reviewer`, a team name. Rhei
  never resolves it to an account, notifies it, or authorizes against it.
  Surrounding whitespace is not part of it: the label is trimmed once when the
  machine loads, so the stored value is the one every surface shows and the
  machine's own JSON cannot name a different person from the text beside it. A
  label that is blank once trimmed is the error in §FS-rhei-states.1.3.
- **Scheduling is unchanged.** The state still self-resumes on its own
  `interval`, still releases its `--parallel` slot between attempts, still
  counts attempts against `max_attempts`, and still leaves the state by an
  ordinary transition. A person-waiting poll is not a `gating` state: the
  distinction is exactly that a gate must be moved by hand and this one moves
  itself when the answer lands. `poll` on a `gating` state stays a validation
  error.
- **Classification is what changes.** The label propagates to the surfaces that
  report what a run is doing, so a self-resuming person wait is legible as one:
  `rhei states` ([§FS-rhei-states-cmd.4](rhei-states-cmd.spec.md#4-text-output), [§FS-rhei-states-cmd.5](rhei-states-cmd.spec.md#5-json-output)), `rhei list`
  ([§FS-rhei-list.4](rhei-list.spec.md#4-output)), the end-of-run summary and report
  ([§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout), [§FS-rhei-run-report.4](rhei-run-report.spec.md#4-transition-ledger)), and the visual surfaces
  ([§FS-rhei-viz.1.1](rhei-viz.spec.md#11-state-category-and-glyph)).
- **Nothing about exit status moves.** Whether a run ends non-zero is still the
  judgment in [§FS-rhei-run.4](rhei-run.spec.md#4-dry-run), unchanged: a person-waiting poll is a poll, and a
  poll inside its backoff window already counted as deliberate waiting there.

## 3. Artifact Contracts

States may declare required file artifacts as explicit contracts. This lets a
workflow say "review must produce findings" or "fix cannot begin until findings
exist" in machine-readable form rather than relying on prose instructions.

Each artifact definition has this shape:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | Yes | Stable identifier for the artifact within that state |
| `path` | string | Yes | Workspace-relative file path template |
| `kind` | string | No | Artifact role. Omitted means a normal artifact. `handoff` marks an output artifact as same-task state handoff prompt context. |
| `description` | string | No | Human-readable explanation of what the artifact contains |
| `optional` | boolean | No | When `true`, a missing file does not block state entry. Only valid on `inputs` entries. Default: `false`. |

Supported path template variables:

- `{task_id}` - the current ticket's **project-qualified** id (`auth.3`), not
  the rhei-local id its heading carries in the plan file ([§AR-rhei-panta.3](../architecture/rhei-panta.spec.md#3-identity-and-id-namespacing))
- `{task_id_local}` - the ticket id as written in its rhei file's heading
  (`3`). Use this — never `{task_id}` — in instructions that author new task
  headings or `**Prior:**` lines into the plan file, since the qualification
  prefix is applied at load time and must not be authored ([§AR-rhei-panta.3](../architecture/rhei-panta.spec.md#3-identity-and-id-namespacing))
- `{rhei_id}` - the owning rhei's id (`auth`); unresolved when the ticket
  carries no qualification prefix
- `{state}` - the canonical unsuffixed state name
- `{visit_count}` - the current visit number for counted-loop states (only available when the state declares `visits`)
- `{target}` - the current execution target selector (only available when the state declares `target` or `all_targets`)
- `{target.slug}` - a filesystem-safe slug derived from `{target}` (only available when the state declares `target` or `all_targets`)
- `{agent}` - the current agent id (available when resolved from `target`, `agent`, or settings)
- `{agent.mode}` - the current agent mode (only available when resolved from `target` or `agent_mode`)
- `{model}` - the current model identifier (available when resolved from `target`, `all_targets`, `model`, or `all_models`)
- `{model.provider}` - the resolved provider id for the current model profile
- `{model.name}` - the resolved provider model name for the current model profile

Runtime semantics:

- `inputs` are checked before entering the state and before work begins in that
  state. If any required (`optional: false`, the default) input file is missing,
  the transition is rejected. Optional inputs (`optional: true`) are not checked;
  entry proceeds regardless of whether the file exists.
- `outputs` are checked after `on_leave` callbacks complete and before the
  transition out of the state is committed. If any required output file is
  missing, the transition or successful-work completion is rejected according
  to the command-specific ordering defined in
  [Plan Language Specification — State Artifact Contracts](rhei-plan-language.spec.md#310-state-artifact-contracts).
- For every declared input — required or optional — the engine resolves the path
  and checks whether the file exists. The result is exposed as:
  - `{input.<name>.exists}` template variable: `true` or `false`.
  - `RHEI_INPUT_<NAME>_EXISTS` environment variable for programs: `true` or `false`
    (name uppercased, hyphens and spaces replaced with underscores).
- Artifact contracts are file-existence contracts in v1. They do not yet define
  JSON schemas, required headings, or content-level validation.
- Output artifacts with `kind: handoff` are still ordinary required outputs for
  transition gating. Their additional behavior is prompt transport: successor
  states may inherit them as same-task state handoff context. Authors should
  keep handoff artifacts concise; Rhei reads and injects the artifact content
  without silently truncating it.
- Under `orchestrator` [Completion Authority](rhei-agents.spec.md#31-completion-authority),
  the `outputs:` existence check doubles as the state's deterministic completion
  signal: the subprocess exit alone is not sufficient, and a zero-exit with any
  missing required output leaves the task in its current state. See
  [Completion Condition](rhei-agents.spec.md#32-completion-condition).

Example:

```yaml
states:
  agent-review:
    description: Review the implementation and record concrete findings
    inputs:
      - name: implementation-summary
        path: runtime/results/{task_id}.md
        description: Result written by the implementation step
    outputs:
      - name: findings
        path: runtime/findings/{task_id}.md
        description: Review findings for the task

  agent-review-fix:
    description: Address reviewer findings without changing scope
    inputs:
      - name: findings
        path: runtime/findings/{task_id}.md
        description: Findings produced by `agent-review`
```

### 3.1. Optional Inputs

Optional inputs are declared with `optional: true`. The engine does not block
state entry when an optional input is absent, but still resolves the path and
exposes `{input.<name>.exists}` so the agent or program can branch on presence.

```yaml
states:
  implement:
    description: Implement the task, incorporating prior-iteration notes when available.
    agent: claude-code
    inputs:
      - name: continuation-notes
        path: runtime/continuation/{task_id}.md
        optional: true
        description: Notes written by the check-continue program on the previous loop iteration.
    instructions: |
      Implement Task {task_id}: {task_title}.

      {if input.continuation-notes.exists}
      A previous iteration left the following notes. Read
      `{input.continuation-notes.path}` before starting and address any
      outstanding issues it identifies.
      {else}
      This is the first iteration. Start from the task description above.
      {endif}

      When finished, transition to `check-continue`.
```

When `continuation-notes` is absent (first loop iteration), the resolved
instructions become:

```text
Implement Task auth.4: Add retry logic.

This is the first iteration. Start from the task description above.

When finished, transition to `check-continue`.
```

When it is present (subsequent iterations):

```text
Implement Task auth.4: Add retry logic.

A previous iteration left the following notes. Read
`runtime/continuation/auth.4.md` before starting and address any
outstanding issues it identifies.

When finished, transition to `check-continue`.
```

The surrounding blank lines are preserved when the block is included and
collapsed to a single blank line when it is removed, so the output is clean in
both cases.

### 3.2. State Handoffs

State handoffs carry notes from a previous state invocation on the same task
into the successor prompt. They are workflow-local context: the current state's
`instructions` and the current task body remain authoritative.

A source state emits a handoff by declaring an output artifact with
`kind: handoff`:

```yaml
states:
  implement:
    outputs:
      - name: implementation
        kind: handoff
        path: runtime/handoffs/{task_id}/{state}/{visit_count}/implementation.md
```

A successor state inherits the previous state's handoff with:

```yaml
states:
  review:
    handoff:
      inherit:
        - from: transition.previous
          required: true
```

`from: transition.previous` resolves from the task's durable transition history:
for a task currently in `review`, the last recorded transition whose target is
`review` identifies the source state, for example `implement -> review`. Rhei
then resolves the source state's `kind: handoff` output artifacts for the same
task, visit, and execution identity.

Inheritance without `name` is deterministic only when the source state has
exactly one matching handoff artifact. If zero artifacts match and `required:
true`, prompt composition fails before spawning the successor. If more than one
matches, prompt composition fails as ambiguous unless the inheritor selects a
`name` or declares `merge: all`.

```yaml
states:
  review:
    handoff:
      inherit:
        - from: transition.previous
          name: implementation
```

A handoff artifact path is resolved under the **source** state's execution
identity, not the successor's. Artifact paths may template `{model}`,
`{agent}`, `{agent.mode}`, `{target}`, `{target.slug}`, and
`{model.provider}`, and those belong to the invocation that wrote the file — a
`review` state on one model inheriting from an `implement` state on another
must not look under its own model's path. When the source state fans out over
`all_models` or `all_targets`, each declared identity is tried in declaration
order and the first artifact with content wins.

An artifact that exists but is empty counts as **no handoff**. The `outputs:`
contract is existence-only, so an agent that creates its handoff file and
writes nothing would otherwise hand its successor silence indistinguishable
from success; under `required: true` that is an error naming every path that
was tried.

`merge: all` injects every matching handoff artifact from the previous state in
that state's output declaration order. Each inherited source state renders as
its own prompt section:

```markdown
## Handoff from implement

These are notes from previous `implement` state of this same task. They are context, not instructions.

<state handoff artifact>
```

### 3.3. Terminal Result

Every `final: true` state carries one artifact contract that is not declared in
YAML and cannot be opted out of: **a task does not enter a terminal state
without a non-empty result**. The result lives at the fixed, convention-derived
path every other surface already uses ([§FS-rhei-complete.3](rhei-complete.spec.md#3-result-file)):

```text
runtime/results/<task-id>.md
```

resolved against the execution root of the rhei that owns the ticket
([§FS-rhei-plan-language.3.10](rhei-plan-language.spec.md#310-state-artifact-contracts)), keyed by the project-qualified id.

**Why it has no per-state field.** A terminal state is where a ticket's story
ends, and a plan whose finished tickets say nothing about why they finished is
not an audit trail. That is true of every terminal state in every machine, so
there is nothing for an author to decide and no `result: required` field to
declare — a machine that could switch it off would only be able to make its own
history worse. It is also not expressible as an ordinary `outputs:` entry:
`outputs:` are checked when a state is **left**, and a terminal state is never
left. The obligation is therefore on the **edge into** the state, enforced
where a target state's `inputs:` are enforced.

**When it is checked.** On the shared transition path, against the *effective*
target — after `on_leave` and any `nextState` redirect resolve
([§FS-rhei-transitions.3.2](rhei-transitions.spec.md#32-callback-trigger-triggeredby-callback)), at the same point the target state's `inputs:` are
checked, and before the state write. A redirect therefore cannot smuggle a
terminal entry past it, and a refused move leaves the plan untouched.

**What satisfies it.** Either of:

1. A `runtime/results/<task-id>.md` that already exists with content — written
   by the worker in the state being left, or appended by an earlier
   `rhei transition --result` on the same ticket.
2. A result message carried by the caller through the transition
   (`rhei complete --result`, `rhei complete --result-file`, `rhei transition
   --result`, or a failure route under `rhei run` the engine genuinely owns —
   a timeout, unavailable tooling, an exit code that matched no route or only a
   `"nonzero"` catch-all, an edge walked with no subprocess). `--result-file`
   changes only how `rhei complete` loads that caller message; after source
   validation it satisfies the same obligation without weakening the
   non-empty requirement. An exit that fired an *exact* `exit_code:`
   transition is the program's declared route rather than the engine's failure
   ([§FS-rhei-programs.3.2](rhei-programs.spec.md#32-evaluation-order)), so
   nothing is carried on it and the program owes the result itself. The message is appended in the existing
   `## Result` entry format ([§FS-rhei-complete.3.2](rhei-complete.spec.md#32-result-file-format)) after the transition
   succeeds.

A file that exists but is only whitespace counts as **no result**, on the same
reading [§FS-rhei-states.3.2](rhei-states.spec.md#32-state-handoffs) applies to handoffs: an existence-only contract
would otherwise let an empty file stand in for an answer.

**A fanned-out state writes fragments, not the file.** A state that fans out
(`all_targets`, `all_models` — [§FS-rhei-transitions.4.2](rhei-transitions.spec.md#42-state-definition)) runs several workers over
one ticket, and each of them has its own account of what it did. One shared
path would make them overwrite each other and let the first writer satisfy the
obligation on everyone's behalf, so under fan-out each invocation is given a
per-invocation **result fragment** instead:

```text
runtime/results/<task-id>/<state>/<visit_count>/<identity>.md
```

The key is the one the state's own per-invocation artifacts already use, in the
same order ([§FS-rhei-states.3.2](rhei-states.spec.md#32-state-handoffs)): the state being worked, the visit of that
state, and `<identity>` — the target slug for `all_targets`, the model id for
`all_models`. All three carry weight. A fragment is one worker's account of one
invocation of one state, and a ticket may fan out more than once: `review` then
`refine` over the same targets, or a counted loop that re-enters `review`. Keyed
by identity alone, every invocation after the first would find a fragment
already on disk, satisfy its completion condition without writing anything, and
hand the ticket a stale account under a state it never came from. `<visit_count>`
alone does not separate them either — an uncounted state renders visit `1` on
every entry — so the path carries both.

The fragment path is what the invocation sees in its prompt
([§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition)), and the completion
condition ([§FS-rhei-agents.3.2](rhei-agents.spec.md#32-completion-condition)) checks *that invocation's own* fragment, exactly
as it checks that invocation's declared `outputs:`. Both sides resolve the path
through the same rule, so the file a worker is told to write is the file the run
looks for.

**Merged once, when the last fragment lands.** An invocation that owes a
fragment has not finished, so `rhei run` does not attempt the ticket's
transition while any declared invocation of the state is still missing one — the
same reason it does not attempt one while a declared `outputs:` artifact of a
sibling invocation is missing. When the last fragment lands and the selected
transition is a `final: true` state, `rhei run` merges the fragments — in
declared invocation order, one `## Result — <identity>` entry each — into
`runtime/results/<task-id>.md` **before** applying the transition, so the shared
path sees one non-empty result carrying every worker's account.

The merge is **idempotent**: the merged block is a deterministic function of the
fragments, and a block the result file already carries is not appended a second
time. A move refused *after* the merge — a target state whose `inputs:` are
missing, say — therefore leaves the merged result on disk, and the next attempt
over the same fragments adds nothing. Fragments that **changed** since are a new
account and do append: entries accumulate, so a result the ticket collected on
an earlier hop is kept, not replaced.

The merge still refuses the move, naming the fragments that are missing, if one
is absent when it runs. That is the rule declared `outputs:` already follow —
they are checked across every invocation identity on the edge out
([§FS-rhei-plan-language.3.10](rhei-plan-language.spec.md#310-state-artifact-contracts)) — and here it is a backstop rather than the
ordinary path: an invocation that wrote nothing fails its own completion
condition first, and nobody reaches the merge.

**A `program:` state is not fanned out.** `rhei run` spawns a program once per
ticket whatever else the state declares, so a program state has one invocation,
no identity, and writes `runtime/results/<task-id>.md` directly — the path
`RHEI_RESULT_PATH` holds for it ([§FS-rhei-programs.2](rhei-programs.spec.md#2-environment-variables)). Asking a state that runs
one process for one fragment per declared target would demand files nothing can
write.

Invocations that are not fanned out are unaffected: they write
`runtime/results/<task-id>.md` directly.

**Who supplies the message.** Whoever knows the outcome. A worker under
`worker` authority ([§FS-rhei-agents.3.1](rhei-agents.spec.md#31-completion-authority)) supplies it on the command that
finishes the ticket. A worker under `orchestrator` authority writes the file
before it exits — the path is in its prompt
([§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition)). The engine supplies the message only for
outcomes the engine itself produced: a timeout it fired, tooling it could not
start, an exit code it read **that no declared route matched**, an edge it
walked with no subprocess in the state
([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)). It never speaks for a worker that ran and never speaks for a
human — where a worker ran, a missing result fails the completion condition,
and a program that exited on an exact `exit_code:` edge is a worker that ran;
where a human decided, the human is **asked**: the human-gate surfaces carry an
optional result field ([§FS-rhei-viz.5.1](rhei-viz.spec.md#51-human-gate-transitions), [§FS-rhei-run-tui.1.5.5](rhei-run-tui.spec.md#155-live-actions-intervene-and-human-gate)) so the operator
who makes the call types the reason on the move that finishes the ticket. They
carry it; they never invent it, so a gate release with no message and no result
on disk is refused exactly as any other terminal move would be.

**What it is not.** It is not a readiness rule. `rhei transition` deliberately
skips `**Prior:**` readiness as the human escape hatch
([§FS-rhei-transition-cmd.3](rhei-transition-cmd.spec.md#3-behavior)), and keeps that; nothing about a deliberate
out-of-order move argues for a free pass on writing down why.

## 4. Template Variables in Instructions and Personality

The `instructions` and `personality` fields support template variable substitution. Variables use the same `{variable}` syntax as artifact path templates. `rhei next` resolves all template variables before printing output, so agents receive fully expanded prompts with no manual variable resolution required.

### 4.1. Variable Namespace

| Variable | Source | Description | Example Value |
|----------|--------|-------------|---------------|
| `{task_id}` | claimed task | Ticket's project-qualified id — the rhei id joined with the rhei-local id, not the plain heading id | `auth.3`, `auth.setup` |
| `{task_id_local}` | claimed task | Ticket id as written in its rhei file's heading. Required for instructions that author headings or `**Prior:**` lines into the plan file ([§AR-rhei-panta.3](../architecture/rhei-panta.spec.md#3-identity-and-id-namespacing)) | `3`, `setup` |
| `{rhei_id}` | claimed task | Owning rhei's id (the qualification prefix); unresolved when the ticket carries none | `auth` |
| `{task_title}` | claimed task | Task title text | `Implement caching layer` |
| `{state}` | state machine | Canonical unsuffixed state name | `review` |
| `{visit_count}` | runtime counter | Current visit number for counted-loop states | `2` |
| `{visits}` | state definition | Configured loop budget for the state | `3` |
| `{target}` | target selector | Current execution target selector (requires `target` or `all_targets`) | `codex[yolo]:openai:gpt-5-codex` |
| `{target.slug}` | target selector | Filesystem-safe slug derived from the current target selector | `codex-yolo-openai-gpt-5-codex` |
| `{model}` | model selector | Current model identifier (requires `target`, `all_targets`, `model`, or `all_models`) | `impl-fast` |
| `{model.provider}` | model selector | Resolved provider id for the current model profile | `anthropic` |
| `{model.name}` | model selector | Resolved provider model name for the current model profile | `claude-sonnet-4-6` |
| `{agent}` | agent selector | Current agent identifier (requires `target`, `all_targets`, `agent`, or settings) | `claude-code` |
| `{agent.mode}` | agent selector | Current agent mode, if one was selected | `yolo` |
| `{plan_title}` | plan header | Title from the `# Rhei: <title>` header | `Feature Branch CI Pipeline` |
| `{plan_path}` | filesystem | Path to the plan file | `./ci-pipeline.rhei.md` |
| `{input.<name>.path}` | artifact contract | Resolved path of a declared input artifact | `runtime/results/auth.3.md` |
| `{input.<name>.exists}` | artifact contract | Whether the input artifact file exists on disk at resolution time | `true`, `false` |
| `{output.<name>.path}` | artifact contract | Resolved path of a declared output artifact | `runtime/findings/auth.3.md` |
| `{mcp.<name>.available}` | tooling | Whether the MCP server with id `<name>` started successfully and is attached to the current agent | `true`, `false` |
| `{skill.<id>.available}` | tooling | Whether the skill with id `<id>` is enabled for the current agent | `true`, `false` |
| `{meta.<key>}` | task metadata | Value from the task's YAML metadata section | `alice`, `2` |

### 4.2. Resolution Rules

- **Resolve at output time, not load time.** Template variables are expanded by `rhei next` when printing instructions to an agent. The state machine YAML remains portable — the same `states.yaml` works across different plans.
- **Fail-open on unknown variables.** An unrecognized variable like `{foo}` is left verbatim in the output. This avoids breaking existing instructions that happen to contain braces and makes templates forward-compatible with future variables.
- **Pure substitution, no expressions.** Templates produce text, not decisions. Conditional logic belongs in transition `condition` fields, not in instructions. The resolved text tells the agent "you are on pass 2 of 3" — the agent reads that to decide what to do.
- **Artifact references create a single source of truth.** Using `{input.<name>.path}` or `{output.<name>.path}` instead of repeating raw paths means the artifact contract defines the path once. If the path changes, instructions stay correct automatically.
- **Agent checkout roots may make artifact paths absolute.** In `rhei run` agent
  mode, when the agent subprocess cwd is a repository root or task worktree
  different from the Rhei artifact root, `{input.<name>.path}` and
  `{output.<name>.path}` resolve to absolute paths under the execution root
  named in the prompt
  ([§FS-rhei-agents.4](rhei-agents.spec.md#4-environment-variables)). When the subprocess cwd is the artifact root, they keep
  the relative form shown above.
- **`{visit_count}` and `{visits}` are only meaningful for counted-loop states.** For states without a `visits` declaration, `{visits}` is left unresolved and `{visit_count}` resolves to `1`.
- **`{target}` and `{target.slug}` are only available for target-based execution.** For states that use the legacy `model` / `all_models` fields, `{target}` is left unresolved.
- **Conditional blocks suppress whole paragraphs.** Use `{if input.<name>.exists}`, `{if mcp.<name>.available}`, or `{if skill.<id>.available}` … `{endif}` to include a block of text only when the referenced artifact, server, or skill is present. Use `{else}` between the opening tag and `{endif}` for an alternative block. The entire block — including surrounding blank lines — is removed from the output when the condition is false. Conditional blocks may not be nested in v1.
- **Escaped braces are literal.** Use `\{` and `\}` when prompt text needs to
  show identifier-shaped runtime syntax literally, for example
  `\{task_id\}` renders as `{task_id}` instead of the current task id. Brace
  content that is not a supported runtime variable already remains literal.

### Example

```yaml
states:
  review:
    description: Review pass that appends findings to a shared artifact.
    instructions: |
      You are on review pass {visit_count} of {visits} for Task {task_id}: {task_title}.

      Review the current task output and append one numbered review pass to
      `{output.review-notes.path}`.

      After each review pass, transition to `fix`.
    visits: 2
    outputs:
      - name: review-notes
        path: runtime/reviews/task-{task_id}-review-{visit_count}.md

  fix:
    description: Fix step that consumes the review artifact.
    instructions: |
      Fix pass {visit_count} of {visits} for Task {task_id}: {task_title}.

      Read `{input.review-notes.path}`, extract the accumulated review
      findings, and update `{output.fix-notes.path}`.

      If there is another review pass remaining (you are in pass
      {visit_count} of {visits}), transition back to `review`. Once every
      review-fix cycle is complete, transition to `completed`.
    visits: 2
    inputs:
      - name: review-notes
        path: runtime/reviews/task-{task_id}-review-{visit_count}.md
    outputs:
      - name: fix-notes
        path: runtime/fixes/task-{task_id}-fix-{visit_count}.md
```

When `rhei next` claims Task auth.3 ("Implement caching layer") during the second visit to `fix`, the agent receives:

```text
Fix pass 2 of 2 for Task auth.3: Implement caching layer.

Read `runtime/reviews/task-auth.3-review-2.md`, extract the accumulated review
findings, and update `runtime/fixes/task-auth.3-fix-2.md`.

Transition back to `review` if 2 < 2.
Otherwise, transition to `completed`.
```

### 4.3. Multi-Target Example

```yaml
states:
  review:
    description: Independent review by each target
    personality: |
      You are reviewing as {agent} in mode {agent.mode} for target {target}.
      Provide a review from your perspective.
      Do not attempt to emulate or defer to another target's style.
    instructions: |
      Review the implementation for Task {task_id}.
      Read `{input.implementation.path}` and write your findings to
      `{output.findings.path}`.
    all_targets:
      - claude-code[yolo]:anthropic:claude-opus-4-7
      - gemini[yolo]:google:gemini-3.1-pro-preview
      - codex[yolo]:openai:gpt-5-codex
    inputs:
      - name: implementation
        path: runtime/results/{task_id}.md
    outputs:
      - name: findings
        path: runtime/findings/{task_id}-{target.slug}.md
```

Here `{target}` and `{target.slug}` appear in both the instructions and the
artifact path. The artifact contract defines the per-target output path once;
instructions reference it by name.

Recommended authoring pattern for heterogeneous multi-target runs:

- Declare one shared state with `all_targets` instead of cloning one state per
  target.
- Use `{target.slug}` in output artifact paths so each execution writes to a
  distinct file, for example `runtime/findings/{task_id}-{target.slug}.md`.
- Prefer the full selector form `<agent>[<mode>]:<provider>:<model>` when the
  workflow depends on all four dimensions. Shorter forms are valid when some
  dimensions are intentionally omitted.
- Add a later synthesis state that consumes the per-target artifacts and writes
  one final document.

### 4.4. Prompt Templates

Prompt templates let one state machine define reusable prompt text once and
reuse it across states with state-specific values. They live as `.md` files in a
sibling `prompt_templates/` directory next to `states.yaml` and are loaded with
that state machine. They are resolved before the normal runtime variables in
[Variable Namespace](#41-variable-namespace). The template body uses
prompt-template placeholders only; values may contain runtime variables such as
`{task_title}` or `{output.findings.path}`.

`prompt_templates/artifact-review.md`:

```markdown
You are a {review_role}. Review only the requested scope.

Review Task {task_id}: {task_title}.

Focus on {focus_area}. Write findings to `{findings_path}`.
```

`states.yaml`:

```yaml
states:
  api-review:
    description: Review API behavior and compatibility.
    prompt_template:
      name: artifact-review
      values:
        task_id: "{task_id}"
        task_title: "{task_title}"
        review_role: API compatibility reviewer
        focus_area: public API behavior and migration risk
        findings_path: "{output.findings.path}"
    outputs:
      - name: findings
        path: runtime/reviews/api-{task_id}.md

  ui-review:
    description: Review UI behavior.
    prompt_template:
      name: artifact-review
      values:
        task_id: "{task_id}"
        task_title: "{task_title}"
        review_role: UI reviewer
        focus_area: user-visible states, copy, and interaction flow
        findings_path: "{output.findings.path}"
    instructions: |
      Also check empty, loading, and error states.
    outputs:
      - name: findings
        path: runtime/reviews/ui-{task_id}.md
```

Resolution rules:

- Each `prompt_templates/<id>.md` file is one reusable prompt fragment, loaded
  as the template's instructions text.
- `prompt_template: <id>` selects a template with no custom values.
- `prompt_template: { name: <id>, values: { ... } }` selects a template and
  supplies concrete placeholder values for that state.
- Placeholder values are scalar YAML values. Strings may include runtime
  variables; those runtime variables resolve after prompt-template placeholder
  substitution.
- Non-identifier brace content is literal and does not require escaping, so
  examples like `{"status":"ok"}` can appear in a prompt template. Use `\{` and
  `\}` only when an identifier-shaped token such as `{focus_area}` must remain
  literal instead of being treated as a prompt-template placeholder.
- Inline `personality` and `instructions` remain valid. When a state declares
  both `prompt_template` and inline text, the selected template text is emitted
  first and the inline state text is appended after a blank line. This lets a
  template provide shared guidance while a state adds local detail.
- A prompt template contributes instructions only. A Markdown file has nowhere
  to declare role framing, so `personality` stays a per-state field and is never
  taken from a template.
- Conditional tags (`{if ...}`, `{else}`, `{endif}`) are control syntax, not
  prompt-template placeholders. Runtime variables should be passed through
  `values`, not written directly in reusable prompt-template text.

## 5. Agent Field

States can declare which coding agent transport executes work in that state. The
`agent` field is always a string id. Rhei resolves the id against the merged
`agents` registry — the built-in agents (`claude-code`, `codex`, `gemini`,
`kilocode`, `cursor`) plus any entries declared in
`~/.config/rhei/settings.json` and the plan root's project settings file
(`.agent-grounds/rhei/settings.json`, §FS-rhei-agents.1.1). Inline
object-shaped agent definitions are not accepted on a state; a custom agent
must be declared in the registry first.

The optional `agent_mode` field selects a named flag set from the resolved
agent's `modes` map. See [Agents Specification — Modes](rhei-agents.spec.md#22-modes)
for the common `yolo` / `safe` conventions and the full mode resolution order.

The optional `effort` field is workflow policy, not execution identity or a
mode. It therefore does not change the selected executor, provider, model,
agent, mode, or permission flags. It combines with legacy `agent`/`model`
selection and explicit `target`, survives task `**Model:**` and `**Target:**`
overrides, and is translated through the profile of the newly effective agent.
`target_locked` keeps its identity-override meaning and adds no effort rule.

For `all_targets` and `all_models`, the same authored effort is applied
separately to every member. Supporting profiles translate it and unsupported
profiles ignore it. If any supporting member cannot represent the value,
validation fails before any fanout member runs. The existing prohibition on
task execution overrides for fanout is unchanged.

```yaml
states:
  draft:
    description: Task requires analysis before execution.
    model: impl-deep
    agent: claude-code
    agent_timeout: 15m

  pending:
    description: Task is ready for implementation.
    model: impl-fast
    agent: claude-code
    agent_mode: yolo
    agent_timeout: 30m

  agent-review:
    description: A separate reviewing agent inspects the result.
    model: review-deep
    agent: codex
    agent_mode: safe
    agent_timeout: 20m

  agent-review-fix:
    description: The implementing agent addresses reviewer findings.
    model: impl-fast
    agent: claude-code
    agent_timeout: 30m

  human-review:
    gating: true
    # No agent - humans act here

  completed:
    final: true
  cancelled:
    final: true
```

When `agent` is set on a state, `rhei run` spawns that agent transport to
execute work. When `agent` is omitted, `rhei run` falls back to project-level
or global-level `defaults.agent`. See
[Agents Specification](rhei-agents.spec.md) for full resolution order,
invocation profiles, modes, and timeout handling.

The `model` field names a model profile, which resolves to a provider/model
combo in settings. The `agent` field is an optional transport for autonomous
execution. Either can be set independently at any configuration level, and the
resolution merges across levels.

## 6. Program States

States can declare a deterministic program to execute instead of spawning an AI agent. The `program` field names a command (string form) or provides a structured command definition (object form). Program states are the right choice for build, test, lint, deploy, and other steps where the behavior is fixed and an AI agent adds no value.

```yaml
states:
  build:
    description: Build the project
    program: "make build"
    program_timeout: 10m

  test:
    description: Run the test suite
    program:
      command: ["npm", "test", "--", "--coverage"]
      env:
        NODE_ENV: test
    program_timeout: 15m
    outputs:
      - name: coverage
        path: coverage/lcov.info

  deploy:
    description: Deploy to staging
    program:
      command: ["./scripts/deploy.sh", "{meta.deploy_env}"]
      working_directory: ./infra
    program_timeout: 20m
```

When `program` is set, `rhei run` spawns the command as a subprocess instead of an agent. The program communicates its outcome via exit code, and transitions from program states can declare `exit_code` conditions for automatic routing:

```yaml
transitions:
  - from: build
    to: test
    exit_code: 0
  - from: build
    to: failed
    exit_code: nonzero
```

A state must not declare both `agent` and `program` — they are mutually exclusive. The `program` field is also incompatible with `gating: true` (programs run autonomously).

See [Program States Specification](rhei-programs.spec.md) for the complete specification including program declaration forms, exit-code transitions, environment variables, timeout handling, and validation rules.

## 7. MCP Servers and Skills

States can attach **MCP servers** and **skills** to the agent subprocess. MCP
servers expose tools through the Model Context Protocol; skills bundle
agent-side prompts and resources. Both are tooling that shapes what an agent
can do in a given phase — research, implementation, review, and so on.

The `mcp_servers` and `skills` fields are lists. Each entry is either a
**registry id** (a string naming an entry in the merged
[`mcp_servers`](rhei-agents.spec.md#114-mcp_servers) or
[`skills`](rhei-agents.spec.md#115-skills) settings registry) or an **inline
object** for one-offs that shouldn't pollute global settings.

### 7.1. Entry forms

```yaml
states:
  pending:
    agent: claude-code
    mcp_servers: [postgres]                         # registry id
    skills: [test-authoring]

  agent-review:
    agent: codex
    mcp_servers:
      - id: postgres                                # registry id (long form)
      - id: grafana
        optional: true                              # warn-and-continue if missing
    skills:
      - id: security-review
      - id: ad-hoc-review                           # inline (no registry entry required)
        path: ./.rhei/skills/ad-hoc-review
        optional: true
```

The object form accepts `id`, `optional`, and the fields of the corresponding
registry entry (`command`/`url`/`transport`/`env`/... for MCP servers; `path`
and `description` for skills). The string form is shorthand for `{id: <name>}`
with `optional: false`.

### 7.2. Effective set

The **effective set** for a state is `defaults.<kind>` ∪ `state.<kind>`,
deduplicated by id. State-level entries override identically-ided defaults.
Passing `mcp_servers: []` or `skills: []` on a state clears the inherited
`defaults` tooling for that state — leaving the field out inherits the
defaults unchanged. See
[Agents Specification — Resolution Order](rhei-agents.spec.md#14-resolution-order)
for the full algorithm.

### 7.3. Runtime semantics

- Entries are resolved and availability-checked by `rhei run` at agent spawn
  time — not at `rhei next`.
- Required entries (the default) that fail their availability check block the
  agent spawn. The engine fires an `mcp_unavailable` or `skill_unavailable`
  transition if one is declared from the current state; otherwise the task
  stays in place with an error logged.
- Optional entries (`optional: true`) that fail produce a warning and are
  dropped from the effective set. The agent still spawns with the remaining
  resolved tooling.
- For every declared entry — required or optional — the engine exposes:
  - `{mcp.<name>.available}` / `{skill.<id>.available}` template variables
    resolved to `true` or `false`.
  - `RHEI_MCP_<NAME>_AVAILABLE` / `RHEI_SKILL_<ID>_AVAILABLE` environment
    variables (name uppercased, hyphens and spaces replaced with underscores).
- The aggregate resolved set is also exposed via `RHEI_MCP_SERVERS` and
  `RHEI_SKILLS` as comma-separated id lists containing only the entries that
  started successfully.

See [Agents Specification — Missing Tooling](rhei-agents.spec.md#6-missing-tooling)
for availability semantics, timeout behavior, and the
`mcp_unavailable` / `skill_unavailable` transition contract. See
[Transitions Specification](rhei-transitions.spec.md) for declaring those
transitions.

### Example

```yaml
states:
  draft:
    description: Research phase — agent needs read access to the issue tracker.
    agent: claude-code
    mcp_servers: [linear]
    instructions: |
      Read Task {task_id}: {task_title} and linked Linear tickets.

      {if mcp.linear.available}
      Use the Linear MCP to fetch related tickets and comments before writing
      the task description.
      {else}
      Linear is unavailable — rely on the task body only and flag any missing
      context in your description.
      {endif}

      Transition to `pending` once the description is finalized.

  pending:
    description: Implementation phase.
    agent: claude-code
    mcp_servers: [postgres]
    skills: [test-authoring]

  agent-review:
    description: Independent review.
    agent: codex
    mcp_servers:
      - id: postgres
      - id: grafana
        optional: true
    skills: [security-review]
    instructions: |
      Review Task {task_id}.

      {if mcp.grafana.available}
      Pull relevant latency panels from Grafana for any request-path changes
      and cite them in your findings.
      {else}
      Grafana is unavailable — note the omission in findings but do not block
      the review on it.
      {endif}
```

## 8. Profiles

A **profile** is a named bundle of `{initial, allowed}` that defines a
per-node state policy. Profiles are defined once at the top level of the
state machine YAML and referenced from `node_policy`.

Separating policy from state definitions means:

- One canonical definition of each state and transition.
- Multiple node kinds can share a profile without duplicating `allowed`
  arrays.
- The root node can use a different profile than task or bug nodes, without
  cloning the state graph.
- `rhei reset` resets each node to its resolved profile's `initial` — there is
  no single machine-wide initial state.

> The `profiles` block in this spec is distinct from the `models` list of
> *model profiles*. A `models` entry names a model profile identifier
> resolved through settings to a provider/model combination. A `profiles`
> entry is a state policy assigned to nodes. They live at different layers
> of the machine and never resolve against each other.

### 8.1. Shape

```yaml
profiles:
  <profile-name>:
    initial: <state-name>        # starting state for any node using this profile
    allowed: [<state-name>, ...] # states any such node may ever hold
```

### 8.2. Per-profile validation

- `initial` must be defined in `states`.
- Every entry in `allowed` must be defined in `states`.
- `initial` must appear in `allowed`.
- `allowed` must contain at least one state marked `final: true`.
- Every non-final state in `allowed` must have a path — using only
  transitions whose `to` is also in `allowed`, and counting an edge as
  [§FS-rhei-transitions.4.6](rhei-transitions.spec.md#46-wildcard-semantics)
  counts it — to some final state in `allowed`. This reachability check
  prevents a narrowed `allowed` set from silently producing a policy where a
  node can enter a state it can never leave. It asks the machine-wide question
  of [Validation Rules](#13-validation-rules) again at profile scope, so a
  profile whose only way out of an allowed state is a wildcard edge to a final
  state is rejected even when the full graph is sound.

### Example

```yaml
profiles:
  simple:
    initial: pending
    allowed: [pending, completed, cancelled]

  reviewed:
    initial: draft
    allowed: [draft, pending, agent-review, agent-review-fix, human-review, completed, cancelled]

  light-review:
    initial: pending
    allowed: [pending, agent-review, completed, cancelled]
```

`allowed` is always a wholesale set — profiles are referenced by name, never
merged. Two profiles that share most of their states still list each state
explicitly.

## 9. Node Policy

`node_policy` maps each node in a plan to a profile. Its keys are the required
`root` (the Panta project root, [§FS-rhei-panta](rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis)) and `default`, and the optional
`rhei` (the rhei tier), `by_type`, and `overrides`.

### 9.1. Shape

```yaml
node_policy:
  root: <profile-name>           # required: profile the Panta project root runs
  rhei: <profile-name>           # optional: profile the rhei tier runs
  default: <profile-name>        # required: fallback for any task kind not listed
  by_type:                       # optional: per-kind overrides (task kinds)
    <kind>: <profile-name>
  overrides:                     # optional: ordered list for multi-dimensional cases
    - match: { type: <kind>, level: <n> }   # level is rhei-local task depth
      profile: <profile-name>
```

### 9.2. Resolution

For a given node, resolve its profile in this order:

1. If the node is the Panta project root (kind `panta`), use
   `node_policy.root`.
2. If the node is a rhei (kind `rhei`), use `node_policy.rhei` when defined.
   When it is omitted, the rhei is a structural rollup with no profile-driven
   state: its status is derived from its descendants ([§FS-rhei-panta.3](rhei-panta.spec.md#3-one-unified-view)), and it
   is never claimed or transitioned.
3. Otherwise the node is a ticket. Walk `node_policy.overrides` in declaration
   order; use the profile of the first entry whose `match` matches the node. A
   `match` block may include `type` and/or `level` (rhei-local task depth);
   every specified field must match the node.
4. Otherwise, if `node_policy.by_type[<kind>]` is defined for the node's kind,
   use it.
5. Otherwise, use `node_policy.default`.

The resolved profile's `initial` is the node's starting state; its `allowed`
set is the authoritative list of states that node may ever hold. `rhei reset`
returns each node to its resolved profile's `initial`.

### 9.3. Validation

- `node_policy.root` is required; it names the Panta project root profile and
  must reference a profile defined in `profiles`.
- `node_policy.default` is required; it must reference a profile defined in
  `profiles`.
- `node_policy.rhei` is optional; when present it must reference a profile
  defined in `profiles`.
- Every key of `node_policy.by_type` must appear in `structure.nodeKinds`.
  The reserved names `panta` and `rhei` must not appear as `by_type` keys or in
  `structure.nodeKinds` — the project root is configured through
  `node_policy.root` and the rhei tier through `node_policy.rhei`.
- Every profile reference in `by_type` and `overrides[].profile` must
  resolve to a profile defined in `profiles`.
- `overrides[].match` keys are limited to `type` and `level`. `type` values
  must be in `structure.nodeKinds`. `level` values must be integers in
  `[1, structure.maxLevels]` and refer to rhei-local task depth. The `panta`
  and `rhei` tiers are not matchable through `overrides`; use `node_policy.root`
  and `node_policy.rhei`.
- Each profile referenced by `node_policy` must pass the per-profile
  validation rules above.
- Any authored `**State:**` on a node must appear in that node's resolved
  profile's `allowed` set.

### Example

```yaml
node_policy:
  root: reviewed        # the Panta project root
  rhei: simple          # the rhei tier (omit to leave rheis as pure rollups)
  default: simple       # task kinds without a type-specific mapping use this
  by_type:
    task: reviewed
    bug:  light-review
  overrides:
    - match: { type: task, level: 3 }   # leaf-level tasks skip review
      profile: simple
```

In this configuration the Panta root uses `reviewed`, each rhei uses `simple`,
and within a rhei a level-3 `task` uses `simple` (via `overrides`), a level-2
`task` uses `reviewed` (via `by_type`), a `bug` at any level uses
`light-review`, and a `story` — a declared kind without a `by_type` entry —
uses `simple`.

## 10. States

| State | Description | Final | Gating |
|-------|-------------|-------|--------|
| `pending` | Task is ready to execute; do the task | No | No |
| `completed` | Task finished successfully; immutable | Yes | No |

Whether a state is a node's starting state is determined by the node's
resolved profile (see [Profiles](#8-profiles) and [Node Policy](#9-node-policy)),
not by a per-state flag.

## 11. Transitions

See [states.yaml](states.yaml) for the enforced transition table. Summary:

- `pending` -> `completed`

Any transition not listed in `states.yaml` is forbidden.

### 11.1. Completion paths

Not every state can be completed directly via `rhei complete`. The command requires a non-cancelled terminal state reachable in one hop:

- From `pending`: direct completion to `completed` is available.

## Related Documentation

- [Plan Language Specification](rhei-plan-language.spec.md) - Formal grammar and semantic constraints
- [Agents Specification](rhei-agents.spec.md) - Agent configuration, invocation profiles, timeout, and log capture
- [Program States Specification](rhei-programs.spec.md) - Deterministic program execution, exit-code transitions
- [Transitions Specification](rhei-transitions.spec.md) - Formal state transition system, callbacks, and YAML schema
- [How Rhei Is Used](rhei-usage.spec.md) - Roles, coordination patterns, and agent workflows
- [Plan Language Usage Guide](rhei-authoring.spec.md) - Practical authoring patterns and walkthroughs
- [Transition Callback Examples](rhei-callbacks.spec.md) - Callback implementations across languages
- [State Machine Writer](rhei-state-machine-writer.spec.md) - Designing custom state machines from project specs and teams
