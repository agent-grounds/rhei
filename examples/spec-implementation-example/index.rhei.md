# Rhei: Spec Implementation Example (current-branch spec diff)

## What this workspace does

Implements one or more specifications end-to-end. Every spec goes through:

1. **Implementation** by `claude-code[yolo]:anthropic:claude-opus-4-7`.
2. **Completeness audit** — every reviewer independently checks for missing
   coverage; `codex[xhigh]:openai:gpt-5.5` merges the findings; `claude-code[yolo]:anthropic:claude-opus-4-7`
   closes the gaps.
3. **Quality review/fix loop** — `2` cycles per spec:
   - every reviewer reviews in parallel,
   - `codex[xhigh]:openai:gpt-5.5` writes a fix plan,
   - `codex[xhigh]:openai:gpt-5.5` applies the accepted fixes.

After every per-spec pipeline completes, a **shared end-to-end coverage loop**
runs once across all implemented specs: `claude-code[yolo]:anthropic:claude-opus-4-7` writes tests
(targeting the mock agent `mock` by default), `codex[xhigh]:openai:gpt-5.5`
re-runs the standard suite and audits the new tests. The loop runs for
`2` cycles.

## Input mode

Exactly one of these must be set when instantiating:

- `spec_path` — single-spec mode. One spec file gets one per-spec task.
- `spec_ref` — multi-spec mode. A reference (PR / branch / commit range /
  diff file) whose changed `*.spec.md` files each get their own per-spec task.
  All per-spec tasks share one e2e loop at the end.

This instantiation has:

- `spec_path` = `(empty)`
- `spec_ref`  = `main..HEAD`

The coordinator task verifies the XOR at the start of the run and fails fast
if both or neither are set.

## Configuration

| Role | Target |
|---|---|
| Implementer (per spec) | `claude-code[yolo]:anthropic:claude-opus-4-7` |
| Reviewers (fan-out, per spec) | `claude-code[yolo]:anthropic:claude-opus-4-7`, `codex[xhigh]:openai:gpt-5.5` |
| Smart target (coordinator, aggregate, fix) | `codex[xhigh]:openai:gpt-5.5` |
| E2E writer (shared loop) | `claude-code[yolo]:anthropic:claude-opus-4-7` |
| E2E verifier (shared loop) | `codex[xhigh]:openai:gpt-5.5` |

Quality loop cycles per spec: **2** &nbsp;·&nbsp; E2E loop cycles: **2**

Quality reviewers must address these focus areas:
- `performance`
- `error handling`
- `concurrency`

End-to-end tests live under `e2e`.

## E2E test policy

Every newly-added e2e test MUST target the mock agent (`mock`),
which returns canned outputs the test controls. The standard suite stays
fast, deterministic, and offline.

Tests that exercise real agent operations are reserved for a small
release-only subset, marked with `release-only` so CI can exclude
them from the default test command and include them only in release builds. Those tests live under
`e2e/release-only`. The rule of thumb is one
happy-path test per distinct real-agent integration — the verifier flags
growth beyond that.

## Where work happens

This workspace is a **scratchpad**. Every state resolves the repository root
with `git rev-parse --show-toplevel` and applies code edits in the repository
checkout. Runtime artifacts (`runtime/...`) and dynamic per-spec task files
(`tasks/...`) stay under this workspace.

## Notes

- The workspace is "living": the coordinator appends per-spec implementation
  task files and the shared e2e-aggregate task file under `tasks/` during
  the run. `rhei reset` clears state but does not delete dynamically
  appended task files.
- Instantiate inside the repository being worked on, ideally under
  `.agent-grounds/scratchpad/`, so `git rev-parse --show-toplevel` from the
  workspace resolves the project root deterministically.