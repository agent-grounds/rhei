# changeset-review

Review a code change (PR, branch, commit, commit range, or diff file) with a
two-agent review loop: independent review, smart aggregation, independent
validation, independent fix proposals, smart adjudication, and smart final
fixing.

Both halves are shipped as independently discoverable templates,
`code-review` and `fix`, which this one lists under `includes:`. Each can be
instantiated on its own or placed into a plan that already exists; this
template is the composition, and its name, inputs, ticket ids and artifact
paths are what they have always been.

Instantiate this workspace inside the repository being reviewed. The
instantiated directory is a scratchpad, not the source tree itself, so agents
must resolve the Git toplevel first and inspect or edit files from there (or
from any prepared worktree/fork the workflow creates later).

## Inputs

| Input | Type | Default | Description |
|---|---|---|---|
| `change_ref` | string | *(required)* | PR URL/number, branch, commit SHA, `base..head` range, or `.diff`/`.patch` file path |
| `review_targets` | string[] | `[claude-code[yolo]:anthropic:claude-opus-4-7, codex[xhigh]:openai:gpt-5.5]` | Execution targets that independently review each part |
| `validation_targets` | string[] | `[claude-code[yolo]:anthropic:claude-opus-4-7, codex[xhigh]:openai:gpt-5.5]` | Execution targets that validate whether aggregated review issues are correct |
| `proposal_targets` | string[] | `[claude-code[yolo]:anthropic:claude-opus-4-7, codex[xhigh]:openai:gpt-5.5]` | Execution targets that independently propose fixes |
| `review_focus` | string[] | `[]` | Optional focus subsections each reviewer must address |
| `smart_target` | string | `codex[xhigh]:openai:gpt-5.5` | Smart target for aggregation, discrepancy adjudication, final fixes, and optional commit |
| `fix_prepare` | string | `none` | Optional pre-fix workspace isolation: `none`, `branch`, `worktree`, `fork` |
| `fix_commit` | string | `none` | Optional post-fix commit step: `none`, `commit`, `push`, `pr` |

The template ships a project `.agent-grounds/rhei/settings.json` that adds `high` and
`xhigh` Codex modes. The default GPT-5.5 target uses `xhigh` reasoning effort.
Claude Code remains available as a second default reviewer, but Rhei does not
currently expose a Claude reasoning-effort flag; override the target arrays if
you want every default review pass to use only xhigh-capable targets. Both
composed blocks bound their agent states per state with `agent_timeout: 1h`
rather than through `defaults.agent_timeout`, whose ownership is exclusive at
mount time and so cannot be claimed by two blocks at once; change a bound by
editing `agent_timeout` on the state in the rendered `states.yaml`.

## State Machine

This template is built out of [`code-review`](../code-review/states.yaml) and
[`fix`](../fix/states.yaml), which it lists under `includes:`. Their states,
edges, profiles and routes join its own by graph union, under the names their
authors wrote: the machine carries `split`, `review`, `human-review` and
`final-fix`, not an alias-encoded spelling of them, so a prompt can name a
state instead of being sent to look one up.

What this template owns is the part neither half knows: the edge out of
`code-review`'s `human-review` gate into `fix`'s entry, and the profile whose
`allowed` spans both, mapped by `by_type` to the `task` kind the coordinate
ticket carries. `code-review` makes its own gate non-terminal and gives it an
edge to `completed`, because a union never un-finalizes a terminal — its author
does. The two templates' `completed` and `cancelled` coalesce by role rather
than by spelling, and their `settings.json` join by key, so each contributes its
own agent. Cancellation remains available at the human approval gate.

When `fix_prepare` or `fix_commit` is `none`, `fix` renders without the
corresponding states and edges, so no placeholder state or extra
workspace/commit artifact appears. The input schema a caller passes is
unchanged. See [§FS-rhei-library.3](../../../../docs/functional-spec/rhei-library.spec.md#3-the-union-rules)
and [§FS-rhei-library.6](../../../../docs/functional-spec/rhei-library.spec.md#6-includes-a-template-built-from-templates).

Per-task paths through the machine:

| Task | Path through the machine |
|---|---|
| coordinator | `split` -> `completed` |
| `review-<slug>` | `review` -> `completed` |
| `aggregate` | `aggregate-reviews` -> `validate-review` -> `propose-fixes` -> `aggregate-proposals` -> `decide` -> `human-review` -> `[prepare-workspace]` -> `final-fix` -> `[commit-fix]` -> `completed` |

## Flow

1. The coordinator resolves `change_ref`, writes an architectural overview,
   splits the change into logical review parts, and appends one `review-<slug>`
   task per part plus one `aggregate` task waiting on all reviews.
2. Each part is reviewed once per configured `review_targets`.
3. `smart_target` aggregates review findings into candidate issues.
4. `validation_targets` independently classify each candidate issue as valid,
   invalid, or needing a decision.
5. `proposal_targets` independently propose concrete fixes for validated or
   disputed issues.
6. `smart_target` aggregates those proposals into a proposal matrix.
7. `smart_target` resolves discrepancies, rejects unsupported issues, and
   writes the final fix plan.
8. A human reviews the final fix plan and explicitly approves the fix phase.
9. `smart_target` applies accepted fixes, optionally in an isolated workspace
   and optionally followed by a commit/push/PR step.

## Instantiate

```bash
rhei instantiate changeset-review \
  --set change_ref=PR#42 \
  --set review_targets='["claude-code[yolo]:anthropic:claude-opus-4-7","codex[xhigh]:openai:gpt-5.5"]' \
  --set review_focus='["performance","security","concurrency"]' \
  --output ./.agent-grounds/scratchpad/changeset-review/
```

Either half can also be used on its own, or placed into a plan that already
exists:

```bash
rhei instantiate code-review PR#42 --output ./review
rhei instantiate code-review PR#42 --into release.ticket
```

## Example

A pre-rendered example lives at [`examples/changeset-review-example/`](../../../../examples/changeset-review-example/)
and passes `rhei validate` as shipped.

Regenerate it with the command in the example README after changing the
compatibility template; regenerate direct block output with the command above
when changing either extracted block.
