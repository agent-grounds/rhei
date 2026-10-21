### Task batch: Apply the batch task to every target
**State:** report

Each target below is a subtask of this ticket and works in its own git
worktree. Once every one of them has finished, report on the whole batch.

{% for t in targets -%}
#### Task batch.{{t.id}}: Apply the batch task in {{t.path}}
**State:** prepare-worktree

**Target path:** `{{t.path}}` — make all edits inside this target's own worktree
(`{{worktree_root}}/{{t.id}}` on branch `{{branch_prefix}}/{{t.id}}`). Do not touch
other targets' paths; they run in parallel in their own worktrees.

The task to apply to `{{t.path}}`:

{{task}}

{% endfor -%}
