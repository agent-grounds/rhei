# `code-review`

`code-review` is the review half of `changeset-review`. It begins at `split`,
fans out review, validation and proposal passes, and gates the final decision
at `human-review`, which is a non-terminal state with an edge to `completed`:
a gate a chain is continued from cannot be a terminal, and the union that
places this template will not un-finalize one for you.

`decide` writes `runtime/decisions/{task_id}-final-decision.md`, and the
approval gate requires that file before entry. The state names are the ones
written in `states.yaml` wherever this template is placed — `split`, `review`,
`aggregate-reviews`, `validate-review`, `propose-fixes`,
`aggregate-proposals`, `decide`, `human-review` — so a prompt can name a state
rather than send its agent to look one up.

```sh
rhei instantiate code-review HEAD~3 --output ./review
rhei instantiate code-review HEAD~3 --into release.ticket
```

Each of the seven agent states carries `agent_timeout: 1h`, because a state that
resolves no finite timeout is refused by `rhei validate` and by `rhei run`. A
state's own `agent_timeout` is the top of the resolution chain, so a project
`defaults.agent_timeout` does not override it; the bound is set per state rather
than in this block's `settings.json` because that key's ownership is exclusive
at mount time, and `code-review` and `fix` both owning it would collide when
`changeset-review` composes them. To use a different bound, edit `agent_timeout`
on the state in the rendered `states.yaml`.
