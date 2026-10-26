# multi-model-analysis — example

A pre-rendered instantiation of the
[`multi-model-analysis`](../../crates/rhei-cli/templates/multi-model-analysis/)
template, used as a smoke test that the template produces a valid workspace.
This is a canonical **multi-target fan-out** reference: one `analyze` state
fans out across every entry in the `agents` array (one pass per target,
runnable in parallel), then a single `summarize` state synthesizes the notes.

## Inputs used

The full input set is checked in at `instantiation-values.yaml`. The structured
`agents` array drives the fan-out:

```yaml
agents:
  - { id: claude, label: Claude analysis note, selector: claude-code[yolo]:anthropic:claude-opus-4-7 }
  - { id: gemini, label: Gemini analysis note, selector: gemini[yolo]:google:gemini-3.1-pro-preview }
  - { id: codex,  label: Codex analysis note,  selector: codex[workspace]:openai:gpt-5-codex }
```

The dry-run below shows one `analyze → summarize` transition per target, which
is the fan-out the `all_targets` state produces.

## Validate

```bash
rhei validate examples/multi-model-analysis-example
rhei run examples/multi-model-analysis-example --dry-run
# parallel execution (one agent per target at once):
rhei run examples/multi-model-analysis-example --parallel 3 --dry-run
```

## Regenerate

```bash
rm -rf examples/multi-model-analysis-example
rhei instantiate crates/rhei-cli/templates/multi-model-analysis \
  --values crates/rhei-cli/templates/multi-model-analysis/.example-values.yaml \
  --output examples/multi-model-analysis-example
```

After regenerating, restore this README and `instantiation-values.yaml` from the
checked-in copy if you want to keep the example metadata alongside the rendered
workspace.
