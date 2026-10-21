# parallel-worktrees — example

A pre-rendered instantiation of the
[`parallel-worktrees`](../../crates/rhei-cli/templates/parallel-worktrees/)
template. This is the canonical reference for two patterns at once:

- **Parallel execution** — three independent subtasks of one `batch` ticket
  (one per target, no `**Prior:**` between them) in `concurrent: true` states,
  so `rhei run --parallel 3` advances all three in a single pass. They are
  subtasks rather than top-level tickets because all three live in one tasks
  file, and a plan whose every top-level ticket is in one file runs
  sequentially whatever `--parallel` says. Once all three have finished, the
  `batch` ticket writes one report of every branch.
- **Per-task git worktrees** — each task creates its own worktree at
  `runtime/worktrees/<task_id>` on branch `docs-pass/<task_id>`, so concurrent
  agents never edit the same checkout.

## Inputs used

The full input set is checked in at `instantiation-values.yaml`. The `targets`
array drives the fan-out — one subtask per entry:

```yaml
targets:
  - { id: cli,       path: crates/rhei-cli }
  - { id: core,      path: crates/rhei-core }
  - { id: validator, path: crates/rhei-validator }
branch_prefix: docs-pass
worktree_root: runtime/worktrees
```

## Validate & see the parallelism

```bash
rhei validate examples/parallel-worktrees-example

# All three targets scheduled in one pass (concurrent: true + --parallel 3):
rhei run examples/parallel-worktrees-example --parallel 3 --dry-run

# Without --parallel the same plan runs one target per pass — compare the output:
rhei run examples/parallel-worktrees-example --dry-run
```

The `--parallel 3` dry-run shows three `prepare-worktree → work` transitions in
pass 1; the default run shows one, with the rest deferred. That difference is
the whole point of `concurrent: true`.

## Regenerate

```bash
rm -rf examples/parallel-worktrees-example
rhei instantiate crates/rhei-cli/templates/parallel-worktrees \
  --values crates/rhei-cli/templates/parallel-worktrees/.example-values.yaml \
  --output examples/parallel-worktrees-example
```

After regenerating, restore this README and `instantiation-values.yaml` from the
checked-in copy if you want to keep the example metadata alongside the rendered
workspace.
