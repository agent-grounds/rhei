# Rhei: Parallel Worktree Batch Example

## Overview

The same task runs once per target, each inside its own git worktree on its own
branch, so the targets advance concurrently with `rhei run --parallel N` without
several agents editing one checkout. Each task walks
`prepare-worktree → work → integrate → completed` independently.

## Shared task

Add a crate-level module doc comment (a `//!` block at the top of the crate
root) summarizing the crate's public responsibility in two or three
sentences. Do not change any other code or behavior.


## Targets
- `cli` → `crates/rhei-cli`  (worktree `runtime/worktrees/cli`, branch `docs-pass/cli`)
- `core` → `crates/rhei-core`  (worktree `runtime/worktrees/core`, branch `docs-pass/core`)
- `validator` → `crates/rhei-validator`  (worktree `runtime/worktrees/validator`, branch `docs-pass/validator`)

## Agent

`claude-code[yolo]:anthropic:claude-opus-4-7` performs every target's worktree setup, edits, and commit.
