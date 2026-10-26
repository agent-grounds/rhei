# changeset-review - example

A pre-rendered instantiation of the [`changeset-review`](../../crates/rhei-cli/templates/changeset-review/)
template used as a smoke test that the template produces a valid workspace.
The source template retains this public example while the independently
mountable `code-review` and `fix` blocks expose the reusable boundary.

Inputs used when rendering:

| Input | Value |
|---|---|
| `change_ref` | `PR#42` |
| `review_targets` | `["claude-code[yolo]:anthropic:claude-opus-4-7", "codex[xhigh]:openai:gpt-5.5"]` |
| `validation_targets` | `["claude-code[yolo]:anthropic:claude-opus-4-7", "codex[xhigh]:openai:gpt-5.5"]` |
| `proposal_targets` | `["claude-code[yolo]:anthropic:claude-opus-4-7", "codex[xhigh]:openai:gpt-5.5"]` |
| `review_focus` | `["performance", "security", "concurrency"]` |
| `smart_target` | `codex[xhigh]:openai:gpt-5.5` (default) |
| `fix_prepare` | `worktree` |
| `fix_commit` | `pr` |

Validate from the repository root:

```bash
cargo run -p rhei-cli -- validate examples/changeset-review-example
```

Check the orchestrator shape without spawning agents:

```bash
cargo run -p rhei-cli -- run examples/changeset-review-example --dry-run
```

To regenerate this example from the template:

```bash
rm -rf examples/changeset-review-example
cargo run -p rhei-cli -- instantiate crates/rhei-cli/templates/changeset-review \
  --values crates/rhei-cli/templates/changeset-review/.example-values.yaml \
  --output examples/changeset-review-example
```

Restore this README and `instantiation-values.yaml` after regenerating.

The example ships `.agent-grounds/rhei/settings.json`, which declares two Codex
profiles, `codex` and `fix-codex`, each with one mode, `xhigh`:
`workspace-write` with network on, under the roots Rhei grants at launch, and
`model_reasoning_effort="xhigh"`. Each agent state bounds itself with
`agent_timeout: 1h`. Claude Code remains included as a second default reviewer,
but Rhei does not currently expose a Claude reasoning-effort flag.
