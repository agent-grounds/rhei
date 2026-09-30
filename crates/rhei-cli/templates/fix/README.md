# Fix block

`fix` is the reusable application half of `changeset-review`. It accepts the
review block's `decision` state-file endpoint, prepares a workspace, applies and
verifies the fix, and optionally records a commit or pull request. It contributes
states to the task supplied by the preceding block rather than creating a
second plan task.

```sh
rhei instantiate \
  --mount review=code-review --mount fix=fix \
  --set review.change_ref=HEAD~3 \
  --seam review.done=fix.entry \
  --pass review.decision=fix.decision
```

Each agent state carries `agent_timeout: 1h` — `final-fix`, plus
`prepare-workspace` and `commit-fix` where `fix_prepare` and `fix_commit` render
them — because a state that resolves no finite timeout is refused by
`rhei validate` and by `rhei run`. A state's own `agent_timeout` is the top of
the resolution chain, so a project `defaults.agent_timeout` does not override
it; the bound is set per state rather than in this block's `settings.json`
because that key's ownership is exclusive at mount time, and `fix` and
`code-review` both owning it would collide when `changeset-review` composes
them. To use a different bound, edit `agent_timeout` on the state in the
rendered `states.yaml`.
