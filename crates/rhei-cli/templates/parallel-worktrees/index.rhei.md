# Rhei: {{batch_title}}

## Overview

The same task runs once per target, each inside its own git worktree on its own
branch, so the targets advance concurrently with `rhei run --parallel N` without
several agents editing one checkout. Each task walks
`prepare-worktree → work → integrate → completed` independently.

## Shared task

{{task}}

## Targets

{%- for t in targets %}
- `{{t.id}}` → `{{t.path}}`  (worktree `{{worktree_root}}/{{t.id}}`, branch `{{branch_prefix}}/{{t.id}}`)
{%- endfor %}

## Agent

`{{agent}}` performs every target's worktree setup, edits, and commit.
