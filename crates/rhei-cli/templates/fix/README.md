# `fix`

`fix` is the application half of `changeset-review`. It contributes the states
that prepare a workspace, apply and verify an approved decision, and optionally
record a commit or pull request — and no tickets of its own, because the ticket
that walks them is the one the review already started. A host that chains it
after a review writes the edge into its entry state and a profile whose
`allowed` spans both halves.

It reads the decision at `runtime/decisions/{task_id}-final-decision.md`, which
is the path `code-review`'s `decide` writes: an artifact that crosses two
templates is a path both sides name, not a mechanism.

```sh
# Composed, as `changeset-review` composes it:
rhei instantiate changeset-review HEAD~3

# Or its states added to a plan that already has a review in it:
rhei instantiate fix --into release
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
