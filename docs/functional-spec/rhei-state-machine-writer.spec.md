# FS-rhei-state-machine-writer: Rhei State Machine Writer

This document specifies a new role in the Rhei ecosystem: the **State Machine Writer**. It designs custom state machines (YAML files) tailored to a project's specification, team structure, and workflow requirements.

For the YAML schema and transition callback system see the [Transitions Specification](rhei-transitions.spec.md). For the default state machine see the [States Specification](rhei-states.spec.md).

## Purpose

The default `rhei` state machine is a minimal `pending` -> `completed` workflow with only basic task instructions. Many projects need domain-specific workflows:

- A compliance-heavy project may require multiple approval gates from distinct teams.
- A data pipeline project may model stages like `ingesting`, `transforming`, `validating`, `published`.
- A release process may need `staging`, `canary`, `rollout`, `stable`.

The state machine writer analyzes two inputs — the **project specification** and the **project teams** — and produces a state machine YAML file that encodes the project's real workflow as Rhei states and transitions.

## 1. Inputs

### 1.1. Project Specification

The project specification describes what the project does, its deliverables, phases, quality gates, and constraints. This can come from:

- A requirements document, PRD, or design doc.
- A README or AGENTS.md in the repository.
- Verbal description from the user.
- An existing Rhei plan whose workflow doesn't match the default machine.

The state machine writer extracts from the specification:

| Extract | Maps to |
|---------|---------|
| Distinct workflow phases | States |
| Phase ordering and branching | Transitions |
| Quality gates and checkpoints | Gating states (no autonomous exit) |
| Failure modes and recovery paths | Error/retry states and transitions |
| Automated checks | Callback-eligible transitions |

### 1.2. Project Teams

The team structure describes who is involved and what authority each team or role has. This can come from:

- An explicit team list with roles.
- Organizational context described by the user.
- Existing approval workflows (PR reviewers, sign-off chains).

The state machine writer maps teams to workflow elements:

| Team aspect | Maps to |
|-------------|---------|
| Teams that must approve before proceeding | Gating states with team name (e.g., `security-review`, `legal-review`) |
| Teams that perform work | Work states with team-specific instructions |
| Handoff points between teams | Transitions between team-owned states |
| Autonomous agents | Non-gating states with agent instructions |
| Human decision-makers | Gating states (no autonomous exit) |

## 2. Output

The state machine writer produces a single YAML file conforming to the [YAML State Machine Format](rhei-transitions.spec.md#4-yaml-state-machine-format-specification). The file is ready to be referenced by a Rhei plan's `**States:**` declaration.

### 2.1. Output Structure

```yaml
name: <project-derived-name>
version: 1.0
personality: <optional agent persona or role framing text>
models:                              # optional: declare model profile identifiers
  - <model-id>
  - <model-id>

states:
  <state-name>:
    description: <what this state means in the project context>     # required
    instructions: |                                                 # optional
      <agent-facing guidance: what to do in this state, when to transition out>
    personality: <optional state-specific role framing override>
    final: true|false    # at least one state must be final
    gating: true|false   # optional: if true, no autonomous exit allowed
    visits: <integer>    # optional: max counted visits for loop states
    all_models: [<model-id>, ...] # optional: run once for each listed declared model profile
    model: <model-id>      # optional: exactly one declared model profile may work this state

transitions:
  - from: <source-state>
    to: <target-state>
    description: <when and why this transition occurs>              # required
    # Optional callback fields when automation is needed:
    # on_leave: <callback-name>
    # on_enter: <callback-name>
    # condition: <expression>
    # timeout: <duration>

profiles:                            # required: named state policies
  <profile-name>:
    initial: <state-name>            # starting state for nodes using this profile
    allowed: [<state-name>, ...]     # states any such node may ever hold

node_policy:                         # required: maps nodes to profiles
  root: <profile-name>               # profile the (always-rhei) root runs
  default: <profile-name>            # fallback for non-root kinds not listed
  by_type:                           # optional: per-kind overrides
    <kind>: <profile-name>
  overrides:                         # optional: ordered, first-match-wins
    - match: { type: <kind>, level: <n> }
      profile: <profile-name>

# Optional: platform-specific callback mappings
# callbacks:
#   cli:
#     <callback-name>: <implementation-path>
#   nodejs:
#     <callback-name>: <implementation-path>
```

When a machine declares `models`, each state may either:

- omit model selectors entirely
- set `all_models: [<model-id>, ...]`
- set `model: <model-id>`

It must not set both `all_models` and `model` on the same state.

## 3. Design Rules

The state machine writer follows these rules when designing a state machine:

### 3.1. State Design

1. **Every distinct workflow phase gets its own state.** Don't overload a single state with multiple meanings. If two phases have different instructions or different exit conditions, they are different states.

2. **Name states after what is happening, not who is doing it.** Prefer `security-review` over `security-team`. The state describes the phase; the instructions describe the actor.

3. **Declare starting states on profiles, not on states.** A state definition never carries an `initial: true` flag. Each profile in the top-level `profiles` block declares its own `initial` state; a node resolves to a profile through `node_policy`, and that profile's `initial` is where the node starts. In most workflows, the initial is a "ready" or "pending" state — not an active work state.

4. **Mark at least one state as `final: true`.** Terminal states are absorbing — no outgoing transitions. Every workflow needs at least a success terminal (`completed`, `deployed`, `published`) and typically a cancellation terminal (`cancelled`, `abandoned`).

5. **Make human approval gates explicit.** Any state where an agent must stop and wait for human judgment should have instructions that say "do not transition out of this state autonomously." These are gating states. Name them to reflect the approver: `legal-review`, `manager-approval`, `security-sign-off`.

6. **Keep state count proportional to workflow complexity.** A simple project needs 4-6 states. A complex multi-team pipeline might need 8-12. More than 15 states suggests the workflow should be split into separate state machines for different plan types.

7. **Include recovery states when failure is expected.** If a phase can fail and be retried, model the retry path explicitly (e.g., `validation-failed` → `fixing` → `validating`). Don't rely on implicit "go back to the previous state."

### 3.2. Transition Design

1. **Every transition must be explicitly declared.** Unlisted transitions are forbidden. This is the core safety property of Rhei state machines.

2. **Transitions encode the project's real workflow, not hypothetical paths.** If a transition never happens in practice, don't declare it. If a transition is rare but valid, declare it.

3. **Fan-out from a state represents decisions.** When a state has multiple outgoing transitions, each target represents a different outcome. Document in the transition's `description` when each path is taken.

4. **The transition graph must be connected.** Every non-terminal state must have a path to at least one final state, counting an edge as [§FS-rhei-transitions.4.6](rhei-transitions.spec.md#46-wildcard-semantics) counts it, and the same must hold inside each profile's `allowed` set, using transitions whose `to` also lies in `allowed`. Every state in a profile's `allowed` set other than its `initial` must be reachable from the `initial` state the same way. A dead-end state is not a design error to be caught in review: the machine is refused when it loads.

5. **Provide a cancellation path.** Use a wildcard transition (`from: "*"`) to a `cancelled` terminal state, or declare explicit cancellation transitions from each non-terminal state. Every task must be cancellable. A wildcard edge to a final state is an escape hatch and never a state's way forward, so it does not discharge rule 4 — the exception is a `gating: true` state, which a human leaves with `rhei transition`.

6. **Team handoffs are transitions.** When work passes from one team to another, model it as a transition between team-owned states. The `on_leave` callback on the source state packages the deliverable; the `on_enter` callback on the target state notifies the receiving team.

### 3.3. Profile and Node Policy Design

1. **Start with a single default profile.** If every node kind in the plan follows the same flow, define one profile (for example `default`) whose `allowed` set is the full list of states, and point both `node_policy.root` and `node_policy.default` at it. Only split into multiple profiles once you have concrete evidence that different kinds need different flows — it's easier to split later than to collapse prematurely fractured profiles.

2. **Name profiles for the policy, not the kind.** Prefer `reviewed`, `simple`, `light-review` over `task-profile`, `bug-profile`. A profile can apply to multiple kinds; naming it after a kind implies a coupling that isn't there.

3. **Each profile's `allowed` is wholesale.** Profiles are referenced by name, never merged. Two profiles that share most of their states still list each state explicitly. This keeps the meaning of `allowed` predictable and removes any "what wins where" resolution rules.

4. **The root always resolves through `node_policy.root`.** The root node's kind is always `rhei` (a reserved kind), so it's never matched by `by_type` or `overrides`. Pick its profile explicitly — often it shares a profile with the top-level work kind, but there's no requirement to.

5. **Use `overrides` only when `by_type` cannot express the rule.** `by_type` covers the common case ("all tasks follow this flow"). Add an `overrides` entry only for genuinely multi-dimensional cases — for example, "leaf-level tasks skip review." Entries are first-match-wins in declaration order; keep the list short enough to read top to bottom.

### 3.4. Instructions Design

1. **Write instructions for the actor in that state.** If the state is for an agent, write what the agent should do. If the state is a human gate, write what the human is expected to decide.

2. **Encode exit conditions structurally, not in instructions.** Under `orchestrator` authority, `rhei run` derives completion from subprocess exit plus the state's required `outputs:`, and it selects the next state from transition `condition` / `exit_code` fields. `instructions` and `personality` therefore describe the domain work only; they must not tell the actor how or when to stop, or when to call transition commands. Gating states (`gating: true`) are the one exception: no subprocess is spawned there, so their instructions address a human reader and should explicitly say "do not transition out of this state autonomously" to mark the hand-off. See [Agents Specification — Completion Authority](rhei-agents.spec.md#31-completion-authority).

3. **Reference concrete artifacts.** Don't write "review the work." Write "review the implementation against the task description and its child task nodes. Check that tests pass and lint is clean."

4. **Use template variables instead of placeholders.** When instructions reference task-specific data, use resolved template variables (`{task_id}`, `{task_title}`, `{visit_count}`, `{visits}`, `{model}`) instead of prose placeholders like `<id>`. When a state declares artifact contracts (`inputs:` / `outputs:` — see [States Specification — Artifact Contracts](rhei-states.spec.md#3-artifact-contracts) for the YAML schema), reference them by name (`{input.<name>.path}`, `{output.<name>.path}`) instead of repeating raw paths. See the [States Specification](rhei-states.spec.md#4-template-variables-in-instructions-and-personality) for the full variable namespace.

## 4. Workflow

The state machine writer follows this process:

1. **Gather inputs.** Read the project specification and team structure. If either is missing or incomplete, ask the user before proceeding.

2. **Identify phases.** List the distinct workflow phases from the specification. Each phase becomes a candidate state.

3. **Identify actors.** Map each phase to its actor: which team or agent type performs work in that phase, and who approves the exit.

4. **Draft states.** For each phase, write a state with a description and instructions. Mark final states. Do not mark initial states here — initial states belong to profiles (step 6).

5. **Draft transitions.** For each pair of phases that have a direct handoff, declare a transition. Add a description explaining when the transition fires.

6. **Draft profiles and node policy.** Identify which node kinds in the plan need distinct flows (often there's only one). For each distinct flow, define a profile with an `initial` and `allowed` set. Point `node_policy.root` and `node_policy.default` at profiles, and add `by_type` entries for any non-root kind whose flow differs from `default`. Add `overrides` only for multi-dimensional rules.

7. **Add safety paths.** For every profile, ensure every non-final state in `allowed` has a path to a final state in `allowed`, using only transitions whose `to` is also in `allowed`. Add cancellation transitions. Add recovery/retry paths for phases that can fail.

8. **Add callbacks (optional).** If the project uses programmatic transitions, assign `on_leave` and `on_enter` callback names to transitions that integrate with external systems.

9. **Validate the design.**
   - Every profile declares an `initial` and a non-empty `allowed` set.
   - Every profile's `initial` is a member of its `allowed` set.
   - Every profile's `allowed` contains at least one final state.
   - `node_policy.root` and `node_policy.default` reference defined profiles.
   - Every `by_type` key is a declared non-root node kind; `rhei` is not used as a `by_type` key.
   - For every profile, every non-final state in `allowed` has a path to a final state in `allowed` using transitions confined to `allowed`.
   - At least one final state exists globally (typically two: success and cancellation).
   - No orphan states (defined in `states` but not referenced by any profile's `allowed`, transition, or override).
   - State names are lowercase, hyphenated identifiers (matching the `IDENTIFIER` grammar production).
   - Non-terminal states encode their exit conditions structurally — through required `outputs:` artifacts and through transition `condition` / `exit_code` fields — not in prose inside `instructions`. Gating states must instead tell the human reader not to transition autonomously.
   - Under `orchestrator` authority, every non-gating, non-final state resolves to a finite `agent_timeout` or `program_timeout` at some level of the timeout chain; see [Agents Specification — Timeout Requirement](rhei-agents.spec.md#322-timeout-requirement).

10. **Write the YAML file.** Emit the file conforming to the YAML State Machine Format.

11. **Validate with the CLI.** If the `rhei` CLI is available, run `rhei states --state-machine <path>` to verify the file parses correctly.

## Examples

> **Note:** The instructions in these examples include phrasing like "on
> success, transition to X" for readability. In production state machines
> run under `orchestrator` authority, encode that routing in transition
> `condition` / `exit_code` fields and `outputs:` artifacts instead, and
> keep `instructions` focused on the domain work (see Instructions Design
> rule #2 above). The examples below are shown in the looser convention for
> narrative clarity only.

### Example 1: Data Pipeline Project

**Input — specification:** "We process customer data through ingestion, transformation, validation, and publication stages. Bad data must be quarantined for manual review."

**Input — teams:** "Data engineering builds and runs the pipeline. Data quality team reviews quarantined records. Product team signs off on publication to production."

**Output:**

```yaml
name: data-pipeline
version: 1.0

states:
  queued:
    description: Data batch is registered and waiting to be processed.
    instructions: |
      Pick up when all Prior tasks are completed. Transition to ingesting
      to begin processing.

  ingesting:
    description: Raw data is being loaded from source systems.
    instructions: |
      Load data from the configured source. Validate schema on ingestion.
      On success, transition to transforming. On schema failure, transition
      to quarantined.

  transforming:
    description: Data is being cleaned, enriched, and reshaped.
    instructions: |
      Apply transformation rules. Log row counts before and after.
      On success, transition to validating.

  validating:
    description: Transformed data is checked against quality rules.
    instructions: |
      Run data quality checks (null rates, range checks, referential
      integrity). On pass, transition to publication-review. On fail,
      transition to quarantined.

  quarantined:
    description: Data failed quality checks and requires manual review.
    instructions: |
      Do not transition out of this state autonomously. Wait for the
      data quality team to investigate, fix, and either return to
      ingesting (reprocess) or cancel.

  publication-review:
    description: Product team reviews data before production publication.
    instructions: |
      Do not transition out of this state autonomously. Wait for
      product team sign-off. On approval, transition to published.
      On rejection, transition to transforming for rework.

  published:
    description: Data is live in production.
    instructions: |
      Terminal state. Do not modify.
    final: true

  cancelled:
    description: Processing abandoned.
    instructions: |
      Terminal state. Skip entirely.
    final: true

transitions:
  - from: queued
    to: ingesting
    description: Begin data processing
  - from: ingesting
    to: transforming
    description: Ingestion succeeded, proceed to transformation
  - from: ingesting
    to: quarantined
    description: Ingestion failed schema validation
  - from: transforming
    to: validating
    description: Transformation complete, run quality checks
  - from: validating
    to: publication-review
    description: Quality checks passed, ready for product review
  - from: validating
    to: quarantined
    description: Quality checks failed
  - from: quarantined
    to: ingesting
    description: Data quality team approved reprocessing
  - from: publication-review
    to: published
    description: Product team approved publication
  - from: publication-review
    to: transforming
    description: Product team requested rework
  - from: "*"
    to: cancelled
    description: Processing abandoned at any stage

profiles:
  default:
    initial: queued
    allowed:
      - queued
      - ingesting
      - transforming
      - validating
      - quarantined
      - publication-review
      - published
      - cancelled

node_policy:
  root: default
  default: default
```

### Example 2: Multi-Team Feature Delivery

**Input — specification:** "Features go through design, implementation, security review, QA, and release. Security issues block release."

**Input — teams:** "Product designs features. Engineering implements. Security team reviews for vulnerabilities. QA validates functionality. Release engineering manages deployments."

**Output:**

```yaml
name: feature-delivery
version: 1.0

states:
  design:
    description: Feature is being specified by product team.
    instructions: |
      Write the feature specification including acceptance criteria.
      Transition to ready-for-dev when the spec is complete and reviewed.

  ready-for-dev:
    description: Feature spec is approved, waiting for engineering pickup.
    instructions: |
      Pick up when all Prior tasks are completed. Transition to
      implementing to begin development.

  implementing:
    description: Engineering is building the feature.
    instructions: |
      Implement the feature per the specification. Write tests.
      When implementation is complete and self-tested, transition
      to security-review.

  security-review:
    description: Security team inspects the implementation for vulnerabilities.
    instructions: |
      Do not transition out of this state autonomously. Wait for
      the security team to review. On pass, transition to qa.
      On fail with findings, transition to security-fix.

  security-fix:
    description: Engineering addresses security findings.
    instructions: |
      Fix only the issues identified by the security team. No scope
      expansion. Transition back to security-review when fixes are applied.

  qa:
    description: QA team validates the feature against acceptance criteria.
    instructions: |
      Do not transition out of this state autonomously. Wait for
      QA validation. On pass, transition to release-ready.
      On fail, transition to implementing for rework.

  release-ready:
    description: Feature is approved and waiting for release window.
    instructions: |
      Release engineering picks up and transitions to released
      during the next deployment window.

  released:
    description: Feature is deployed to production.
    instructions: |
      Terminal state. Do not modify.
    final: true

  cancelled:
    description: Feature is abandoned.
    instructions: |
      Terminal state. Skip entirely.
    final: true

transitions:
  - from: design
    to: ready-for-dev
    description: Feature spec complete and approved
  - from: ready-for-dev
    to: implementing
    description: Engineering picks up the feature
  - from: implementing
    to: security-review
    description: Implementation complete, ready for security review
  - from: security-review
    to: qa
    description: Security review passed
  - from: security-review
    to: security-fix
    description: Security issues found, needs fixes
  - from: security-fix
    to: security-review
    description: Security fixes applied, ready for re-review
  - from: qa
    to: release-ready
    description: QA validation passed
  - from: qa
    to: implementing
    description: QA found defects, needs rework
  - from: release-ready
    to: released
    description: Deployed to production
  - from: "*"
    to: cancelled
    description: Feature abandoned at any stage

profiles:
  default:
    initial: design
    allowed:
      - design
      - ready-for-dev
      - implementing
      - security-review
      - security-fix
      - qa
      - release-ready
      - released
      - cancelled

node_policy:
  root: default
  default: default
```

## Relationship to Other Roles

| Role | Relationship |
|------|-------------|
| **Plan Writer** | Uses the state machine produced by this role. References it via `**States:** <name>` in the plan header. |
| **Plan Worker** | Executes tasks using the states and transitions defined by this role. Follows the `instructions` field on each state. |
| **Reviewer** | Review states in the machine define what the reviewer checks and how they advance work. |
| **Human Operator** | Gating states designed by this role define where human judgment is required. |

The state machine writer runs **before** the plan writer. The plan writer consumes the YAML file; it does not design workflows.

## When to Use a Custom State Machine

Use the default `rhei` state machine when:
- The project follows a standard agent workflow (implement, review, complete).
- There are no domain-specific phases or team handoffs.
- The plan is small and does not need specialized gates.

Use a custom state machine when:
- The project has domain-specific phases (data stages, compliance gates, deployment tiers).
- Multiple teams are involved with distinct approval authorities.
- The workflow has non-trivial branching (retry loops, conditional paths, escalation chains).
- Automated callbacks should fire on specific transitions.
- The default states don't capture the project's real workflow.

## 5. File Placement

Rhei finds a state machine only where state-machine resolution looks ([§FS-rhei-plan-language.1.3](rhei-plan-language.spec.md#13-state-machine-resolution)), and only in a file named `states.yaml`. Place a machine by what it governs:

- **A single-file plan:** `states.yaml` in the plan's directory.
- **A Directory Workspace:** `states.yaml` at the workspace root.
- **A Panta project's custom default:** `states.yaml` at the project root (the directory holding `index.panta.md`), named by that manifest's `**States:**`. Every rhei that declares no `**States:**` runs under it. When an adopted project has no matching project-root file, one unique matching member-root file may supply this custom default.
- **A Panta project's built-in `rhei` default:** an optional matching `states.yaml` at the project root replaces the built-in machine. Without that project-root file, the project uses built-in `pending`/`completed`; a matching file found only in a member rhei root does not replace the default, whether the member inherits or restates `rhei`.
- **One rhei's own process:** `states.yaml` at that rhei's execution root, named by the rhei's own `**States:**` when that names a machine other than the project default. Every instantiated template ships this shape. A rhei whose `**States:**` restates the default's name runs the project default and resolves it exactly as the default resolves: its own root gets no precedence. It can therefore supply a custom default only through that default's unique name-match fallback; it can never supply the built-in `rhei` project default from a member root ([§DA-per-rhei-state-machines](../decisions/architectural/per-rhei-state-machines.md#da-per-rhei-state-machines-the-state-machine-is-a-per-rhei-property-defaulted-by-the-manifest)).

The file's `name` must equal the `**States:**` value that selects it. No other directory or file name is searched. A machine kept anywhere else loads only when every invocation passes `--state-machine <path>`. That flag replaces resolution for the whole scope, so it works for a plan, a workspace, or a project that runs a single machine. It cannot supply one machine among several.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — formal grammar and semantic constraints
- [States Specification](rhei-states.spec.md) — default state machine format
- [Transitions Specification](rhei-transitions.spec.md) — YAML schema, callbacks, and transition system
- [How Rhei Is Used](rhei-usage.spec.md) — roles, coordination patterns, and agent workflows
