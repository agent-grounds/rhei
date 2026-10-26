# spec-review

A small, self-contained **counted review/fix loop** for a single
specification file. Point it at a spec and it runs two review passes — each
followed by a fix pass that applies the findings directly to the spec — then
stops. This is the simplest template in the set and the canonical reference for
the *loop* pattern (`visits: N` + a `visitCount`-conditioned transition).

## What it does

One task walks a four-state machine: `review` writes findings for the `spec` to
a per-pass artifact, `fix` reads those findings and edits the spec, and the two
states each declare `visits: 2`, so the task loops `review → fix → review → fix`
and then transitions to `completed`. A wildcard `* → cancelled` lets a human
abort from any state.

## When to use it

- You have one written spec and want a bounded, hands-off review-and-fix pass
  over it.
- You want a minimal example of the counted-loop shape to copy into a larger
  template. For multi-spec, multi-agent, or aggregation work, reach for
  `spec-implementation`, `changeset-review`, or `multi-model-analysis` instead.

## Inputs

| Input | Type | Default | What it does |
|---|---|---|---|
| `spec` | string | *(required)* | The specification file to review. Rendered exactly as supplied, so use a path the spawned agent can read from the instantiated workspace. |
| `criteria` | string | empty | Extra things each review pass must look for (e.g., `thread safety, backward compatibility`). Empty means a general consistency/completeness/correctness/clarity review. |

`spec` is positional-friendly via the single-required-input fallback:
`rhei instantiate spec-review docs/.../my.spec.md` works without `--set`.

## Flow at a glance

```
review [initial]  --(write findings)-->  fix
   ^                                       |
   |  visitCount < visits                  |
   +---------------------------------------+
                                           |
                       visitCount >= visits
                                           v
                                     completed [final]

any non-final state --> cancelled [final]
```

The full state machine diagram (with per-task paths) lives at the top of
[`states.yaml`](./states.yaml).

Per-task path:

| Task | Path through the machine |
|---|---|
| `spec-review` | `review → fix → review → fix → completed` (2 passes) |

## Codex sandbox

This template's `review` and `fix` states run `codex` with `agent_mode:
workspace`: the built-in profile's mode that runs unattended
(`approval_policy="never"`) with network on and writes only under the roots Rhei
grants at launch, which are the plan and the Panta project around it, the
checkout the state runs in with its git directories, and Rhei's own state
directory (§FS-rhei-agents.2.2). The built-in's other mode, `yolo`, writes
anywhere (`--sandbox danger-full-access`), but a managed ChatGPT workspace
refuses it, and the worker then falls back to read-only, exits `0` and writes
nothing.

**Migrating.** A plan laid with an earlier Rhei keeps the settings it was laid
with, and with them `danger-full-access`. In a plan laid now, a Codex state
cannot write outside those roots: not a build cache such as `~/.cargo`,
`~/.gradle` or `~/.npm`, and not another repository. This template ships no
Codex profile, so grant a directory with one: an `agents` entry beside
`defaults` in the plan's `.agent-grounds/rhei/settings.json`, or in the Panta
project's copy if the settings were hoisted, that keeps the built-in's family:

```json
"agents": {
  "codex": {
    "family": "codex",
    "modes": {
      "yolo": ["--sandbox", "danger-full-access", "--skip-git-repo-check",
               "-c", "approval_policy=\"never\""],
      "workspace": ["--sandbox", "workspace-write", "--skip-git-repo-check",
                    "-c", "approval_policy=\"never\"",
                    "-c", "sandbox_workspace_write.network_access=true",
                    "--add-dir", "/home/me/.cargo"]
    }
  }
}
```

`modes` is replaced whole, so `yolo` is restated to keep it; `writable_roots` is
still inherited from the built-in. The path must be absolute: Rhei passes mode
tokens through untouched, so `~` and `$HOME` are not expanded. If settings of
your own replace `codex` by id, give that entry the same `family` and
`workspace` mode: without the mode a plan laid from this template is refused at
`rhei validate`, and without the family it is granted no roots unless it writes
`writable_roots` itself. The way back is `agent_mode: yolo` on both states in
the rendered `states.yaml`. It works only on an account without managed
requirements: where they refuse `danger-full-access`, Codex rust-v0.159.3 stops
at startup if they still allow `approval_policy="never"` and otherwise falls
back on both settings and runs read-only, as earlier releases do; Rhei names
each fallback. Where `never` is refused, every Codex invocation also reports an
`approval_policy` fallback, harmless under `on-request`, since writes inside the
granted roots need no approval there.

## Quick start

```bash
rhei instantiate spec-review \
  --set spec=docs/functional-spec/rhei-templates.spec.md \
  --set criteria="thread safety, backward compatibility" \
  --output .agent-grounds/scratchpad/spec-review/

rhei run .agent-grounds/scratchpad/spec-review/
```

## What's bundled

- `template.yaml` — the two inputs.
- `states.yaml` — the `spec-review` counted-loop machine (diagram in the top
  comment). Reviews run on the built-in `codex` agent's `workspace` mode.
- `settings.json` — project defaults with `agent_timeout: 20m`, so the quick
  start runs without requiring a global timeout setting.
- `index.rhei.md` — workspace index.
- `tasks/01-review.md` — the single review task skeleton.

## Example

A pre-rendered example lives at
[`examples/spec-review-example/`](../../../../examples/spec-review-example/)
and passes `rhei validate` as shipped.
