# FS-rhei-authoring: Rhei Plan Language Usage Guide

This guide shows how to author Rhei Plan documents that conform to the
[Rhei Language Specification](rhei-plan-language.spec.md). It focuses on practical
patterns rather than formal grammar — consult the spec for precise rules.

## 1. A Minimal Plan

The smallest valid plan declares a title, a `## Tasks` section, and at least
one task with a `**State:**` field:

```markdown
# Rhei: First Plan

## Tasks

### Task 1: Set up the repository
**State:** pending
```

Save this file with a `.rhei.md` extension, then validate it:

```bash
rhei validate my-plan.rhei.md
```

The file stem becomes the rhei id that prefixes every ticket id in command
output (`my-plan.rhei.md` → tickets `my-plan.1`, `my-plan.2`, ...), so choose
it like an identifier: start with a letter, then letters, digits, `_`, or `-`.

When no `**States:**` field is declared, a standalone plan uses the built-in
`rhei` state machine. Inside a Panta Project, a rhei that omits `**States:**`
may inherit the project default from `index.panta.md`.

## 2. Authoring Workflow

A typical plan grows in three passes:

1. **Outline** — write the `# Rhei:` title and draft `## Overview`,
   `## Context`, or similar prose sections that motivate the work.
2. **Break down** — add a `## Tasks` section and enumerate tasks with
   numeric or named ids. Give every task a `**State:**` (usually `draft`
   or `pending`) and a short description.
3. **Refine** — add `**Prior:**` dependencies, split tasks into child
   task nodes, and transition states as work progresses.

Keep content sections before `## Tasks`. Everything after `## Tasks` is
parsed as task structure.

## 3. Tasks and Child Tasks

### 3.1. Numeric vs named tasks

Numeric task ids (`### Task 1:`) and named task ids (`### Task setup:`) may
both contain child tasks. Use numeric segments when sibling order matters, and
use named segments for durable conceptual anchors that other tasks depend on.

```markdown
### Task infra: Provision cloud resources
**State:** pending

### Task 1: Deploy application
**State:** pending
**Prior:** Task infra
```

### 3.2. Child task nodes

Root tasks live at `###` (H3). Child tasks are declared at the next heading
level (`####`, H4), their children at `#####` (H5), and so on. A child task
is a full task node — it carries its own `**State:**` line and may declare
`**Prior:**` dependencies just like a root task.

Child ids extend the parent id by exactly one segment, joined with a dot. Any
segment may be numeric or named. So children of `Task 2` are `Task 2.1`,
`Task 2.2`, …; a child of `Task api.cache` is `Task api.cache.fix`. Numeric
segments are ordered; named segments are arbitrary labels. Duplicate sibling
ids under one parent are rejected.

```markdown
### Task 2: Implement login flow
**State:** pending

#### Task 2.1: Wire OAuth callback
**State:** pending

#### Task 2.2: Persist session tokens
**State:** pending
**Prior:** Task 2.1
```

### 3.3. Depth and node kinds

By default a plan permits a root task plus one level of children, using
only the `Task` node kind. Plans that need deeper trees or additional
kinds (for example `Bug`) may declare a `structure` block in the plan
frontmatter:

```yaml
structure:
  maxLevels: 4
  nodeKinds: [task, bug]
```

`maxLevels` counts from the root (a root task alone has `maxLevels: 1`;
root plus direct children is `maxLevels: 2`). Validation rejects nodes
that exceed the declared depth or use a heading kind not listed in
`nodeKinds`. When `structure` is omitted, the defaults are
`maxLevels: 2, nodeKinds: [task]`.

In a directory workspace, the `structure` block is declared once in
`index.rhei.md` and applies to every markdown task file under `tasks/`.
Task files do not redeclare structure locally; loaders and validators parse
them using the workspace index structure.

## 4. Metadata

`**State:**` is mandatory and must be the first line after the task header.
`**Prior:**` is optional and, when present, must immediately follow
`**State:**`.

```markdown
### Task 3: Ship release notes
**State:** agent-review
**Prior:** Task 1, Task 2
```

### 4.1. State values with spaces

Single-word states are written bare. Multi-word states must be wrapped in
backticks:

```markdown
**State:** `in review`
```

The state value must be defined in the active states file — multi-word
states typically require a custom states file such as
[`examples/states-with-spaces.yaml`](../../examples/states-with-spaces.yaml).

### 4.2. Dependencies

List prerequisites by id, separated by commas. References must resolve to
tasks defined in the same document, and the dependency graph must stay
acyclic. A task must not list itself or any ancestor as a prerequisite: a
child task cannot use its parent as `**Prior:**`. If follow-up work must wait
for a completed parent task, write that follow-up as a top-level sibling and
point `**Prior:**` at the completed task.

```markdown
**Prior:** Task 1, Task design, Task 4
```

```markdown
#### Task fetch-prs.ci-failure-5227: Triage CI failure
**State:** pending
**Prior:** Task fetch-prs    ← Invalid when `fetch-prs` is this task's parent
```

`**Prior:**` is the markdown authoring form. SDKs expose the same data under
idiomatic names — `task.metadata.dependsOn` (TypeScript/JavaScript, Java,
CLI JSON) or `task.metadata.depends_on` (Python). See
[Transitions Specification — Naming conventions](rhei-transitions.spec.md#21-naming-conventions)
for the full table.

### 4.3. Read Exclusions

Use `**Excludes:**` when a task must not receive a source payload, for example
when several reviewers must form independent views:

```markdown
**Excludes:** checkout=notes/private.md, artifact=runtime/reviews/, review.2:statement
```

Use `checkout=` for repository/worktree sources, `artifact=` for the owning
rhei's runtime tree, and an export reference for another task's declared
export. A trailing `/` excludes a directory recursively. Do not both consume
and exclude one export, and do not exclude required invocation inputs; validate
the whole project so cross-rhei references resolve. The exact grammar and the
difference between composition-only and filesystem enforcement are in
[§FS-rhei-plan-language.3.13](rhei-plan-language.spec.md#313-task-read-exclusions) and [§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition).

### 4.4. Checked task exports

Write a task handoff as two explicit halves: the producer names the file with
`**Provides:**`, and the consumer names both the producer in its own direct
`**Prior:**` and the data it reads in `**Consumes:**`:

```markdown
### Task 1: Design the API
**State:** completed
**Provides:** api-contract

### Task 2: Build the client
**State:** pending
**Prior:** Task 1
**Consumes:** 1:api-contract
```

The direct prior is required even when another dependency path already reaches
Task 1; `**Consumes:**` never creates ordering. The producer writes nonblank
text to `runtime/exports/<task-id>/<name>.md` before successful completion, and
the consumer is not spawned until that file is available under the producer's
execution root. To upgrade an older plan, add the direct prior, correct any
unmatched export name, and make the producer write non-whitespace content
([§FS-rhei-plan-language.3.12](rhei-plan-language.spec.md#312-task-exports)).

## 5. Using a Custom State Machine

To reuse one state machine across plans, declare it on the line directly
after the `# Rhei:` title:

```markdown
# Rhei: Content Refresh
**States:** content-workflow
```

The `name` field in the resolved YAML file must match this value. By
default, Rhei resolves that file automatically from `states.yaml` next to
the plan (single-file plans) or at the workspace root (directory
workspaces). Pass `--state-machine <path>` only when you want to override
that automatic lookup:

```bash
rhei validate plans/content-refresh.rhei.md
```

See the [States Specification](rhei-states.spec.md) for the states file format.

State `effort` lets phases reuse one execution target and permission mode while
choosing how much reasoning each phase requests. It is optional; omitting it
leaves the selected agent's arguments and defaults unchanged:

```yaml
states:
  check:
    target: codex[yolo]:openai:gpt-5.6-sol
    effort: low
  implement:
    target: codex[yolo]:openai:gpt-5.6-sol
    effort: high
```

The canonical values are `off`, `minimal`, `low`, `medium`, `high`, `xhigh`,
and `max`. An agent profile translates only the values it declares; profiles
without an effort mapping ignore the field. See
[§FS-rhei-states.5](rhei-states.spec.md#5-agent-field) and
[§FS-rhei-agents.1.1.2](rhei-agents.spec.md#112-agents).

Inside a Panta project, an explicit declaration also selects where the
definition is resolved. For example, if `index.panta.md` declares `alpha` and
the project-root `states.yaml` defines `surveying → signed-off`, a `billing`
member that declares `**States:** alpha` beside its own matching `states.yaml`
uses that member-local definition — for example `drafting → filed`. Remove the
member's `**States:**` line to inherit the already-resolved project definition
instead. The same-name declaration inherits the default's *name*, not its
definition file ([§FS-rhei-plan-language.1.3](rhei-plan-language.spec.md#13-state-machine-resolution)).

## 6. Common Pitfalls

- **Missing `**State:**`** — every task header must be followed by a
  `**State:**` line.
- **Metadata out of order** — `**Prior:**` must come after `**State:**`,
  not before.
- **Child id does not extend parent id** — a child under `Task api` must use an
  id such as `api.cache`, not `cache` or `1.cache`.
- **Cross-plan references** — `**Prior:**` only resolves within one
  document; to model cross-plan dependencies, keep those tasks in the
  same file.
- **Unknown state** — validation fails if a `**State:**` value is not
  declared in the active states file.
- **Duplicate ids** — two tasks with the same id (numeric or named) are
  rejected, as are duplicate child task ids under one parent.

## Worked Examples

The [`examples/`](../../examples/) directory contains end-to-end plans that
exercise the patterns above:

- [`release-automation.rhei.md`](../../examples/release-automation.rhei.md) —
  mixed numeric and named task ids with fenced code inside a child task.
- [`human-review-loop.rhei.md`](../../examples/human-review-loop.rhei.md) —
  multi-state workflow with chained dependencies.
- [`escaped-state-values.rhei.md`](../../examples/escaped-state-values.rhei.md) —
  multi-word state values paired with a custom states file.

## 7. Advancing Task States with `rhei transition`

While `**State:**` values can be edited by hand, the `rhei transition`
command provides an atomic, validated way to advance a task's state:

```bash
rhei transition my-plan.rhei.md --task 1 --from pending --to in-progress
```

The command:

1. **Acquires the plan's canonical sibling sidecar** as its sole writer lock,
   leaving the replaceable plan file itself unlocked.
2. **Reads the current state** from the current plan pathname after acquiring
   that sidecar.
3. **Compare-and-swap** — if the task's current state does not match
   `--from`, the command fails with a conflict error. This prevents two
   agents from claiming the same task.
4. **Validates the transition** against the state machine — illegal
   transitions are rejected before any write occurs.
5. **Writes the new state** to the markdown and releases the sidecar after all
   callback, result, ledger, finalization, or rollback work finishes.

### 7.1. Flags

| Flag             | Required | Description                                     |
| ---------------- | -------- | ----------------------------------------------- |
| `--task <id>`    | Yes      | Task id (numeric or named) to transition        |
| `--from <state>` | Yes      | Expected current state (compare-and-swap guard) |
| `--to <state>`   | Yes      | Target state                                    |
| `--json`         | No       | Emit result as JSON instead of plain text       |

On success the command prints the updated state. On conflict (another
agent already transitioned the task) it exits non-zero with a message
indicating the actual current state. Agents should re-read the plan and
re-select when this happens.

### 7.2. Parallel safety

When multiple agents work on the same plan, `rhei transition` is the
coordination primitive. Because the `--from` flag acts as a
compare-and-swap guard, only one agent can win a race on the same task —
the loser gets a clean error and picks a different task. See
[How Rhei Is Used — Pattern 3](rhei-usage.spec.md) for the full
parallel-workers pattern.

Sidecars are empty files named by appending `.lock` to the exact destination
filename, beside that destination. Rhei retains them and any directories needed
to keep their path stable after success, rollback, failed creation, and dry
run. Do not delete them to clear a suspected lock: only the operating-system
lock on an open handle denotes a live owner. When upgrading writers that share
a plan directory, stop all older writers, upgrade every writer, and only then
resume. Existing sidecars are reused; live mixed-version writing is unsupported
and a sidecar's presence does not prove the upgrade is complete.

## 8. Next Steps

- Read the [Plan Language Specification](rhei-plan-language.spec.md) for the
  formal grammar and semantic constraints.
- Browse the [States Specification](rhei-states.spec.md) to define project-
  specific workflows.
- Use `rhei render --format github` to produce review-friendly views of
  a plan, or `--format progress` for a terminal overview.

### 8.1. Progress format

`--format progress` renders the plan as a compact ANSI terminal report intended
for quick navigation.  Its structure is:

```text
Rhei: <title>
<SectionTitle>:
  <line 1 of section content>
  <line 2 of section content>
  …
* Task <id>: <title>  [STATE]
  - Prior: Task <id>, …          (omitted when empty)
  - <id>.<sub>: <child task title>
  …
```

Content sections (e.g. `## Overview`, `## Success Metrics`) are rendered in
full, with each non-blank line indented by two spaces under the section title.
No content is truncated.  This ensures the progress view is a useful navigation
surface even for plans with substantial product context.
