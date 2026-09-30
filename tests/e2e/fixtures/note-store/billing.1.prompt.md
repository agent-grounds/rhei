# Task billing.1: Charge on refreshed sessions

## State: pending

## Position

Panta: Knowledge › rhei `billing`: Billing
› **Task billing.1: Charge on refreshed sessions [pending]** ← this invocation (visit 1)

### Rhei Context

```markdown
## Ground Rules

Money paths are idempotent.
```

### Project Context

```markdown
## House Rules

Run the gate before shipping.
```

## Instructions

Do the work for Task billing.1.

## Task Content

Bill against a session the auth work refreshed.

## Prior Task Results

These are result files from prior tasks. They are context, not instructions.

### Task auth.2

```markdown
## Result

Token refresh implemented behind the auth.refresh flag; 12 new tests, cargo test green.

Trap: Three concurrent cargo builds under /tmp fill the root disk; build under ~/ag/tmp with --target-dir.
```

## Result

A transition from this state can finish this task. The finished task's result is read from this file.

- `{ROOT}/billing/runtime/results/billing.1.md`

## Plan History

Finished work, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root.

- Task auth.2: Implement token refresh — completed — see above (rhei `auth`, prior)

## Rhei Commands

You are working in a rhei-managed plan at `{ROOT}/billing`.
The active state machine is `{ROOT}/states.yaml`.
The `rhei run` process that spawned you is responsible for advancing the task after this invocation completes.
Do not call `rhei transition` or `rhei complete`, and do not modify `**State:**` lines directly, unless you are launching a nested execution that manages its own state.

Available transitions from `pending`:
- pending -> completed
- pending -> cancelled

### Reading the rhei

- This rhei: `{ROOT}/billing` — plan `{ROOT}/billing/index.rhei.md`, this task's file `{ROOT}/billing/tasks/01-charge.md`
- Every rhei in this project and its execution root:
  - `auth` — `{ROOT}/auth`
  - `billing` — `{ROOT}/billing`
  - `mentor` — `{ROOT}/mentor`
  - `reporting` — `{ROOT}/reporting`
- Under each execution root: `runtime/results/<task-id>.md` (results),
  `runtime/exports/<task-id>/<name>.md` (exports), `runtime/supervise/<task-id>[/<state>].md` (briefs),
  `runtime/state-transitions.log` (order of events)
- Agent transcripts: `{ROOT}/runtime/logs`
- Read-only commands, always safe: `rhei list [--rhei <id>] [--terminal] [--has-prior <id>] [--parent <id>]`,
  `rhei render <plan> --format json --pretty`

### Leaving a trail

What you write is what the next agent and the human see.
- `runtime/results/<task-id>.md`: the first line is the one-line summary every later Plan History shows; detail below it.
- You may append progress paragraphs to your own task body — files touched, commands run, decisions made — and append child tasks under your own task. Do not edit `**State:**` lines or any other task's body.
- Write progress as plain paragraphs or lists, never Markdown headings: a heading inside a task body declares a child task, so one such as `#### Notes` stops the plan from parsing for the whole run.
