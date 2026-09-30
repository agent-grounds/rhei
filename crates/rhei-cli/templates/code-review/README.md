# Code review block

`code-review` is the reusable review half of `changeset-review`. It begins at
`split`, retains the review/validation/proposal fan-out states, gates the final
decision at `human-review`, and exposes the decision written by `decide` as
the `decision` state-file output. The approval gate requires that file before
entry. State prompt files retain the review duties and resolve mounted state
names and artifact paths from the compiled machine.

```sh
rhei instantiate --mount review=code-review \
  --set review.change_ref=HEAD~3
```

Each of the seven agent states carries `agent_timeout: 1h`, because a state that
resolves no finite timeout is refused by `rhei validate` and by `rhei run`. A
state's own `agent_timeout` is the top of the resolution chain, so a project
`defaults.agent_timeout` does not override it; the bound is set per state rather
than in this block's `settings.json` because that key's ownership is exclusive
at mount time, and `code-review` and `fix` both owning it would collide when
`changeset-review` composes them. To use a different bound, edit `agent_timeout`
on the state in the rendered `states.yaml`.
