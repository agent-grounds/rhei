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

## Codex sandbox

This block's `settings.json` gives `codex` one mode, `xhigh`. It runs unattended
(`approval_policy="never"`) at that reasoning effort with network on, and writes
only under the roots Rhei grants at launch: the plan and the Panta project
around it, the checkout the state runs in with its git directories, and Rhei's
own state directory (§FS-rhei-agents.2.2). It asks for no `danger-full-access`,
because a managed ChatGPT workspace refuses it, and the worker then falls back
to read-only, exits `0` and writes nothing.

**Migrating.** A plan laid with an earlier Rhei keeps the settings it was laid
with, and with them `danger-full-access`. In a plan laid now, a Codex state
cannot write outside those roots: not a build cache such as `~/.cargo`,
`~/.gradle` or `~/.npm`, and not another repository. Grant a directory by
appending it to the `xhigh` line in the plan's
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
only on an account without managed requirements: under them, Codex rust-v0.159.3
and later stop at startup when given `danger-full-access` with
`approval_policy="never"`, and earlier releases fall back to read-only.
