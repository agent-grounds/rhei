# The Parts of a Feature: Flat Tasks, Not a Folder

One of the paired examples of
[§FS-rhei-shape](../../../docs/functional-spec/rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose):
the same work authored two ways, both valid and both runnable with the mock
agent, so the difference you see is the shape and nothing else.

## The situation

Users can upload an avatar and see it on their profile page. The work is three
parts — a column to keep it in, an endpoint to upload it through, and the page
that shows it — each building on the one before. Nobody reviews "the avatar
feature" as a thing of its own: each part is read, and found again later, by
itself.

## The two shapes

Both run one machine, `states.yaml`, with a single `work` state. Only whether
the parts have a parent differs.

**Flat** — three siblings chained with `**Prior:**`
(`flat/tasks/01-avatar-upload.md`):

```markdown
### Task 1: Add the avatar column
### Task 2: Add the upload endpoint
**Prior:** Task 1
### Task 3: Show the avatar on the profile page
**Prior:** Task 2
```

**Nested** — the same three under a parent named for the feature
(`nested/tasks/01-avatar-upload.md`):

```markdown
### Task 1: Avatar upload
The avatar feature. The three parts below are its work.

#### Task 1.1: Add the avatar column
#### Task 1.2: Add the upload endpoint
**Prior:** Task 1.1
#### Task 1.3: Show the avatar on the profile page
**Prior:** Task 1.2
```

The parent's body is the tell: it can only say that the parts below are its
work, and its result can only say that they are done.

## What the next task sees

A task added after the nested run is told this under `## Plan History`
(`rhei next --peek`):

<!-- rhei:plan-history nested -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task nested.1: Avatar upload — completed — The three parts below are done. — 3 subtasks: 3 completed
```
<!-- /rhei:plan-history nested -->

Everything a later task could use — the column's name, the endpoint's path —
is folded under a line that says nothing. The flat run tells the same task:

<!-- rhei:plan-history flat -->
```text
Finished work on the way from the plan's roots to this task, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root; a folded subtree: `rhei list --parent <id>`.

- Task flat.1: Add the avatar column — completed — Added the nullable `avatar_url` column to `users`, with migration 0042.
- Task flat.2: Add the upload endpoint — completed — Added `PUT /users/{id}/avatar`; it stores the image and writes its URL to `avatar_url`.
- Task flat.3: Show the avatar on the profile page — completed — The profile page shows the avatar, and the initials when there is none.
```
<!-- /rhei:plan-history flat -->

## What the person sees

The nested run's console task tree folds the finished parent into its line
([§FS-rhei-run-report.3.2](../../../docs/functional-spec/rhei-run-report.spec.md#32-task-tree)):

<!-- rhei:task-tree nested -->
```text
   4 tasks · source order
  ✓ nested.1                   completed   agent  <t> — 3 subtasks: 3 completed
```
<!-- /rhei:task-tree nested -->

The flat run's tree is one row per part:

<!-- rhei:task-tree flat -->
```text
   3 tasks · source order
  ✓ flat.1                     completed   agent  <t>
  ✓ flat.2                     completed   agent  <t>
  ✓ flat.3                     completed   agent  <t>
```
<!-- /rhei:task-tree flat -->

The nested shape also costs the parent an identity, a result file and an agent
visit of its own, spent on writing *the three parts below are done*.

## The ruling

**Flat**: nothing is owed above the three parts, so the parent neither steers,
integrates nor speaks for them and they are siblings chained with `**Prior:**`
([§FS-rhei-shape.3.1](../../../docs/functional-spec/rhei-shape.spec.md#31-the-default-is-flat),
[§FS-rhei-shape.3.2](../../../docs/functional-spec/rhei-shape.spec.md#32-the-three-reasons-a-task-has-children)).

## When the other shape is right anyway

When somebody reads the integration — an end-to-end check that uploads an
avatar and sees it on the page, or a release note for the feature as a whole —
the parent has a deliverable made out of its children's, and the three parts
are its subtasks
([§FS-rhei-shape.4](../../../docs/functional-spec/rhei-shape.spec.md#4-the-decision-table)).
Its body then says what it integrates, not that the parts are its work.

## Run it

```bash
cargo xtask examples run shape-parts-of-a-feature-flat
cargo xtask examples run shape-parts-of-a-feature-nested
```

Each shape keeps its machine in the `states.yaml` beside its `index.rhei.md`,
so a copy of the directory runs with `rhei run <copy> --no-tui` and no
`--state-machine` flag. `<t>` above stands for a duration, which differs on
every run.
