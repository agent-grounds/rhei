# Rhei: {{spec_title}}

## What this workspace does

Implements one or more specifications end-to-end. Every spec goes through:

1. **Implementation** by `{{implementation_target}}`.
2. **Completeness audit** — every reviewer independently checks for missing
   coverage; `{{smart_target}}` merges the findings; `{{implementation_target}}`
   closes the gaps.
3. **Quality review/fix loop** — `{{review_passes}}` cycles per spec:
   - every reviewer reviews in parallel,
   - `{{smart_target}}` writes a fix plan,
   - `{{smart_target}}` applies the accepted fixes.

After every per-spec pipeline completes, a **shared end-to-end coverage loop**
runs once across all implemented specs: `{{e2e_writer}}` writes tests
(targeting the mock agent `{{mock_agent}}` by default), `{{e2e_verifier}}`
re-runs the standard suite and audits the new tests. The loop runs for
`{{e2e_passes}}` cycles.

## Input mode

Exactly one of these must be set when instantiating:

- `spec_path` — single-spec mode. One spec file gets one per-spec task.
- `spec_ref` — multi-spec mode. A reference (PR / branch / commit range /
  diff file) whose changed `*.spec.md` files each get their own per-spec task.
  All per-spec tasks share one e2e loop at the end.

This instantiation has:

- `spec_path` = `{{ spec_path or '(empty)' }}`
- `spec_ref`  = `{{ spec_ref or '(empty)' }}`

The coordinator task verifies the XOR at the start of the run and fails fast
if both or neither are set.

## Configuration

| Role | Target |
|---|---|
| Implementer (per spec) | `{{implementation_target}}` |
| Reviewers (fan-out, per spec) | {% for t in review_targets %}`{{ t }}`{% if not loop.last %}, {% endif %}{% endfor %} |
| Smart target (coordinator, aggregate, fix) | `{{smart_target}}` |
| E2E writer (shared loop) | `{{e2e_writer}}` |
| E2E verifier (shared loop) | `{{e2e_verifier}}` |

Quality loop cycles per spec: **{{review_passes}}** &nbsp;·&nbsp; E2E loop cycles: **{{e2e_passes}}**
{%- if focus_areas %}

Quality reviewers must address these focus areas:

{%- for f in focus_areas %}
- `{{ f }}`
{%- endfor %}
{%- endif %}
{%- if e2e_test_root %}

End-to-end tests live under `{{e2e_test_root}}`.
{%- endif %}

## E2E test policy

Every newly-added e2e test MUST target the mock agent (`{{mock_agent}}`),
which returns canned outputs the test controls. The standard suite stays
fast, deterministic, and offline.

Tests that exercise real agent operations are reserved for a small
release-only subset, marked with `{{release_only_marker}}` so CI can exclude
them from the default test command and include them only in release builds.
{%- if release_only_test_root %} Those tests live under
`{{release_only_test_root}}`.{% endif %} The rule of thumb is one
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
