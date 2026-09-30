# Task mentor.1.2: Second child

## State: pending

## Position

Panta: Knowledge › rhei `mentor`: Mentor › Task mentor.1: Carry the fact down a subtree [pending]
› **Task mentor.1.2: Second child [pending]** ← this invocation (visit 1)

### Siblings

- Task mentor.1.1: First child [completed]

### Parent: Task mentor.1: Carry the fact down a subtree

```markdown
A paragraph this task appended to its own body, which "Leaving a trail" already permits.
```

### Rhei Context

```markdown
## Ground Rules

Carry what you learn.
```

### Project Context

```markdown
## House Rules

Run the gate before shipping.
```

## Instructions

Do the work for Task mentor.1.2.

## Result

A transition from this state can finish this task. The finished task's result is read from this file.

- `{ROOT}/mentor/runtime/results/mentor.1.2.md`

## Plan History

Finished work, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root.

- Task mentor.1.1: First child — completed — (no result)

## Rhei Commands

You are working in a rhei-managed plan at `{ROOT}/mentor`.
The active state machine is `{ROOT}/states.yaml`.
The `rhei run` process that spawned you is responsible for advancing the task after this invocation completes.
Do not call `rhei transition` or `rhei complete`, and do not modify `**State:**` lines directly, unless you are launching a nested execution that manages its own state.

Available transitions from `pending`:
- pending -> completed
- pending -> cancelled

### Reading the rhei

- This rhei: `{ROOT}/mentor` — plan `{ROOT}/mentor/index.rhei.md`, this task's file `{ROOT}/mentor/tasks/01-parent.md`
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
