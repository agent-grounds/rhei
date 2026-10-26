# spec-implementation - example (multi-spec / diff mode)

A pre-rendered instantiation of the
[`spec-implementation`](../../crates/rhei-cli/templates/spec-implementation/)
template used as a smoke test that the template produces a valid workspace.

This example exercises **multi-spec mode** (`spec_ref`) — the coordinator
would resolve the ref to the `*.spec.md` files changed on the current
branch and spawn one per-spec implementation task per file, plus one
shared e2e-aggregate task.

## Inputs used

```yaml
spec_ref: main..HEAD
spec_title: Spec Implementation Example (current-branch spec diff)
implementation_target: claude-code[yolo]:anthropic:claude-opus-4-7
review_targets:
  - claude-code[yolo]:anthropic:claude-opus-4-7
  - codex[xhigh]:openai:gpt-5.5
smart_target: codex[xhigh]:openai:gpt-5.5
review_passes: 2
focus_areas:
  - performance
  - error handling
  - concurrency
e2e_writer: claude-code[yolo]:anthropic:claude-opus-4-7
e2e_verifier: codex[xhigh]:openai:gpt-5.5
e2e_passes: 2
e2e_test_root: e2e
mock_agent: mock
release_only_marker: release-only
release_only_test_root: e2e/release-only
```

The same values are checked in at `instantiation-values.yaml`.

To exercise single-spec mode instead, swap `spec_ref` for
`spec_path: docs/functional-spec/<your-spec>.spec.md`.

## Validate

```bash
rhei validate examples/spec-implementation-example
rhei run examples/spec-implementation-example --dry-run
```

## Regenerate

```bash
rm -rf examples/spec-implementation-example
rhei instantiate crates/rhei-cli/templates/spec-implementation \
  --values crates/rhei-cli/templates/spec-implementation/.example-values.yaml \
  --output examples/spec-implementation-example
```

After regenerating, restore this README and the checked-in
`instantiation-values.yaml` if the generator overwrote them.
