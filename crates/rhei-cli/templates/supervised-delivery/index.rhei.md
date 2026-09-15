# Rhei: {{title}}
**States:** supervised-delivery

## What this workspace does

One task delivers `{{spec_path}}`, and it is the only task with an opinion:
`Task deliver` sits in `supervising`, a state that declares
`execute_on: child-terminal`. That makes it a **supervisor** — the orchestrator
wakes it after every child that finishes and holds the rest of the subtree in
between, so the pipeline below is a set of steps the supervisor *sends*, not a
conveyor belt that runs on its own.

```text
supervisor prepares
    -> implement
    -> [ code review  ||  product review ] -> fix     x {{review_rounds}} round(s)
    -> coverage audit -> fix                          x {{coverage_rounds}} round(s)
    -> documentation                                  x {{docs_rounds}} round(s)
    -> supervisor writes the delivery result
```

Every round is a real task in `tasks/01-deliver.md`, unrolled at instantiation.
The supervisor cancels the rounds the results made unnecessary, so the numbers
above are ceilings rather than a schedule.

## The release gate is the brief

Every child state declares a **required** input at
`runtime/supervise/<task-id>.md`. That file is the supervisor's brief, and no
child runs until the supervisor has written it. The subtree is therefore
dispatched one decision at a time: the supervisor reads what the last step left
behind, decides what happens next, writes the brief for it, and returns — and
the engine releases exactly the steps that now have one.

The only pair the supervisor briefs together is the code review and the product
review of the same round. Run with `--parallel 2` (or more) so they overlap:

```bash
rhei run . --parallel 2
```

## The structured channel is plan exports

Steps hand each other work product through `**Provides:**` / `**Consumes:**`
exports, not through prose. These declarations select prompt data-flow, not
filesystem visibility: workers can still read undeclared sibling exports under
`runtime/exports/`. Concurrent scheduling and explicit briefs can reduce
accidental cross-reading, but cannot enforce blindness once an export exists.
Each export is one file holding exactly one fenced `json` block, at
`runtime/exports/<task-id>/<name>.md`:

| Export | Written by | Read by |
|---|---|---|
| `report` | `implement`, `docs-*` | every reviewer, the coverage audit, the documentation round |
| `findings` | `review-*`, `pm-*` | the fixer of that round, and the next round's reviewer |
| `resolutions` | `fix-*`, `coverage-fix-*` | the next round's reviewer, the documentation round |
| `gaps` | `coverage-*` | that round's fixer |

The same paths are declared as each state's `outputs:`, so a step cannot reach
a terminal state without having written its export. The schemas live in this workspace's [`README.md`](README.md) and in the
prompt fragments under `prompt_templates/`.

## Configuration

| Role | Target |
|---|---|
| Supervisor | `{{supervisor_target}}` |
| Implementer | `{{implementer_target}}` |
| Code reviewer | `{{reviewer_target}}` |
| Product reviewer | `{{pm_target}}` |
| Fixer (reviews and coverage) | `{{fixer_target}}` |
| Coverage auditor | `{{coverage_target}}` |
| Documentation | `{{docs_target}}` |

Round ceilings: **{{review_rounds}}** review · **{{coverage_rounds}}** coverage · **{{docs_rounds}}** documentation
{%- if review_focus %}

Every code review must answer these focus areas:
{% for f in review_focus %}
- `{{ f }}`
{%- endfor %}
{%- endif %}
{%- if ci_commands %}

These commands must be green before a fix, coverage, or documentation step
reports success:
{% for c in ci_commands %}
- `{{ c }}`
{%- endfor %}
{%- endif %}

## Session continuity

{% if supervisor_session -%}
`supervisor_session` is **true**, so the supervising state emits and inherits a
snapshot named `supervisor` from itself: every visit continues the previous
transcript. This requires `supervisor_target` to name a session-capable agent
that resolves both a provider and a model — of the built-in profiles that is
`pi` today.
{%- else -%}
`supervisor_session` is **false**, so the supervisor runs each visit cold. That
is the supported shape for `claude-code`, `codex`, `gemini`, `cursor`, and
`kilocode`, which reject a snapshot block outright. Nothing is lost that the
workspace does not already carry: every visit is handed `## Checkpoints` —
what moved and what it left behind — and the preparation note the supervisor
wrote on its first visit. Set `supervisor_session=true` only with a
session-capable target such as `pi`.
{%- endif %}

## Where work happens

This workspace is a **scratchpad**. Every state resolves the repository root
with `git rev-parse --show-toplevel` and edits code there. Runtime artifacts —
briefs, exports, results, the preparation note — stay under this workspace.

## Notes

- `rhei run` releases the subtree only between supervisor visits, so a run with
  `--parallel 1` still works; it just serializes the two reviews of a round and
  costs the supervisor one extra visit per round.
- The supervisor cancels with `rhei transition <id> --from <current-state>
  --to cancelled --result "<why>"`. `--from` is the compare-and-swap guard and
  is required; the child's current state is the one in brackets beside it in
  the supervisor's task list. A cancelled ticket does not satisfy anyone's
  prerequisite, which is why the later phases are chained to the round-1
  fixers (`fix-1`, `coverage-fix-1`) — the two steps the supervisor never
  cancels.
