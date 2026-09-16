# changeset-review

Review a code change (PR, branch, commit, commit range, or diff file) with a
two-agent review loop: independent review, smart aggregation, independent
validation, independent fix proposals, smart adjudication, and smart final
fixing.

The reusable halves are also shipped as the independently discoverable
`code-review` and `fix` blocks. They can be recomposed with
`--pass review.decision=fix.decision`; the established `changeset-review`
command remains the compatibility surface for existing callers.

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
you want every default review pass to use only xhigh-capable targets.

## State Machine

The wrapper compiles the state fragments from
[`code-review`](../code-review/states.yaml) and [`fix`](../fix/states.yaml),
then applies its checked compatibility map so existing state and artifact names
remain stable. `compatibility.terminals` checks that both child terminal pairs
have matching effective contracts before sharing public `completed`/`cancelled`
identities. Cancellation remains available at the human approval gate.

The manifest's opt-in `select: |` scalar selects only `ports`, `data`, or
`compatibility` after resolving its static inputs. Here it omits mappings for
preparation/commit stages when the respective input is `none`; the reusable
`fix` block selects matching entry/data ports. No placeholder state or extra
workspace/commit artifact is required. The static input schema, mounts, binds,
and seams are unchanged. See §FS-rhei-library.1.1 and §FS-rhei-library.7.1–2.

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

The equivalent reusable block boundary is:

```bash
rhei instantiate \
  --mount review=code-review --mount fix=fix \
  --set review.change_ref=PR#42 \
  --seam review.done=fix.entry \
  --pass review.decision=fix.decision
```

## Example

A pre-rendered example lives at [`examples/changeset-review-example/`](../../../../examples/changeset-review-example/)
and passes `rhei validate` as shipped.

Regenerate it with the command in the example README after changing the
compatibility template; regenerate direct block output with the command above
when changing either extracted block.
