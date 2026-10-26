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

## Codex sandbox

This block's `settings.json` declares `fix-codex`, the agent `smart_target`
defaults to. It writes `family: codex`, so it takes the built-in profile's
writable roots, and it has one mode, `xhigh`, which runs unattended
(`approval_policy="never"`) at that reasoning effort with network on and writes
only under those roots: the plan and the Panta project around it, the checkout
the state runs in with its git directories, and Rhei's own state directory
(§FS-rhei-agents.2.2). When `fix_prepare` prepares a separate checkout,
`prepare-workspace` records it at `runtime/worktree-refs/{task_id}.yaml`, so
`final-fix` and `commit-fix` run in that checkout and are granted it. It asks
for no `danger-full-access`, because a managed ChatGPT workspace refuses it, and
the worker then falls back to read-only, exits `0` and writes nothing.

**Migrating.** A plan laid with an earlier Rhei keeps the settings it was laid
with, and with them `danger-full-access`. In a plan laid now, a Codex state
cannot write outside those roots: not a build cache such as `~/.cargo`,
`~/.gradle` or `~/.npm`, and not another repository. Grant a directory by
appending it to `fix-codex`'s `xhigh` line in the plan's
`.agent-grounds/rhei/settings.json`, or in the Panta project's copy if the
settings were hoisted:

```json
"xhigh": ["--sandbox", "workspace-write",
          "-c", "approval_policy=\"never\"",
          "-c", "sandbox_workspace_write.network_access=true",
          "-c", "model_reasoning_effort=\"xhigh\"",
          "--add-dir", "/home/me/.cargo"]
```

The path must be absolute: Rhei passes mode tokens through untouched, so `~` and
`$HOME` are not expanded. There is no `yolo` mode to select, so the way back is
to put `danger-full-access` in place of `workspace-write` on that line. It works
only on an account without managed requirements: where they refuse
`danger-full-access`, Codex rust-v0.159.3 stops at startup if they still allow
`approval_policy="never"` and otherwise falls back on both settings and runs
read-only, as earlier releases do; Rhei names each fallback. Where `never` is
refused, every Codex invocation also reports an `approval_policy` fallback,
harmless under `on-request`, since writes inside the granted roots need no
approval there.
