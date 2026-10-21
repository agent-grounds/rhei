### Task batch: Apply the batch task to every target
**State:** report

Each target below is a subtask of this ticket and works in its own git
worktree. Once every one of them has finished, report on the whole batch.

#### Task batch.cli: Apply the batch task in crates/rhei-cli
**State:** prepare-worktree

**Target path:** `crates/rhei-cli` — make all edits inside this target's own worktree
(`runtime/worktrees/cli` on branch `docs-pass/cli`). Do not touch
other targets' paths; they run in parallel in their own worktrees.

The task to apply to `crates/rhei-cli`:

Add a crate-level module doc comment (a `//!` block at the top of the crate
root) summarizing the crate's public responsibility in two or three
sentences. Do not change any other code or behavior.


#### Task batch.core: Apply the batch task in crates/rhei-core
**State:** prepare-worktree

**Target path:** `crates/rhei-core` — make all edits inside this target's own worktree
(`runtime/worktrees/core` on branch `docs-pass/core`). Do not touch
other targets' paths; they run in parallel in their own worktrees.

The task to apply to `crates/rhei-core`:

Add a crate-level module doc comment (a `//!` block at the top of the crate
root) summarizing the crate's public responsibility in two or three
sentences. Do not change any other code or behavior.


#### Task batch.validator: Apply the batch task in crates/rhei-validator
**State:** prepare-worktree

**Target path:** `crates/rhei-validator` — make all edits inside this target's own worktree
(`runtime/worktrees/validator` on branch `docs-pass/validator`). Do not touch
other targets' paths; they run in parallel in their own worktrees.

The task to apply to `crates/rhei-validator`:

Add a crate-level module doc comment (a `//!` block at the top of the crate
root) summarizing the crate's public responsibility in two or three
sentences. Do not change any other code or behavior.


