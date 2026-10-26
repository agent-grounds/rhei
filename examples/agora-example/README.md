# agora — example

A pre-rendered instantiation of the
[`agora`](../../crates/rhei-cli/templates/agora/)
template. This example is a smoke test for the workflow shape: one splitter task
starts in `split-points`; at runtime it creates point tasks and a final
aggregation task.

## Inputs Used

The full input set is checked in at `instantiation-values.yaml`. The example
discussion asks the agents to choose a default report export format while
balancing reviewability, stakeholder readability, and stable CI diffs.

The structured `target_agents` array drives both fan-out states:

```yaml
target_agents:
  - { id: claude, label: Claude proposal, selector: claude-code[yolo]:anthropic:claude-opus-4-7 }
  - { id: codex,  label: Codex proposal,  selector: codex[workspace]:openai:gpt-5.5 }
  - { id: gemini, label: Gemini proposal, selector: gemini[yolo]:google:gemini-3.1-pro-preview }
```

## Validate

```bash
rhei validate examples/agora-example
rhei run examples/agora-example --dry-run
```

`rhei run --dry-run` stops at the ready splitter task. The point fan-out appears
after `Task split` executes and appends concrete `point-<slug>` tasks.

## Regenerate

```bash
rm -rf examples/agora-example
rhei instantiate crates/rhei-cli/templates/agora \
  --values crates/rhei-cli/templates/agora/.example-values.yaml \
  --output examples/agora-example
```

After regenerating, restore this README and `instantiation-values.yaml` from the
checked-in copy if you want to keep the example metadata alongside the rendered
workspace.
