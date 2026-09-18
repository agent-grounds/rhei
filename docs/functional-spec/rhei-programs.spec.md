# FS-rhei-programs: Rhei Program States Specification

This document specifies **program states** — states that execute a deterministic program or command instead of spawning an AI coding agent. Program states are the algorithmic complement to agent states: where agents receive prompts and make decisions, programs execute a fixed command and let exit codes and file outputs determine workflow progression.

For agent states see [Agents Specification](rhei-agents.spec.md). For the state machine format see [States Specification](rhei-states.spec.md).

## Overview

A program state runs a command as a subprocess. The command receives context through environment variables and command-line template variables. Its exit code and file outputs determine what happens next. This is the right choice for deterministic workflow steps — builds, tests, linting, deployment, data transforms, migrations — where an AI agent would add latency and cost without value.

Program states participate in the same `rhei run` execution loop as agent states. They support the same artifact contracts, counted loops, timeout handling, log capture, and callback integration. The only difference is how the work is performed: a fixed command instead of a prompted agent.

A program that can directly or transitively reach a model is neural work; the
`program` label and deterministic-looking command are not proof otherwise.
Rhei starts such a program only through §FS-rhei-budgets.4 with a qualified
tuple and confined provider path. Programs proved non-neural keep the ordinary
contract below and do not consume the monetary allowance.

## 1. Program Declaration

### 1.1. String Form

The simplest form runs a shell command:

```yaml
states:
  build:
    description: Build the project
    program: "npm run build"
    program_timeout: 10m
```

String-form commands are executed via the system shell (`/bin/sh -c` on Unix, `cmd /c` on Windows).

### 1.2. Object Form

For more control over execution:

```yaml
states:
  build:
    description: Build the project
    program:
      command: ["npm", "run", "build"]
      env:
        NODE_ENV: production
      working_directory: ./packages/core
    program_timeout: 10m
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `command` | string or string array | Yes | The command to execute. Array form bypasses the shell (exec directly). String form runs via shell. |
| `env` | object | No | Additional environment variables merged with the base set. Values support template variables. |
| `working_directory` | string | No | Override the subprocess working directory. Relative to workspace root. Supports template variables. Default: workspace root. |
| `shell` | boolean | No | Force shell execution even for array-form commands. Default: `false` for arrays, `true` for strings. |

### 1.3. Template Variables in Commands

Program commands support the same template variables as `instructions` and agent prompts:

```yaml
states:
  test:
    program: "./scripts/test.sh {task_id}"
    program_timeout: 5m

  deploy:
    program:
      command: ["./scripts/deploy.sh", "--env", "{meta.deploy_env}"]
      env:
        TASK_TITLE: "{task_title}"
```

Variables are resolved by `rhei run` before spawning the process, using the same resolution rules as agent prompt composition. See [Template Variables](rhei-states.spec.md#4-template-variables-in-instructions-and-personality).

## 2. Environment Variables

Program subprocesses have their own execution environment contract; autonomous
agent isolation does not remove these values from a program Rhei launches:

| Variable | Value |
|----------|-------|
| `RHEI_PLAN_PATH` | Absolute path to the plan file or workspace directory |
| `RHEI_TASK_ID` | Project-qualified ticket id (`auth.1`) — matches command output, `{task_id}`, and result artifact names |
| `RHEI_TASK_ID_LOCAL` | Ticket id as written in its rhei file's heading (`1`) — matches what a script that edits or greps the plan file needs |
| `RHEI_RESULT_PATH` | Absolute path to the ticket's result file, `runtime/results/<task-id>.md` under the Rhei artifact root. Always set, and always the ticket-level file: `rhei run` spawns a program once per ticket, so a program state has one invocation and never writes the per-invocation result fragments a fanned-out agent state does, whatever else the state declares ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)). A program whose exit routes the ticket into a `final: true` state must leave this file non-empty ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)) |
| `RHEI_STATE` | Current state name |
| `RHEI_VISIT_COUNT` | Current visit number (for counted-loop states) |
| `RHEI_ATTEMPT` | Which attempt of this state visit the program is, counting from 1. A program has no prompt, so this is the only way it can tell a retry from a first run — and it re-spawns on an unmet completion condition exactly as an agent does ([§FS-rhei-agents.3.2.1](rhei-agents.spec.md#321-runtime-semantics)) |
| `RHEI_INPUT_<NAME>_EXISTS` | `true` or `false` — whether the declared input artifact exists on disk. Set for every declared input, required or optional. `<NAME>` is the artifact `name` uppercased with hyphens and spaces replaced by underscores (e.g., `continuation-notes` → `RHEI_INPUT_CONTINUATION_NOTES_EXISTS`). |
| `RHEI_INPUT_<NAME>_PATH` | Resolved path of the declared input artifact. Set for every declared input regardless of whether the file exists. Same name transform as `RHEI_INPUT_<NAME>_EXISTS`. |

Additional variables declared in `program.env` are merged on top of this base set. When a `program.env` key collides with a base variable, the `program.env` value wins.

`RHEI_ROOT` is not part of this program contract. A program locates its plan
through `RHEI_PLAN_PATH` and resolves relative files from its working directory.
In particular, it must not acquire an enclosing autonomous worker's artifact
root merely because that worker launched this Rhei (§FS-rhei-agents.4).

The working directory defaults to the workspace root (for directory workspaces) or the plan file's parent directory (for single-file plans), unless overridden by `program.working_directory`.

## 3. Exit-Code Transitions

Programs communicate their outcome through exit codes. Transitions from program states can declare an `exit_code` condition that matches specific exit values, enabling automatic routing without the program needing to call `rhei transition` directly.

```yaml
states:
  build:
    description: Build the project
    program: "make build"

  test:
    description: Run the test suite
    program: "make test"

  build-failed:
    description: Build failed
    gating: true

  test-failed:
    description: Tests failed
    gating: true

transitions:
  - from: build
    to: test
    description: Build succeeded
    exit_code: 0

  - from: build
    to: build-failed
    description: Build failed
    exit_code: nonzero

  - from: test
    to: completed
    description: All tests passed
    exit_code: 0

  - from: test
    to: test-failed
    description: Tests failed with assertion errors
    exit_code: [1, 2]

  - from: test
    to: test-failed
    description: Test infrastructure error
    exit_code: nonzero
```

### 3.1. `exit_code` Field

Added to the transition definition schema (see [Transitions Specification](rhei-transitions.spec.md)):

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `exit_code` | integer, integer array, or `"nonzero"` | No | Exit-code condition for automatic transitions from program states. |

Values:
- **integer** (`0`, `1`, `42`): matches that exact exit code.
- **integer array** (`[1, 2, 3]`): matches any listed exit code.
- **`"nonzero"`**: matches any non-zero exit code not already matched by a more specific transition from the same source state.

### 3.2. Evaluation Order

When a program exits and has not already advanced the task (via `rhei transition` or `rhei complete`):

1. Collect all transitions from the current state that declare an `exit_code` field.
2. Evaluate specific matches first: integer and integer-array conditions are checked against the actual exit code.
3. If no specific match and the exit code is non-zero, evaluate `"nonzero"` transitions.
4. If exactly one transition matches, fire it (with its `on_leave`/`on_enter` callbacks).
5. If multiple transitions match at the same specificity level, also evaluate `condition` fields to disambiguate. If still ambiguous after conditions, it is a validation error.
6. If no transition matches and exit code is `0`, log a warning: `warning: program exited 0 but task {id} did not advance from '{state}'`.
7. If no transition matches and exit code is non-zero, log an error and apply the `--continue-on-error` policy.

On a `poll:` state whose attempt budget is spent, step 1 does not govern the
exhaustion edge: the engine selects the first matching non-self-loop
transition regardless of `exit_code` (§FS-rhei-run.5.1).

**An exit that fired an exact match is a declared route, not a failure.** A
step-2 match — an integer or an integer-array condition — is the program naming
the edge it wants: it chose that code to say where the ticket goes, so
`rhei run` records **no subprocess-failure entry** in the ticket's result for
it, whatever the code was
([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop),
[§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)). A `"nonzero"`
match at step 3 is not a declared route — the program did not choose the code,
it merely failed — so the engine ends the work and says why. Where one state
declares both and the exact edge wins, the entry follows the edge that fired:
none. An exact rule disqualified by its `condition:` at step 5 never fired, so
an exit that falls through to `"nonzero"` is an ordinary failure.

The route is the only account of the outcome the engine has, so a declared
route into a `final: true` state leaves the ticket's result to the program.
That is what `RHEI_RESULT_PATH` is for (§2), and a program that leaves it empty
stalls the ticket rather than advancing it on a sentence the engine wrote in
its place ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)).

### 3.3. Mixing Exit-Code and Manual Transitions

Programs may also call `rhei transition` or `rhei complete` directly via subprocess invocation, just like agents. When a program does so, the task state changes before the program exits, and exit-code evaluation is skipped entirely — the explicit transition takes precedence.

If a transition from a program state has no `exit_code` field, it is available for manual invocation (`rhei transition`) or for the program to call directly, but it will never be selected by exit-code evaluation.

This allows hybrid approaches where a program handles the common case via exit codes and calls `rhei transition` for edge cases that need more nuanced routing.

## 4. Timeout Handling

Program timeout uses the same mechanism as agent timeout.

### 4.1. Configuration

Timeout can be set at three levels:

1. **Per-state** — `program_timeout` field on the state definition:
   ```yaml
   states:
     build:
       program: "make build"
       program_timeout: 10m
   ```

2. **Global default** — `program_timeout` in settings:
   ```json
   { "program_timeout": "15m" }
   ```

3. **CLI override** — `--program-timeout` flag on `rhei run` overrides all levels.

Resolution: CLI override > state-level > settings-level.

Duration format is the same as `agent_timeout`: `30s`, `5m`, `1h`, `2h30m`. See [Agents Specification — Duration Format](rhei-agents.spec.md#72-duration-format).

### 4.2. Behavior

Program timeout uses the same SIGTERM → 10 s grace → SIGKILL sequence and the same timeout-transition evaluation as agent timeout. See [Agents Specification — Timeout Behavior](rhei-agents.spec.md#73-timeout-behavior) for the full step-by-step. The only program-specific differences are:

- The log line reads `program timed out after {duration}` (not `agent timed out`).
- The state-level field is `program_timeout` (not `agent_timeout`).

Timeout transitions fire with `triggeredBy: 'system'`. See [Transitions Specification — Agent Timeout Trigger](rhei-transitions.spec.md#36-agent-timeout-trigger-triggeredby-system) for the transition selection rules.

## 5. Log Capture

Program stdout and stderr are captured using the same log format and naming conventions as agents. All output is written to `runtime/logs/` relative to the workspace or plan root.

### 5.1. Log File Naming

| Scenario | Log file path |
|----------|---------------|
| Simple state | `runtime/logs/task-{task_id}-{state}.log` |
| Counted-loop state | `runtime/logs/task-{task_id}-{state}-{visit_count}.log` |
| Retry within one visit | the name above with `-attempt{n}` appended, `n` counting from 2 |

A program state re-spawns for the same reason an agent state does — it is never
skipped at scheduling, and a pass that finds its completion condition unmet runs
it again — so the attempt rule of
[Agents Specification — Log File Naming](rhei-agents.spec.md#81-log-file-naming)
applies here in full: the attempt is keyed to the state visit, a fresh entry
into the state starts over at the unsuffixed name, and a retry never truncates
the transcript that says why the attempt before it did not finish. A program
spawn writes the same record as an agent spawn
([§FS-rhei-agents.8.4](rhei-agents.spec.md#84-spawn-records)), with `kind` `program` and `worker` the resolved command,
and it spends the same per-visit attempt budget ([§FS-rhei-agents.3.2.3](rhei-agents.spec.md#323-attempt-budget)).

### 5.2. Log Format

Programs use the same header / body / footer structure as agent logs (see [Agents Specification — Log Format](rhei-agents.spec.md#82-log-format)). The program-specific differences are:

- The header marker is `=== rhei program log v1 ===` (not `rhei agent log v1`).
- The header carries `program:` (the resolved command) instead of `agent:`, `model:`, `provider:`, `model_name:`, `mcp_servers:`, and `skills:` — those fields are agent-only.
- Everything else (body, `=== exit ===` footer with `code` / `duration` / `ended`) is identical.

Example:

```
=== rhei program log v1 ===
program: npm run build
task: 3
state: build
started: 2026-04-20T10:30:00Z
timeout: 10m
plan: /home/user/project/plan.rhei.md
===

<raw program stdout and stderr, interleaved>

=== exit ===
code: 0
duration: 2m15s
ended: 2026-04-20T10:32:15Z
===
```

## 6. `rhei run` Integration

### 6.1. Execution Loop

Program states integrate into the same `rhei run` loop as agent states. The resolution priority when `rhei run` encounters a state is:

1. State declares `program` → spawn program.
2. State declares `agent` → spawn agent.
3. Project/global agent settings → spawn agent.
4. No program or agent configured → fail with error.

Programs take precedence because they are declared explicitly on the state and represent a deliberate choice for deterministic execution.

### 6.2. Sequential Mode

Within the sequential execution loop (see [Agents Specification](rhei-agents.spec.md#521-sequential-mode-default---parallel-1)):

1. Find the next claimable task.
2. Check the task's current state.
3. If the state declares `program`, resolve and expand the command.
4. Log the spawn to `runtime/logs/task-{task_id}-{state}[-{visit_count}].log`.
5. Spawn the program as a subprocess.
6. Wait for exit (subject to `program_timeout`).
7. Re-read the plan from disk. Check whether the task's state changed — `rhei transition` and `rhei complete` are file-mutating operations that write the new state directly to the plan file.
8. If state changed, log the transition and continue the loop.
9. If state did not change, evaluate exit-code transitions.
10. If an exit-code transition matches, fire it and continue.
11. If no match: warning (exit 0) or error (exit non-zero).

### 6.3. Parallel Mode

Programs respect the same independence rules as agents in parallel mode. Independent tasks with program states can run concurrently up to `--parallel N`.

### 6.4. Flags

| Flag | Effect on Program States |
|------|--------------------------|
| `--dry-run` | Shows the command that would be spawned, without executing |
| `--no-agent` | No effect on program states (programs are not agents) |
| `--no-program` | Suppresses program spawning; fall back to callback-only advancement |
| `--parallel <N>` | Programs respect the same independence and concurrency rules as agents |
| `--program-timeout <duration>` | Override program timeout for this run |

### 6.5. Dry-Run Output

```
Pass 1: 2 ready, 0 terminal, 5 total.

Would run: npm run build
  Task app.1: Build the project [draft -> build]
  Timeout: 10m
  Log: runtime/logs/task-app.1-build.log

Would spawn: claude --output-format json -p --model claude-sonnet-4-6 --
  Task app.3: Write documentation [draft -> pending]
  Agent: claude-code, Model: impl-fast (anthropic/claude-sonnet-4-6), Timeout: 30m
  Log: runtime/logs/task-app.3-pending.log

Dry run complete - nothing was executed.
```

### 6.6. Gating States

A state must not declare both `program` and `gating: true`. Programs execute autonomously; gating states require human action. This combination is a validation error.

## 7. Interaction with Other Features

### 7.1. Artifact Contracts

Program states use the same `inputs` / `outputs` artifact contracts as any other state — see [States Specification — Artifact Contracts](rhei-states.spec.md#3-artifact-contracts) for the schema, path template variables, and general semantics.

Program-specific timing:

- Required `inputs` are checked before the program is spawned. Missing required inputs abort the spawn.
- Required `outputs` are checked after the program exits and before the exit-code transition is committed. A zero exit with a missing required output leaves the task in its current state.

```yaml
states:
  build:
    program: "make build"
    program_timeout: 10m
    outputs:
      - name: bundle
        path: dist/bundle.js

  test:
    program: "make test"
    program_timeout: 15m
    inputs:
      - name: bundle
        path: dist/bundle.js
    outputs:
      - name: coverage
        path: coverage/lcov.info
```

### 7.2. Callbacks

Programs and callbacks are complementary, the same as agents and callbacks:

1. The program runs and exits.
2. Exit-code transition evaluation determines the target state.
3. The selected transition's `on_leave` and `on_enter` callbacks execute as usual.

```yaml
transitions:
  - from: build
    to: test
    exit_code: 0
    on_leave: "cli:bash ./workflow.sh archive-build"
    on_enter: "cli:bash ./workflow.sh prepare-test-env"
```

### 7.3. Counted Loops

Programs work with `visits` for retry and iteration patterns:

```yaml
states:
  deploy:
    description: Deploy to staging
    program: "./scripts/deploy.sh staging"
    program_timeout: 15m
    visits: 3

  deploy-failed:
    description: Deployment exhausted retries
    gating: true

  verify:
    description: Verify the deployment
    program: "./scripts/verify.sh staging"
    program_timeout: 5m

transitions:
  - from: deploy
    to: deploy
    description: Retry deployment
    exit_code: nonzero
    condition: visitCount < visits

  - from: deploy
    to: deploy-failed
    description: Deployment exhausted retries
    exit_code: nonzero
    condition: visitCount >= visits

  - from: deploy
    to: verify
    description: Deployment succeeded
    exit_code: 0
```

### 7.4. Instructions and Personality

`instructions` on program states serve as documentation — they describe what the program does for human operators viewing `rhei next` output. They are not passed to the program.

`personality` is ignored for program states.

### 7.5. Models

`model` and `all_models` are ignored for program states. Programs do not use AI models. Declaring both `program` and `model`/`all_models` on the same state is a validation warning (not an error, to allow gradual migration).

## 8. Validation Rules

### 8.1. State-Level

- `state.program`, when present, must be a non-empty string or a valid program object with at least a `command` field.
- A state must not declare both `agent` and `program`.
- `state.program` on a `final: true` state is a validation error (terminal states have no work to execute).
- `state.program` on a `gating: true` state is a validation error (gating states require human action).
- `state.program_timeout`, when present, must be a valid duration string (e.g., `30s`, `5m`, `1h`, `2h30m`).
- `program.command`, when present as an array, must be a non-empty array of strings.
- `program.env` values must be strings (after template variable resolution).
- `program.working_directory`, when present, must resolve to a path within the workspace root after template expansion.

### 8.2. Transition-Level

- Transitions with `exit_code` must originate from a state that declares `program`. An `exit_code` on a transition from a non-program state is a validation error.
- Exit-code transitions from the same source state must not have overlapping values at the same specificity. Specifically:
  - Two specific integers must not be equal.
  - Two integer arrays must not share any value.
  - A specific integer must not appear in another transition's integer array.
  - At most one `"nonzero"` transition per source state (before `condition` disambiguation).
- When multiple `"nonzero"` or overlapping transitions exist from the same source, they must have mutually exclusive `condition` fields to disambiguate.

## 9. Per-State Fields (additions to States Specification)

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `program` | string or object | No | The program command to execute in this state. Mutually exclusive with `agent`. String form runs via shell. Object form specifies `command`, `env`, `working_directory`, and `shell`. |
| `program_timeout` | string | No | Maximum time the program may run before being killed (e.g., `10m`, `1h`). Same duration format and timeout handling as `agent_timeout`. |

## 10. Transition Field (addition to Transitions Specification)

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `exit_code` | integer, integer array, or `"nonzero"` | No | Exit-code condition for transitions from program states. Only evaluated when the program exits without calling `rhei transition`. |

## Example: CI Pipeline — Pure Program Workflow

A fully deterministic pipeline with no AI agents:

```yaml
name: ci-pipeline
version: 1.0

states:
  lint:
    description: Run linters and formatters
    program: "npm run lint"
    program_timeout: 5m

  build:
    description: Build the project
    program:
      command: ["npm", "run", "build"]
      env:
        NODE_ENV: production
    program_timeout: 10m

  test:
    description: Run the test suite
    program: "npm test -- --coverage"
    program_timeout: 15m
    outputs:
      - name: coverage
        path: coverage/lcov.info

  deploy-staging:
    description: Deploy to staging environment
    program:
      command: ["./scripts/deploy.sh", "staging"]
      env:
        DEPLOY_ENV: staging
    program_timeout: 20m
    visits: 3

  human-review:
    description: Human verifies staging deployment
    gating: true

  deploy-prod:
    description: Deploy to production
    program:
      command: ["./scripts/deploy.sh", "production"]
      env:
        DEPLOY_ENV: production
    program_timeout: 20m

  completed:
    description: Pipeline finished successfully
    final: true

  failed:
    description: Pipeline failed
    final: true

transitions:
  - from: lint
    to: build
    description: Lint passed
    exit_code: 0
  - from: lint
    to: failed
    description: Lint failed
    exit_code: nonzero

  - from: build
    to: test
    description: Build succeeded
    exit_code: 0
  - from: build
    to: failed
    description: Build failed
    exit_code: nonzero

  - from: test
    to: deploy-staging
    description: Tests passed
    exit_code: 0
  - from: test
    to: failed
    description: Tests failed
    exit_code: nonzero

  - from: deploy-staging
    to: human-review
    description: Staging deploy succeeded
    exit_code: 0
  - from: deploy-staging
    to: deploy-staging
    description: Staging deploy failed, retry
    exit_code: nonzero
    condition: visitCount < visits
  - from: deploy-staging
    to: failed
    description: Staging deploy exhausted retries
    exit_code: nonzero
    condition: visitCount >= visits

  - from: human-review
    to: deploy-prod
    description: Human approved staging
  - from: human-review
    to: failed
    description: Human rejected staging

  - from: deploy-prod
    to: completed
    description: Production deploy succeeded
    exit_code: 0
  - from: deploy-prod
    to: failed
    description: Production deploy failed
    exit_code: nonzero
```

## Example: Mixed Agent and Program Workflow

Program states and agent states coexist naturally in the same state machine. Use programs for deterministic steps and agents for creative work:

```yaml
name: feature-workflow
version: 1.0

states:
  draft:
    description: Agent analyzes requirements and writes task description
    model: impl-fast
    agent: claude-code

  pending:
    description: Agent implements the feature
    model: impl-fast
    agent: claude-code
    agent_timeout: 30m

  build:
    description: Build the project to verify implementation compiles
    program: "make build"
    program_timeout: 10m

  test:
    description: Run the test suite against the implementation
    program: "make test"
    program_timeout: 15m

  agent-review:
    description: A separate agent reviews the implementation
    model: review-deep
    agent: claude-code
    agent_timeout: 20m

  agent-review-fix:
    description: Implementing agent addresses reviewer findings
    model: impl-fast
    agent: claude-code
    agent_timeout: 30m

  completed:
    description: Feature implemented, built, tested, and reviewed
    final: true

  failed:
    description: Pipeline failed
    final: true

transitions:
  - from: draft
    to: pending
    description: Analysis complete, ready for implementation

  - from: pending
    to: build
    description: Implementation complete, verify it builds

  - from: build
    to: test
    description: Build succeeded, run tests
    exit_code: 0
  - from: build
    to: pending
    description: Build failed, agent must fix the code
    exit_code: nonzero

  - from: test
    to: agent-review
    description: Tests passed, submit for review
    exit_code: 0
  - from: test
    to: pending
    description: Tests failed, agent must fix the code
    exit_code: nonzero

  - from: agent-review
    to: completed
    description: Review passed, feature complete
  - from: agent-review
    to: agent-review-fix
    description: Review found issues

  - from: agent-review-fix
    to: build
    description: Fixes applied, re-verify build
```

In this workflow, the agent implements code, then deterministic build and test steps verify the result. If they fail, control returns to the agent for fixes. This creates a tight agent-program feedback loop where each component does what it does best.

## Related Specifications

- [States Specification](rhei-states.spec.md) — State machine format, per-state fields, artifact contracts
- [Agents Specification](rhei-agents.spec.md) — Agent states (the AI-driven complement to program states)
- [Transitions Specification](rhei-transitions.spec.md) — Transition callbacks, timeout transitions, `exit_code` field
- [How Rhei Is Used](rhei-usage.spec.md) — Roles, coordination patterns, agent workflows
- [Templates Specification](rhei-templates.spec.md) — Reusable plan bundles (program states work in templates)
