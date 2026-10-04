# Rhei: CI Watch and Heal

---
metadata:
  tasks:
    heal-ci:
      branch: feature/retry-cleanup
---

## Overview

Watch GitHub CI for a branch and heal it in place: poll the CI status on a
fixed interval, and when any job fails, let an agent write a fix, push it,
and resume polling. Two nested loops:

- **Inner (poll)** — `ci-watch` runs a deterministic status check every
  `poll.interval` and stays in the state while jobs are still running. The
  `--parallel` slot is released between attempts so other work can advance.
  Bounded by `poll.max_attempts`.
- **Outer (fix)** — on a failing verdict, the agent produces one focused
  fix, `push-fix` commits and pushes, and the task re-enters `ci-watch`
  with fresh poll counters. Bounded by `visits` on `analyze-and-fix`.

The happy path: `ci-watch` loops a few times → exits with `0` → `heal-done`.

The unhappy path: `ci-watch` exits with `1` → `analyze-and-fix` → `push-fix`
→ back to `ci-watch`. Either the new push turns CI green (`heal-done`), or
the outer loop exhausts (`fix-exhausted`).

## Status-check contract (`.rhei/gh-ci-status.py`)

A small script on the consuming repo side is expected to encode the tri-state
verdict via exit code:

| Exit | Meaning                               |
|-----:|---------------------------------------|
| `0`  | Every required check passed.          |
| `1`  | At least one required check failed.   |
| `75` | Checks are still running (EX_TEMPFAIL); retry after `poll.interval`. |

It must also write a JSON report to the path declared in the `ci-report`
output. Suggested contents:

```json
{
  "branch": "feature/retry-cleanup",
  "sha": "abc1234",
  "jobs": [
    { "name": "test-rust", "status": "failure", "log_url": "..." },
    { "name": "lint",      "status": "success" }
  ]
}
```

The self-loop transition (`ci-watch → ci-watch` on exit `75`) is what marks
this a time-triggered state. See `docs/functional-spec/rhei-states.spec.md` §Polling
States and `docs/functional-spec/rhei-run.spec.md` §Polling States for the semantics.

## Spawn-child variant

The brief above keeps one task walking the whole loop — the agent applies
the fix inline in `analyze-and-fix`. A richer variant is to have the agent
*spawn one fix task per root-cause failure* in that state. Two caveats:

1. **Direction of `Prior:`**. Children cannot block the parent with `Prior:`
   — that edge points the wrong way. Instead, the agent emits a *new peer*
   task (not a child) that will re-enter `ci-watch` and whose `**Prior:**`
   lists every freshly spawned fix task. The current task then transitions
   to `heal-done` (its job is done: hand-off is encoded in the plan). The
   new peer picks up polling once every fix task is terminal.
2. **Fix-task shape**. Each fix task has its own two-state flow:
   `fix-and-push → completed` (agent state followed by a deterministic
   push program). They share no state with the poll loop.

This variant trades a longer plan for easier parallel fixing and clearer
audit trails per failure. The inline form is simpler to reason about and
composes better with a single-agent `--parallel 1` workspace.

## Running

Once the `poll:` block is wired through `rhei run` (see
`docs/functional-spec/rhei-states.spec.md` §Polling States):

```bash
rhei run examples/ci-heal
```

The initial task (`heal-ci`) starts in `ci-watch`. The `branch` value under
`metadata.tasks.heal-ci` above feeds the `{meta.branch}` template in the state
definitions. Task metadata lives in this file's `metadata:` block, keyed by the
rhei-local task id — a `metadata:` block written inside a task body is prose and
is never read.

## Notes

- `ci-watch` and `push-fix` are program states; they're bounded by
  `program_timeout` and produce exit-code-driven transitions. No agent is
  spawned for them.
- `analyze-and-fix` writes one `fix-summary` per visit
  (`{task_id}.{visit_count}.md`) so retries don't overwrite history.
- Because `ci-watch` is in the inner loop, its `stateVisits` counter is
  *not* the outer-loop counter — those are scoped per state. The outer loop
  reads `visits` / `visitCount` on `analyze-and-fix`.
- `push-fix` repeats `visits: 5` for the same reason: `{visit_count}` is
  per-state and falls back to `1` wherever `visits` is absent, so a consumer
  of a per-visit artifact has to be counted too. Keep the two budgets equal.
