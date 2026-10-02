# An Epic of Issues: Flat Siblings, Not Children of the Epic

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

An epic, *Support Windows paths*, groups three issues: separators in plan
paths, quoting in program commands, and a drive letter in `--state-machine`.
Each is fixed on its own, in any order, and each fix is the whole of what its
issue asked. The epic is how the tracker groups them; nobody decides anything
between them, merges them, or reports on them as one.

Nothing outside the plan is touched, and no forge is read: each issue's number
is in its title, so there is no stand-in file.

## The two shapes

Both run one machine, `states.yaml`, with one agent state. Only whether the
issues sit under the epic differs.

**Flat** — the three issues are siblings (`flat/tasks/01-windows-paths.md`):

```markdown
### Task 1: Normalize separators in plan paths (issue 101)
### Task 2: Quote paths in program commands (issue 102)
### Task 3: Accept a drive letter in --state-machine (issue 103)
```

**Nested** — the three issues are children of a parent named for the epic
(`nested/tasks/01-windows-paths.md`):

```markdown
### Task 1: Support Windows paths
#### Task 1.1: Normalize separators in plan paths (issue 101)
#### Task 1.2: Quote paths in program commands (issue 102)
#### Task 1.3: Accept a drive letter in --state-machine (issue 103)
```

## What the next task sees

A task added after the flat run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history flat -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task flat.1: Normalize separators in plan paths (issue 101) — completed — Plan paths are compared with `/` separators on every platform.
- Task flat.2: Quote paths in program commands (issue 102) — completed — A program command with a space in its path now runs on Windows.
- Task flat.3: Accept a drive letter in --state-machine (issue 103) — completed — `--state-machine C:\plans\states.yaml` resolves instead of being read as a URL.
```
<!-- /rhei:plan-history flat -->

Each fix, in its own words. The nested run tells the same task:

<!-- rhei:plan-history nested -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task nested.1: Support Windows paths — completed — The three Windows-path issues below are done. — 3 subtasks: 3 completed
```
<!-- /rhei:plan-history nested -->

The one line it gets is the parent's, and the parent had nothing of its own to
say: the fixes are behind `rhei list --parent nested.1`.

## What the person sees

The flat run's console task tree
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree))
gives each issue its row:

<!-- rhei:task-tree flat -->
```text
   3 tasks · source order
  ✓ flat.1                     completed   agent  <t>
  ✓ flat.2                     completed   agent  <t>
  ✓ flat.3                     completed   agent  <t>
```
<!-- /rhei:task-tree flat -->

The nested run folds them under a row whose own work was writing that
sentence:

<!-- rhei:task-tree nested -->
```text
   4 tasks · source order
  ✓ nested.1                   completed   agent  <t> — 3 subtasks: 3 completed
```
<!-- /rhei:task-tree nested -->

## The ruling

**Flat**: a parent is a ticket, not a folder, and the epic's parent fails all
three of the reasons a task has children
([§FS-rhei-shape.3.2](../../../docs/functional-spec/rhei-shape.spec.md#32-the-three-reasons-a-task-has-children)).
It does not **steer** — nothing is decided between the issues. It does not
**integrate** — its deliverable is not made out of theirs. And it cannot
**speak for** them, because its body could only ever say *"the children below
are done"*. So the issues are flat siblings, the default
([§FS-rhei-shape.3.1](../../../docs/functional-spec/rhei-shape.spec.md#31-the-default-is-flat)):
grouping them for a reader is not a reason for a child, since the tracker's
epic and `rhei list` already give depth on demand.

## When the other shape is right anyway

When the epic owes something of its own — a release note made out of the
three fixes, or a supervisor that decides the next issue from what the last
one found — the parent integrates or steers, and the issues are its subtasks.
And when each issue carries its own lifecycle — a claim, an implementation,
review rounds and a ship — each issue is a rhei of its own, one plan per
issue, rather than a task of an epic's plan.

## Run it

```bash
cargo xtask examples run shape-epic-of-issues-flat
cargo xtask examples run shape-epic-of-issues-nested
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run.
