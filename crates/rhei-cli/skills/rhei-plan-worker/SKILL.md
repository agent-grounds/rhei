---
name: rhei-plan-worker
description: Execute tasks in a Rhei Plan markdown document without an orchestrator. Takes one required argument `<plan>` — the path to a `.rhei.md` file or a Directory Workspace `index.rhei.md` (e.g. `rhei-plan-worker docs/my-plan.rhei.md`). Use when the user asks to work, implement, advance, drive, or make progress on a specific `.rhei.md` plan — the worker self-selects the next eligible task, works in its current state, logs subtask progress, advances state, finalizes with a result, and respects human-review gates.
argument-hint: <plan>
---

# Rhei Plan Worker (Unorchestrated)

Pick up a Rhei Plan and make progress on it without any external scheduler. The worker is driven by the plan itself: the state machine defines what is legal, `**Prior:**` edges define what is ready, and each state's `instructions` field defines what to do.

Do not repurpose this skill for plan authoring — use `rhei-plan-writer` for that. Structural edits to the plan are limited to logging subtask progress and updating child task states; all root-task state changes and assignments go through the CLI.

## Parameters

The skill takes a single required parameter:

- `<plan>` — path to the `.rhei.md` file for a Single-File Plan, or the `index.rhei.md` at the root of a Directory Workspace.

Invoke as `rhei-plan-worker <plan>` (e.g., `rhei-plan-worker examples/ci-heal/index.rhei.md`). If the caller does not supply `<plan>`, ask for it and stop — do not guess, scan the working directory, or pick from multiple candidates. The plan path is the worker's only source of truth for which plan to drive; all other inputs (state machine, task selection, transitions) are derived from the plan file and the state machine it declares.

## Operating Loop

Execute this loop until no eligible task remains or a human gate stops you:

1. **Validate the plan.** Run `rhei validate <plan>`. If it fails, stop and report — do not work a broken plan, and never skip validation.
2. **Load the state machine.** Run `rhei states` against the plan (or `rhei states --state-machine <path>`) to read allowed states, their `instructions`, and the transition graph. The CLI resolves the machine from the plan's `**States:**` field (or a sibling/workspace `states.yaml`); add `--json` for structured data. Fall back to reading the YAML directly, or [default-states.md](../rhei-plan-writer/references/default-states.md), if the CLI is unavailable.
3. **Read the plan.** Prefer `rhei render <plan> --format json --pretty` for structured access. Read the raw markdown too — you will edit it in place to log subtask progress.
4. **Claim the next task.** Run `rhei next <plan>`. It atomically selects the next claimable task (see *Task Selection*), writes `**Assignee:**` on it, and prints the task id, current state, and resolved instructions. If nothing is claimable, stop — the plan is done or blocked.
    - Use `rhei next <plan> --peek` first for a read-only look at what would be claimed.
    - If `rhei next` fails with a missing-artifact error, the current state requires an input file that does not exist — surface it; do not skip ahead.
5. **Work in the current state.** Follow the printed instructions verbatim. The state you are handed is where the work happens — `rhei next` does **not** advance state. Implement child task nodes in order, logging per child (see *Progress Logging*). If the ticket you were handed is itself a parent, its subtree is already terminal — the work is the parent's own (see *Parent tasks*).
6. **Advance state only when the workflow demands it.** Use `rhei transition` for intermediate hops; for terminal completion use `rhei complete` (see *State Transitions*).
7. **Finalize with `rhei complete`.** For a short plain result, run `rhei complete <plan> --task <id> --result "<one-line summary>"`. For multiline or Markdown-heavy text, prefer `--result-file <path>` or pipe a quoted heredoc to `--result-file -`, so backticks and quotes never enter a shell-expanded argument:

    ```bash
    rhei complete <plan> --task <id> --result-file - <<'EOF'
    Replayed onto `origin/main`.
    Preserved the review notes verbatim.
    EOF
    ```

    It transitions to the first reachable non-cancelled terminal, appends a `## Result` entry with the message to `runtime/results/<task-id>.md`, links that file via `> **Result:**`, and removes `**Assignee:**`. `<task-id>` is the project-qualified id (`plan.1` for `plan.rhei.md`); `rhei next` prints qualified ids, and `--task` accepts both the qualified id and the rhei-local shorthand. The input file is only the caller's message; it is not the worker-authored `runtime/results/<task-id>.md` artifact, which Rhei preserves and appends to. The message is not paperwork: no task enters a `final: true` state without one, so write what a reader six months from now needs.
8. **Stop at terminal or gating states.** `completed` is final in the built-in machine. Any state with `gating: true` in a custom machine halts the worker — do not transition out of it autonomously, and do not try to `rhei complete` through it.
9. **Loop.** Return to step 4. Re-read the plan on every pass; the markdown file is the single source of truth.

## Task Selection

Selection is owned by `rhei next` — do not re-implement it in prose. A task is claimable when:

1. Every descendant task node it has is in a terminal state. A leaf satisfies this trivially.
2. Every task in its `**Prior:**` list is in a terminal state (`final: true`; `completed` in the default machine).
3. The task has no `**Assignee:**` field.
4. The current state is neither terminal nor gating.
5. Every required `inputs` artifact declared on the current state exists.
6. No `poll:` deadline on the current state is still in the future — a polled state is not retried before its next attempt is due.
7. The current state is the machine's initial state. `rhei next` enters a ticket at the top of its workflow; one already partway through belongs to whoever holds it. Rules 3 and 7 are `rhei next`'s own narrowing — `rhei list --ready` reports the wider *ready set*, so a ticket it names can still be one `rhei next` declines.

When multiple tasks are claimable, `rhei next` picks the first in plan order — do not pre-rank by descendant count or other heuristics. Validation rejects plans where a child task lists its parent or another ancestor as `**Prior:**`; if you hit that failure, do not work around it by manually claiming the child — ask for or make a structural fix so the follow-up task is a sibling.

### Parent tasks

A task with child nodes is a task in its own right, not a container that fills up. Nothing advances it when its children advance: it carries its own state, its own work, and its own result. Rule 1 is the only thing that makes it different — it is not claimable while any descendant is still open, and it becomes claimable the moment the last one closes.

So a parent is worked exactly like a leaf, just later: `rhei next` hands it to you after its subtree is terminal — unless its state declares `execute_on:`, in which case it is handed to you *between* its children instead (see *Supervising parents*). You do whatever its state's instructions say (the integration, the verification, the write-up the children do not cover), and you finish it with `rhei complete`. Do not try to skip it, and do not read a parent left in `pending` after its children finished as a bug — it is work waiting for you.

Two refusals follow from the same rule, and neither is worked around by editing markdown:

- `rhei next --task <parent>` while a descendant is open fails, naming the open descendants and the ticket that is claimable instead. Claim that one.
- Any move into a terminal state — `rhei complete`, `rhei transition`, an orchestrated run — is refused on a task with a non-terminal descendant. Finish or cancel the descendants first; `cancelled` is terminal, so a deliberately abandoned child releases its parent.

### Supervising parents

A parent whose current state declares `execute_on:` is a **supervisor**: `rhei next` hands it to you *while* its subtree is open, at every checkpoint, not once at the end. Which moves reach you is that state's `execute_on` value — every finished child, every child transition, every finished descendant, or every descendant transition — and your prompt says which. The visit is short and it is not the parent's own work:

1. **Judge the checkpoints.** `rhei next` prints `## Checkpoints` — the descendants that moved since the last visit, each with what that step left behind — plus `## Supervising This Subtree`, which gives the paths below.
2. **Steer the next step.** Write a brief at `<root>/runtime/supervise/<task-id>.md` (read by every state of that descendant) or `<root>/runtime/supervise/<task-id>/<state>.md` (that state only). Append a child by editing the parent's own task file. Cancel a step the checkpoints made unnecessary with `rhei transition <plan> --task <child> --from <state> --to cancelled --result "<why>"` — a cancel does not have to satisfy the cancelled step's declared outputs, but it does have to say why.
3. **End the visit.** Run `rhei transition <plan> --task <P> --from <s> --to <s>` — the state's own self-loop. That one edge is the release: it lets the subtree run again and drops your `**Assignee:**`, so the next checkpoint is claimed afresh. Do not leave the visit by editing markdown, and do not hold the ticket across checkpoints.
4. **Finish it** with `rhei complete` only once every descendant is terminal; the machine's `openDescendants < 1` edge is what selects that.

While you hold a supervisor's visit, nothing beneath it runs. A `rhei next --task <child>` refusal that says the child is *held by supervisor Task \<P\>* is not your ticket to force — it names the supervisor to work instead, or the worker holding it and the `rhei release` that hands it back.

A resumable task (already carrying your own `**Assignee:**` from an interrupted prior session) is not re-claimable via `rhei next`. Resume it directly: read the current state, follow its instructions, and advance with `rhei transition` / `rhei complete` as usual.

## State Transitions

All root-task state transitions go through the CLI. Never edit a root `**State:**` line by hand.

```bash
rhei transition <plan> --task <id> --from <current-state> --to <target-state>
```

The CLI provides file locking; compare-and-swap (`--from` guards against racing workers — if another agent already transitioned the task, the command fails and prints the actual state); transition validation (illegal edges rejected before any write); artifact enforcement (required `outputs:` on the source state must exist); callbacks (`on_leave` / `on_enter` fire unless `--no-callbacks`); and the central ledger entry in `runtime/state-transitions.log`. On conflict, re-read the plan and re-claim with `rhei next` — do not retry the same transition blindly.

`rhei complete` stays the everyday finishing verb: it picks the first reachable non-cancelled terminal for you, so you do not have to name the state. Reach for `rhei transition --result` when you mean a *specific* terminal state `complete` will not choose — cancelling a ticket, or routing to a `failed`/`rejected` state:

```bash
rhei transition <plan> --task <id> --from <current-state> --to cancelled \
  --result "Abandoned: superseded by Task 7."
```

Either way the message is required. No task enters a `final: true` state without a non-empty `runtime/results/<task-id>.md` — the ticket's own record of why it ended there — so a bare `rhei transition` into a terminal state is refused, naming the file and the flag. `rhei transition` is not a way around a refusal either: it skips `**Prior:**` readiness as a deliberate escape hatch, but the descendants-first rule and the result rule are enforced on the same shared path for every verb, so it refuses a parent with an open descendant exactly as `rhei complete` does.

### Typical transitions for the default `rhei` machine

- `pending` → `completed` — the task is done.

If the loaded machine differs from the default, trust it over this list.

## Assignee Discipline

`**Assignee:**` is owned by the CLI — never edit it by hand. `rhei next` writes it when claiming; any move into a terminal state removes it, whether that move came from `rhei complete` or `rhei transition`; a supervising state's self-loop removes it too, because that edge ends the visit it was claimed for (see *Supervising parents*); every other non-terminal `rhei transition` leaves it untouched, so a long-running task keeps the same assignee across intermediate transitions in custom machines.

## Progress Logging

Log implementation progress by appending to each task node's body — do not invent new metadata fields.

- One short paragraph per leaf task node, written as you complete it (not batched at the end).
- State the concrete change: files touched, functions added, commands run to verify. Do not restate the task title; extend the description.
- When a task re-enters an earlier state in a custom machine, append a new paragraph describing the rework rather than rewriting history. If the machine uses counted visits, the re-rendered `**State:** <name>-<n>` line makes the visit explicit — do not edit that suffix by hand.

## Agent Review

When a task enters `agent-review`, the reviewer is a *different* mental mode, not a different person. The reviewer reads the task description, its child task nodes, and the diff actually produced; checks repository conventions (lint, format, test commands listed in [AGENTS.md](../../AGENTS.md) or the project's equivalent); records concrete findings as a new paragraph in the task body prefixed with `Review:` (one bullet per finding); and chooses the next edge — `agent-review-fix` (rework), `human-review` (needs human), or straight to terminal via `rhei complete` (pass). Never finalize a task whose tests or build fail.

## Editing Discipline

State transitions and progress logging are separate concerns:

- **Root task `**State:**`, `**Assignee:**`, and `> **Result:**`** are CLI-owned — see *State Transitions* and *Assignee Discipline*. Never edit them by hand.
- **Child task state transitions** — child nodes carry their own mandatory `**State:**` field. For child-only flows in the default machine you may update child states directly in the markdown as you finish each one. If the active machine's `node_policy` routes children through a stateful profile that uses the CLI, prefer `rhei transition` / `rhei complete` for children too.
- **Progress logging** — edit the markdown directly to append per-child progress.

- **New tickets** are `rhei new`, never a hand-written task block — see *Capturing Work You Did Not Come For*.

Run `rhei validate <plan>` after any direct edit; a failure means the edit is wrong — fix it before moving on. Preserve IDs, titles, `**Prior:**` edges, and frontmatter (structural changes belong to the plan writer). Do not reorder tasks, delete completed or cancelled tasks, remove `> **Result:**` links, or reformat unrelated sections.

## Capturing Work You Did Not Come For

Implementing a ticket turns up work that is not this ticket's: a bug next door, a follow-up, a cleanup the diff made obvious. Do not widen the ticket, and do not hand-write a task block into the plan — capture it and carry on:

```bash
rhei new "Null-cache panic in the avatar loader" --under basin \
  --description "Seen while working plan.3; reproduces on an empty cache."
```

`--under basin` is the project's unfiled inbox: it takes a ticket that has no rhei chosen yet, creates the basin on demand, and files nothing into your plan. It needs a Panta project around that plan, though: the basin exists only inside one ([§FS-rhei-new.3.5](../../../../docs/functional-spec/rhei-new.spec.md#35-the-basin-needs-a-project)). Against a lone `.rhei.md` the command refuses and points at `rhei init`, which is not yours to run — report the capture in your result instead, with the title and the description you would have filed, and carry on with the ticket you claimed. When the work clearly belongs to a rhei that exists, name it instead (`--under auth`), and add `--prior <id>` when it genuinely cannot start before something else. `rhei new` allocates the id under a lock, validates the result, and rolls the write back if it broke anything, so capturing is safe mid-task.

Capture is the only creation a worker does. Creating a *rhei*, and running an orchestrator, are the human's call. This is also not a way to move work off the ticket you were handed: capture what is genuinely out of scope, and finish what you claimed.

## Stopping Conditions

Stop the loop and report when any of these is true:

- `rhei next` prints no claimable task (everything terminal, blocked on a gating state in a custom machine, or awaiting priors).
- A task reaches a gating state in a custom machine — stop *that* task but keep working independent branches of the DAG by re-running `rhei next`.
- `rhei validate` fails after an edit you cannot explain.
- A CAS conflict on `rhei transition` / `rhei complete` cannot be resolved by re-claiming (another worker is actively driving the same plan and there is no other eligible work).
- The task requires information or access the worker does not have (credentials, external decisions, missing input artifact) — stop and ask.

When stopping, print a short summary: which tasks advanced, which task is blocked and why, and what the next human action is.

## Unorchestrated Mode Notes

"Unorchestrated" means **no** external process tells the worker what to do next. Consequences:

- The worker reads the plan on every pass; treat the markdown file(s) as the single source of truth and do not cache state across passes beyond the current conversation.
- Multiple workers can safely operate on the same plan: `rhei next` acquires a file lock and `rhei transition` / `rhei complete` use compare-and-swap — the loser of a race re-reads and re-claims. Do not batch multiple task transitions into one command — one task, one transition, then re-read.
- Directory Workspaces behave the same, except the lock scope is per-task-file rather than per-plan — which is why the format exists.

This worker flow is distinct from `rhei run` (agent mode): under `rhei run`, the orchestrator spawns a subprocess for each claimed state and performs the transition after the subprocess exits; the worker skill does the work manually instead.

## Missing Information Handling

If the plan path, state machine, or a task description is ambiguous or missing required detail:

- Ask the user before editing, and never invent prerequisites, states, or transitions to unblock selection.
- If a task description is too thin to implement, surface it and ask for clarification — do not silently expand scope.
