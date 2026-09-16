# Example Plans

This directory contains example inputs for the current markdown plan compiler implementation.

## Choose an example

Use this table when you know the job you want to model:

| Goal | Start here | Why |
|------|------------|-----|
| First local run with no external agent setup | `agent-discussion/` | Mock agents complete a multi-target discussion workflow in a temporary copy. |
| Simple manual plan authoring | `release-automation.rhei.md` | A compact single-file plan with sections, dependencies, and named ids. |
| Human approval gates | `human-review-loop.rhei.md` | Shows the default `human-review` and `agent-review` flow. |
| Repeatable product or PM work | `product-management-example/` | Fan-out PM entries, aggregate them, and implement a bounded slice. |
| Agent debate or deliberation | `agent-discussion/` | Two agent targets discuss, aggregate disagreement, and apply a decision. |
| Multi-model analysis | `multi-model-analysis/` | Runs analysis across several targets before aggregation. |
| Parallel implementation branches | `parallel-worktrees/` | Demonstrates independent tasks advancing concurrently. |
| Snapshot continuation | `snapshot-continuation/` | Shows snapshot emit, inherit, list, continue, and run override commands. |
| A parent that steers its subtree while it runs | `subtree-supervision/` | A supervising state wakes the parent at every finished child and holds the rest of the subtree in between. |
| A full delivery one supervisor drives | `supervised-delivery-example/` | Implement, code review alongside product review, fix, coverage, and docs — every step released by the supervisor's brief, every handoff a plan export. |
| Live dashboard regression testing | `ui-test-canonical-example/` | Canonical runnable fixture for Flow and TUI/dashboard behavior. |

## Files

- `release-automation.rhei.md`
  Valid example using:
  - rhei overview and requirements sections before `## Tasks`
  - numeric and named task identifiers
  - `**Prior:**` dependencies across numeric and named tasks
  - fenced code block content inside a subtask
  - the built-in `rhei` states (the plan declares no `**States:**`)

- `human-review-loop.rhei.md`
  Valid example using:
  - numeric task identifiers
  - default workflow states including `human-review` and `agent-review`
  - normal subtask content bodies
  - dependency chaining across multiple tasks

- `pm-onboarding-experiment.rhei.md`
  Valid example using:
  - a product-manager-oriented plan with context sections and success metrics
  - numeric task identifiers and subtasks
  - a linear dependency chain from planning through launch recommendation

- `product-management-example/`
  Valid example directory using:
  - `index.rhei.md` plus `tasks/`
  - a template-shipped `.agent-grounds/rhei/settings.json`
  - two configurable PM targets feeding a smart aggregation/validation state
  - a two-pass aggregate and cheaper implementation loop

- `escaped-state-values.rhei.md`
  Valid example using:
  - escaped spaces in `**State:**` values such as `in\ review`
  - a companion custom states file because those states are not present in the default set

- `claude-code/`
  Valid example directory using:
  - `plan.rhei.md`
  - `states.yaml`
  - `**States:** claude-code-simple`
  - a Claude Code least-privilege workflow with only simple states

- `living-review-loop/`
  Valid example directory using:
  - `index.rhei.md` plus `tasks/`
  - `team-states.yaml`
  - orchestrator callbacks that append new workspace task files during `rhei run`
  - a shared findings artifact followed by verification and selective fix tasks

- `review-fix-visits/`
  Valid example directory using:
  - `index.rhei.md` plus `tasks/`
  - `states.yaml`
  - counted `review` and `fix` states with two total passes each
  - one review artifact file and one fix artifact updated across the loop

- `subtree-supervision/`
  Valid example directory using:
  - `index.rhei.md` plus `tasks/` and `states.yaml`
  - a state that declares `execute_on: descendant-terminal`, making its task a supervisor
  - the three supervisor edges — exhaustion, `openDescendants < 1`, and the
    unconditional self-loop that releases the subtree
  - a committed mock agent, so the whole chain runs without credentials

- `supervised-delivery-example/`
  Valid example directory using:
  - `index.rhei.md` plus `tasks/`, `states.yaml`, and `prompt_templates/`
  - a `supervising` root task with `execute_on: child-terminal` and ten children
    unrolled from the template's round ceilings
  - a required `brief` input on every child state, so the supervisor's brief is
    what releases each step
  - `**Provides:**` / `**Consumes:**` exports declared as the producing state's
    `outputs:`, with every producer also named directly in the consumer's
    `**Prior:**`, so a step cannot finish without publishing nonblank JSON
  - Instantiated from the `supervised-delivery` built-in template; regenerate it
    with the command in its own README.

- `snapshot-continuation/`
  Valid example directory using:
  - `index.rhei.md` plus `tasks/`
  - a project-scoped `.agent-grounds/rhei/settings.json` with a snapshot-capable agent profile
  - a named `snapshot.emit:` followed by required same-agent `snapshot.inherit:`
  - documented `snapshot list`, `snapshot continue`, `snapshot show`, and
    `run --from-snapshot` commands

- `hourly-human-intervention-example/`
  Valid example directory using:
  - `index.rhei.md` plus `tasks/`
  - a template-shipped `.agent-grounds/rhei/settings.json`
  - a routed hourly GitHub sweep with human gates for GraalVM fixes

- `ui-test-canonical-example/`
  Canonical **runnable fixture** for the live UI. Valid example using:
  - a directory workspace instantiated from `.agent-grounds/rhei/templates/ui-test-canonical/`
  - three-level nested task nodes (`###` / `####` / `#####`) inherited from the
    workspace index `structure.maxLevels`
  - mock agent and mock program execution only
  - artifact `inputs:` / `outputs:`, target fan-out, counted loops, polling,
    callbacks, generated follow-up task files, snapshots, seeded gates, and
    terminal examples
  - Run the mock workflow with `cargo xtask examples run ui-test-canonical --viz`.

- `states-with-spaces.yaml`
  Companion states file for `escaped-state-values.rhei.md`.

## Verification commands

Each example is registered with the `xtask` build helper, which selects the
correct state machine and runtime setup per example:

```bash
cargo xtask examples list                      # show all examples
cargo xtask examples validate <name>           # validate one example
cargo xtask examples validate --all            # validate every example
cargo xtask examples run agent-discussion      # run a runnable mock-backed example in a tmp copy
```

Direct CLI invocations still work if you need a one-off — for example, rendering
an example as JSON:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format json --pretty
```

## Notes on current behavior

These examples are aligned to the current repository behavior, including the following implementation details:

- task-level descriptive paragraphs are accepted by the parser but are not currently preserved in the AST or render outputs
- subtask numbering is validated only for numeric parent task identifiers
- named parent tasks with subtasks may produce a validation warning rather than an error
- state values with spaces must be written using backslash escapes and validated against the selected states file
